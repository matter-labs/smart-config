use std::{iter, marker::PhantomData, sync::Arc};

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
    merged: WithOrigin,
}

impl<'a> ConfigRepository<'a> {
    /// Creates an empty config repo based on the provided schema.
    pub fn new(schema: &'a ConfigSchema) -> Self {
        Self {
            schema,
            de_options: DeserializerOptions::default(),
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
            ConfigContents::KeyValue(kv) => WithOrigin::nest_kvs(kv, self.schema, &source_origin),
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

        context.preprocess_and_deserialize::<C>().map_err(|_| {
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
        self.nest_object_params(schema);
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

    /// Nests values inside matching object params.
    ///
    /// For example, we have an object param at `test.param` and a source with a value at `test.param_ms`.
    /// This transform will copy this value to `test.param.ms` (i.e., inside the param object), provided that
    /// the source doesn't contain `test.param` or contains an object at this path.
    fn nest_object_params(&mut self, schema: &ConfigSchema) {
        for (prefix, config_data) in schema.iter() {
            let Some(config_object) = self.get_mut(prefix) else {
                continue;
            };
            let config_origin = &config_object.origin;
            let Value::Object(config_object) = &mut config_object.inner else {
                continue;
            };

            for param in config_data.metadata.params {
                if !param.expecting.contains(BasicTypes::OBJECT) {
                    continue;
                }

                let param_object = match config_object.get(param.name) {
                    None => None,
                    Some(WithOrigin {
                        inner: Value::Object(obj),
                        ..
                    }) => Some(obj),
                    // Never overwrite non-objects with an object value.
                    Some(_) => continue,
                };

                let matching_fields: Vec<_> = config_object
                    .iter()
                    .filter_map(|(name, field)| {
                        let stripped_name = name.strip_prefix(param.name)?.strip_prefix('_')?;
                        if let Some(param_object) = param_object {
                            if param_object.contains_key(stripped_name) {
                                return None; // Never overwrite existing fields
                            }
                        }
                        Some((stripped_name.to_owned(), field.clone()))
                    })
                    .collect();
                if matching_fields.is_empty() {
                    continue;
                }

                if !config_object.contains_key(param.name) {
                    let origin = Arc::new(ValueOrigin::Synthetic {
                        source: config_origin.clone(),
                        transform: format!("nesting for object param '{}'", param.name),
                    });
                    let val = Self::new(Value::Object(Map::new()), origin);
                    config_object.insert(param.name.to_owned(), val);
                }

                let Value::Object(param_object) =
                    &mut config_object.get_mut(param.name).unwrap().inner
                else {
                    unreachable!(); // Due to the checks above
                };
                param_object.extend(matching_fields);
            }
        }
    }

    /// Nests a flat key–value map into a structured object using the provided `schema`.
    ///
    /// Has complexity `O(kvs.len() * log(n_params))`, which seems about the best possible option if `kvs` is not presorted.
    fn nest_kvs(kvs: Map<String>, schema: &ConfigSchema, source_origin: &Arc<ValueOrigin>) -> Self {
        let mut dest = Self {
            inner: Value::Object(Map::new()),
            origin: source_origin.clone(),
        };

        for (key, value) in kvs {
            let value = Self::new(Value::String(value.inner.clone()), value.origin);

            // Get all params with full paths matching a prefix of `key` split on one of `_`s. E.g.,
            // for `key = "very_long_prefix_value"`, we'll try "very_long_prefix_value", "very_long_prefix", ..., "very".
            // If any of these prefixes corresponds to a param, we'll nest the value to align with the param.
            // For example, if `very.long_prefix.value` is a param, we'll nest the value to `very.long_prefix.value`,
            // and if `very_long.prefix.value` is a param as well, we'll copy the value to both places.
            //
            // For prefixes, we only copy the value if the param supports objects; e.g. if `very_long.prefix` is a param,
            // then we'll copy the value to `very_long.prefix_value`.
            let mut key_prefix = key.as_str();
            while !key_prefix.is_empty() {
                for (param_path, expecting) in schema.params_with_kv_path(key_prefix) {
                    let should_copy = key_prefix == key || expecting.contains(BasicTypes::OBJECT);
                    if should_copy {
                        // `unwrap()` is safe: params have non-empty paths
                        let (parent, _) = param_path.split_last().unwrap();
                        let field_name_start = if parent.0.is_empty() {
                            parent.0.len()
                        } else {
                            parent.0.len() + 1 // skip `_` after the parent
                        };
                        let field_name = key[field_name_start..].to_owned();

                        let origin = Arc::new(ValueOrigin::Synthetic {
                            source: source_origin.clone(),
                            transform: format!("nesting kv entries for '{param_path}'"),
                        });
                        dest.ensure_object(parent, |_| origin.clone())
                            .insert(field_name, value.clone());
                    }
                }

                key_prefix = match key_prefix.rsplit_once('_') {
                    Some((prefix, _)) => prefix,
                    None => break,
                };
            }
        }
        dest
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
