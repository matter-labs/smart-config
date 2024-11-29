use std::{collections::BTreeSet, iter, marker::PhantomData, sync::Arc};

pub use self::{env::Environment, json::Json, yaml::Yaml};
use crate::{
    de::{DeserializeContext, DeserializerOptions},
    metadata::BasicTypes,
    schema::{ConfigRef, ConfigSchema},
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
/// # Merging sources
///
/// [`Self::with()`] merges a new source into this repo. The new source has higher priority and will overwrite
/// values defined in old sources, including via parameter aliases.
///
/// # Type coercion
///
/// When processing [`ConfigSource`]s, values can be *coerced* depending on the [expected type](BasicTypes)
/// at the corresponding location [as indicated](crate::de::DeserializeParam::EXPECTING) by the param deserializer.
/// Currently, coercion only happens if the original value is a string.
///
/// - If the expected type is [`BasicTypes::INTEGER`], [`BasicTypes::FLOAT`], or [`BasicTypes::BOOL`],
///   the number / Boolean is [parsed](str::parse()) from the string. If parsing succeeds, the value is replaced.
/// - If the expected type is [`BasicTypes::ARRAY`], [`BasicTypes::OBJECT`], or their union, then the original string
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
        let mut source_value = match source.into_contents() {
            ConfigContents::KeyValue(kv) => {
                let mut value = WithOrigin {
                    inner: Value::Object(Map::default()),
                    origin: source_origin.clone(),
                };
                for &object_ptr in self.all_prefixes_with_aliases.iter().rev() {
                    value.copy_key_value_entries(object_ptr, &source_origin, &kv);
                }
                value
            }
            ConfigContents::Hierarchical(map) => WithOrigin {
                inner: Value::Object(map),
                origin: source_origin,
            },
        };

        source_value.preprocess_source(self.schema);
        self.merged
            .guided_merge(source_value, self.schema, Pointer(""));
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
        let config_ref = self.schema.single(&C::DESCRIPTION)?;
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
    fn preprocess_source(&mut self, schema: &ConfigSchema) {
        self.copy_aliased_values(schema);

        // Coerce types of all copied values. At this point we only care about canonical names,
        // since any aliases were copied on the previous step.
        for (path, expecting) in schema.canonical_params() {
            if let Some(val) = self.get_mut(path) {
                val.coerce_value_type(expecting);
            }
        }
    }

    fn copy_aliased_values(&mut self, schema: &ConfigSchema) {
        for (prefix, config_data) in schema.iter() {
            let canonical_map = match self.get(prefix).map(|val| &val.inner) {
                Some(Value::Object(map)) => Some(map),
                Some(_) => continue, // TODO: log warning
                None => None,
            };

            let alias_maps: Vec<_> = config_data
                .aliases
                .iter()
                .filter_map(|&alias| {
                    let val = self.get(alias)?;
                    Some((val.inner.as_object()?, &val.origin))
                })
                .collect();

            let mut new_values = vec![];
            let mut new_map_origin = None;
            for param in config_data.metadata.params {
                if canonical_map.map_or(false, |map| map.contains_key(param.name)) {
                    continue;
                }

                // Create a prioritized iterator of all candidates
                let local_candidates = canonical_map
                    .into_iter()
                    .flat_map(|map| param.aliases.iter().map(move |&alias| (map, alias, None)));
                let all_names = iter::once(param.name).chain(param.aliases.iter().copied());
                let alias_candidates = alias_maps.iter().flat_map(|&(map, origin)| {
                    all_names.clone().map(move |name| (map, name, Some(origin)))
                });

                // Find the value alias among the candidates
                let maybe_value_and_origin = local_candidates
                    .chain(alias_candidates)
                    .find_map(|(map, name, origin)| Some((map.get(name)?, origin)));
                if let Some((value, origin)) = maybe_value_and_origin {
                    new_values.push((param.name.to_owned(), value.clone()));
                    if new_map_origin.is_none() {
                        new_map_origin = origin.cloned();
                    }
                }
            }

            if new_values.is_empty() {
                continue;
            }

            let new_map_origin = new_map_origin.map(|source| {
                Arc::new(ValueOrigin::Synthetic {
                    source,
                    transform: format!("copy to '{prefix}' per aliasing rules"),
                })
            });
            // `unwrap()` below is safe: if there is no `current_map`, `new_values` are obtained from the alias maps,
            // meaning that `new_map_origin` has been set.
            self.ensure_object(prefix, |_| new_map_origin.clone().unwrap())
                .extend(new_values);
        }
    }

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
        entries: &Map<String>,
    ) {
        let env_prefix = Self::kv_prefix(at.0);
        let matching_entries = entries.iter().filter_map(|(name, value)| {
            let name_suffix = name.strip_prefix(&env_prefix)?;
            Some((
                name_suffix.to_owned(),
                WithOrigin {
                    inner: Value::String(value.inner.clone()),
                    origin: value.origin.clone(),
                },
            ))
        });
        let matching_entries: Vec<_> = matching_entries.collect();

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

    /// Converts a logical prefix like `api.limits` to `api_limits_`.
    fn kv_prefix(prefix: &str) -> String {
        let mut prefix = prefix.replace('.', "_");
        if !prefix.is_empty() && !prefix.ends_with('_') {
            prefix.push('_');
        }
        prefix
    }

    /// This is necessary to prevent `deserialize_any` errors
    // TODO: log coercion errors
    fn coerce_value_type(&mut self, expecting: BasicTypes) {
        const STRUCTURED: BasicTypes = BasicTypes::ARRAY.or(BasicTypes::OBJECT);

        let Value::String(str) = &self.inner else {
            return;
        };

        // Attempt to transform the type to the expected type
        match expecting {
            // We intentionally use exact comparisons; if a type supports multiple primitive representations,
            // we do nothing.
            BasicTypes::BOOL => {
                if let Ok(bool_value) = str.parse::<bool>() {
                    self.inner = Value::Bool(bool_value);
                }
            }
            BasicTypes::INTEGER | BasicTypes::FLOAT => {
                if let Ok(number) = str.parse::<serde_json::Number>() {
                    self.inner = Value::Number(number);
                }
            }

            ty if STRUCTURED.contains(ty) => {
                let Ok(val) = serde_json::from_str::<serde_json::Value>(str) else {
                    return;
                };
                let is_value_supported = (val.is_array() && ty.contains(BasicTypes::ARRAY))
                    || (val.is_object() && ty.contains(BasicTypes::OBJECT));
                if is_value_supported {
                    let root_origin = Arc::new(ValueOrigin::Synthetic {
                        source: self.origin.clone(),
                        transform: "parsed JSON string".into(),
                    });
                    *self = Json::map_value(val, &root_origin, String::new());
                }
            }
            _ => { /* Do nothing */ }
        }
    }

    /// Deep merge stopped at params (i.e., params are always merged atomically).
    fn guided_merge(&mut self, overrides: Self, schema: &ConfigSchema, current_path: Pointer<'_>) {
        match (&mut self.inner, overrides.inner) {
            (Value::Object(this), Value::Object(other)) if !schema.contains_param(current_path) => {
                for (key, value) in other {
                    if let Some(existing_value) = this.get_mut(&key) {
                        let child_path = current_path.join(&key);
                        existing_value.guided_merge(value, schema, Pointer(&child_path));
                    } else {
                        this.insert(key, value);
                    }
                }
            }
            (this, value) => {
                *this = value;
                self.origin = overrides.origin;
            }
        }
    }
}
