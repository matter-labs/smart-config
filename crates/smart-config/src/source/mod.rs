use std::{collections::BTreeSet, marker::PhantomData, mem, sync::Arc};

pub use self::{env::Environment, json::Json, yaml::Yaml};
use crate::{
    de::{DeserializeContext, DeserializerOptions},
    metadata::{BasicTypes, ConfigMetadata},
    schema::{Alias, ConfigRef, ConfigSchema},
    value::{Map, Pointer, Value, ValueOrigin, WithOrigin},
    DeserializeConfig, ParseError, ParseErrors,
};

#[macro_use]
mod macros;
mod env;
mod json;
#[cfg(test)]
mod tests;
mod yaml;

/// Contents of a [`ConfigSource`].
#[derive(Debug)]
#[non_exhaustive]
pub enum ConfigContents {
    /// Key–value / flat configuration.
    KeyValue(Map<String>),
    /// Hierarchical configuration.
    Hierarchical(Map),
}

/// Source of configuration parameters that can be added to a [`ConfigRepository`].
pub trait ConfigSource {
    /// Returns the origin of the entire source (e.g., [`ValueOrigin::File`] for JSON and YAML files).
    fn origin(&self) -> Arc<ValueOrigin>;
    /// Converts this source into config contents.
    fn into_contents(self) -> ConfigContents;
}

/// Configuration repository containing zero or more [configuration sources](ConfigSource).
/// Sources are preprocessed and merged according to the provided [`ConfigSchema`].
///
/// # Type coercion
///
/// When processing [`ConfigSource`]s, values can be *coerced* depending on the [expected type](BasicType)
/// at the corresponding location [as indicated](crate::de::DeserializeParam::expecting()) by the param deserializer.
/// Currently, coercion only happens if the original value is a string.
///
/// - If the expected type is [`BasicType::Integer`], [`BasicType::Float`], or [`BasicType::Bool`],
///   the number / Boolean is [parsed](str::parse()) from the string. If parsing succeeds, the value is replaced.
/// - If the expected type is [`BasicType::Array`] or [`BasicType::Object`], then the original string
///   is considered to be a JSON array / object. If JSON parsing succeeds, and the parsed value has the expected shape,
///   then it replaces the original value.
///
/// Coercion is not performed if the param deserializer doesn't specify an expected type.
///
/// This means that it's possible to supply values for structured params from env vars without much hassle:
///
/// ```rust
/// # use std::collections::HashMap;
/// use smart_config::{testing, DescribeConfig, DeserializeConfig, Environment};
///
/// #[derive(Debug, DescribeConfig, DeserializeConfig)]
/// struct CoercingConfig {
///     flag: bool,
///     ints: Vec<u64>,
///     map: HashMap<String, u32>,
/// }
///
/// let env = Environment::from_iter("APP_", [
///     ("APP_FLAG", "true"),
///     ("APP_INTS", "[2, 3, 5]"),
///     ("APP_MAP", r#"{ "value": 5 }"#),
/// ]);
/// // `testing` functions create a repository internally
/// let config: CoercingConfig = testing::test(env)?;
/// assert!(config.flag);
/// assert_eq!(config.ints, [2, 3, 5]);
/// assert_eq!(config.map, HashMap::from([("value".into(), 5)]));
/// # anyhow::Ok(())
/// ```
#[derive(Debug, Clone)]
pub struct ConfigRepository<'a> {
    schema: &'a ConfigSchema,
    de_options: DeserializerOptions,
    all_prefixes_with_aliases: BTreeSet<Pointer<'a>>,
    merged: WithOrigin,
}

impl<'a> ConfigRepository<'a> {
    /// Creates an empty config repo based on the provided schema.
    pub fn new(schema: &'a ConfigSchema) -> Self {
        let all_prefixes_with_aliases: BTreeSet<_> = schema
            .prefixes_with_aliases()
            .flat_map(Pointer::with_ancestors)
            .chain([Pointer("")])
            .collect();
        Self {
            schema,
            de_options: DeserializerOptions::default(),
            all_prefixes_with_aliases,
            merged: WithOrigin {
                inner: Value::Object(Map::default()),
                origin: Arc::default(),
            },
        }
    }

    /// Accesses options used during `serde`-powered deserialization.
    pub fn deserializer_options(&mut self) -> &mut DeserializerOptions {
        &mut self.de_options
    }

    /// Extends this environment with a new configuration source.
    #[must_use]
    pub fn with<S: ConfigSource>(mut self, source: S) -> Self {
        let source_origin = source.origin();
        match source.into_contents() {
            ConfigContents::KeyValue(mut kv) => {
                for &object_ptr in self.all_prefixes_with_aliases.iter().rev() {
                    self.merged
                        .copy_key_value_entries(object_ptr, &source_origin, &mut kv);
                }
            }
            ConfigContents::Hierarchical(map) => {
                self.merged.merge(WithOrigin {
                    inner: Value::Object(map),
                    origin: Arc::default(), // will not be used
                });
            }
        }

        // Copy all globally aliased values.
        for (prefix, config_data) in self.schema.iter() {
            for alias in &config_data.aliases {
                self.merged.merge_alias(prefix, alias);
            }
        }

        // Copy all locally aliased values.
        for (prefix, config_data) in self.schema.iter() {
            let Some(config_object) = self.merged.get_mut(prefix) else {
                continue;
            };
            let Value::Object(config_object) = &mut config_object.inner else {
                // FIXME: is it possible to break invariants (e.g., overwrite objects at mounting points)?
                unreachable!();
            };

            for param in &*config_data.metadata.params {
                if config_object.contains_key(param.name) {
                    continue;
                }

                for &alias in param.aliases {
                    if let Some(alias_value) = config_object.get(alias).cloned() {
                        config_object.insert(param.name.to_owned(), alias_value);
                        break;
                    }
                }
            }
        }

        // Normalize types of all copied values. At this point we only care about canonical names,
        // since any aliases were copied on the previous step.
        for (prefix, config_data) in self.schema.iter() {
            if let Some(config_map) = self.merged.get_mut(prefix) {
                config_map.coerce_value_types(config_data.metadata);
            }
        }

        self
    }

    // TODO: probably makes sense to make public
    pub(crate) fn merged(&self) -> &WithOrigin {
        &self.merged
    }

    /// Returns a parser for the single configuration of the specified type.
    ///
    /// # Errors
    ///
    /// Errors if the config is not a part of the schema or is mounted to multiple locations.
    pub fn single<C: DeserializeConfig>(&self) -> anyhow::Result<ConfigParser<'_, C>> {
        let config_ref = self.schema.single(C::describe_config())?;
        Ok(ConfigParser {
            repo: self,
            config_ref,
            _config: PhantomData,
        })
    }
}

/// Parser of configuration input in a [`ConfigRepository`].
#[derive(Debug)]
pub struct ConfigParser<'a, C> {
    repo: &'a ConfigRepository<'a>,
    config_ref: ConfigRef<'a>,
    _config: PhantomData<C>,
}

impl<C: DeserializeConfig> ConfigParser<'_, C> {
    /// Performs parsing.
    ///
    /// # Errors
    ///
    /// Returns errors encountered during parsing. This list of errors is as full as possible (i.e.,
    /// there is no short-circuiting on encountering an error).
    pub fn parse(self) -> Result<C, ParseErrors> {
        let prefix = self.config_ref.prefix();
        let metadata = self.config_ref.data.metadata;
        let mut errors = ParseErrors::default();
        let context = DeserializeContext::new(
            &self.repo.de_options,
            &self.repo.merged,
            prefix.to_owned(),
            metadata,
            &mut errors,
        );

        C::deserialize_config(context).ok_or_else(|| {
            if errors.len() == 0 {
                errors.push(ParseError::generic(prefix.to_owned(), metadata));
            }
            errors
        })
    }
}

impl WithOrigin {
    /// Ensures that there is an object (possibly empty) at the specified location.
    fn ensure_object(
        &mut self,
        at: Pointer<'_>,
        mut create_origin: impl FnMut(Pointer<'_>) -> Arc<ValueOrigin>,
    ) -> &mut Map {
        for ancestor_path in at.with_ancestors() {
            self.ensure_object_step(ancestor_path, &mut create_origin);
        }

        let Value::Object(map) = &mut self.get_mut(at).unwrap().inner else {
            unreachable!(); // Ensured by calls above
        };
        map
    }

    fn ensure_object_step(
        &mut self,
        at: Pointer<'_>,
        mut create_origin: impl FnMut(Pointer<'_>) -> Arc<ValueOrigin>,
    ) {
        let Some((parent, last_segment)) = at.split_last() else {
            // Nothing to do.
            return;
        };

        // `unwrap()` is safe since `ensure_object()` is always called for the parent
        let parent = &mut self.get_mut(parent).unwrap().inner;
        if !matches!(parent, Value::Object(_)) {
            *parent = Value::Object(Map::new());
        }
        let Value::Object(parent_object) = parent else {
            unreachable!();
        };

        if !parent_object.contains_key(last_segment) {
            parent_object.insert(
                last_segment.to_owned(),
                WithOrigin {
                    inner: Value::Object(Map::new()),
                    origin: create_origin(at),
                },
            );
        }
    }

    fn copy_key_value_entries(
        &mut self,
        at: Pointer<'_>,
        source_origin: &Arc<ValueOrigin>,
        entries: &mut Map<String>,
    ) {
        let matching_entries: Vec<_> = Self::take_matching_kv_entries(entries, at)
            .into_iter()
            .map(|(key, value)| {
                (
                    key,
                    WithOrigin {
                        inner: Value::String(value.inner),
                        origin: value.origin,
                    },
                )
            })
            .collect();

        if matching_entries.is_empty() {
            return;
        }
        let origin = Arc::new(ValueOrigin::Synthetic {
            source: source_origin.clone(),
            transform: format!("nesting kv entries for '{at}'"),
        });
        self.ensure_object(at, |_| origin.clone())
            .extend(matching_entries);
    }

    fn take_matching_kv_entries(
        from: &mut Map<String>,
        prefix: Pointer,
    ) -> Vec<(String, WithOrigin<String>)> {
        let mut matching_entries = vec![];
        let env_prefix = Self::kv_prefix(prefix.0);
        from.retain(|name, value| {
            if let Some(name_suffix) = name.strip_prefix(&env_prefix) {
                let value = mem::take(value);
                matching_entries.push((name_suffix.to_owned(), value));
                false
            } else {
                true
            }
        });
        matching_entries
    }

    /// Converts a logical prefix like `api.limits` to `api_limits_`.
    fn kv_prefix(prefix: &str) -> String {
        let mut prefix = prefix.replace('.', "_");
        if !prefix.is_empty() && !prefix.ends_with('_') {
            prefix.push('_');
        }
        prefix
    }

    fn merge_alias(&mut self, target_prefix: Pointer<'_>, alias: &Alias<()>) {
        let map = self
            .get(target_prefix)
            .and_then(|val| val.inner.as_object());
        let Some((alias_map, alias_origin)) = self
            .get(alias.prefix)
            .and_then(|val| Some((val.inner.as_object()?, &val.origin)))
        else {
            return; // No values at the alias
        };

        let new_entries = alias.param_names.iter().filter_map(|&param_name| {
            if map.map_or(false, |map| map.contains_key(param_name)) {
                None // Variable is already set
            } else {
                let value = alias_map.get(param_name).cloned()?;
                Some((param_name.to_owned(), value))
            }
        });
        let new_entries: Vec<_> = new_entries.collect();
        if new_entries.is_empty() {
            return;
        }

        let origin = Arc::new(ValueOrigin::Synthetic {
            source: alias_origin.clone(),
            transform: format!(
                "copy of '{}' to '{target_prefix}' per aliasing rules",
                alias.prefix
            ),
        });
        self.ensure_object(target_prefix, |_| origin.clone())
            .extend(new_entries);
    }

    /// This is necessary to prevent `deserialize_any` errors
    // TODO: log coercion errors
    fn coerce_value_types(&mut self, metadata: &ConfigMetadata) {
        const STRUCTURED: BasicTypes = BasicTypes::ARRAY.or(BasicTypes::OBJECT);

        let Value::Object(map) = &mut self.inner else {
            unreachable!("expected an object due to previous preprocessing steps");
        };

        for param in &*metadata.params {
            if let Some(value) = map.get_mut(param.name) {
                let Value::String(str) = &value.inner else {
                    continue;
                };

                // Attempt to transform the type to the expected type
                let expecting = param.expecting;
                match expecting {
                    // We intentionally use exact comparisons; if a type supports multiple primitive representations,
                    // we do nothing.
                    BasicTypes::BOOL => {
                        if let Ok(bool_value) = str.parse::<bool>() {
                            value.inner = Value::Bool(bool_value);
                        }
                    }
                    BasicTypes::INTEGER | BasicTypes::FLOAT => {
                        if let Ok(number) = str.parse::<serde_json::Number>() {
                            value.inner = Value::Number(number);
                        }
                    }

                    ty if STRUCTURED.contains(ty) => {
                        let Ok(val) = serde_json::from_str::<serde_json::Value>(str) else {
                            continue;
                        };
                        let is_value_supported = (val.is_array() && ty.contains(BasicTypes::ARRAY))
                            || (val.is_object() && ty.contains(BasicTypes::OBJECT));
                        if is_value_supported {
                            let root_origin = Arc::new(ValueOrigin::Synthetic {
                                source: value.origin.clone(),
                                transform: "parsed JSON string".into(),
                            });
                            *value = Json::map_value(val, &root_origin, String::new());
                        }
                    }
                    _ => { /* Do nothing */ }
                }
            }
        }
    }
}
