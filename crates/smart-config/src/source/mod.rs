use std::{collections::BTreeSet, sync::Arc};

use anyhow::Context as _;
use serde::de::DeserializeOwned;

pub use self::env::Environment;
use crate::{
    metadata::{ConfigMetadata, DescribeConfig, TypeKind},
    parsing::ValueDeserializer,
    schema::{Alias, ConfigSchema},
    value::{Map, Pointer, Value, ValueOrigin, ValueWithOrigin},
};

mod env;
#[cfg(test)]
mod tests;

/// Configuration repository containing zero or more configuration sources.
#[derive(Debug, Default)]
pub struct ConfigRepository {
    env: Environment,
}

impl From<Environment> for ConfigRepository {
    fn from(env: Environment) -> Self {
        Self { env }
    }
}

impl ConfigRepository {
    /// Extends this environment with environment variables / key-value map.
    pub fn with_env(mut self, env: Environment) -> Self {
        self.env.extend(env);
        self
    }

    pub fn parser(mut self, schema: &ConfigSchema) -> anyhow::Result<ConfigParser<'_>> {
        let synthetic_origin = Arc::new(ValueOrigin::SyntheticObject);
        let mut map = ValueWithOrigin {
            inner: Value::Object(Map::new()),
            origin: synthetic_origin.clone(),
        };

        let all_objects: BTreeSet<_> = schema
            .prefixes_with_aliases()
            .flat_map(Pointer::with_ancestors)
            .chain([Pointer("")])
            .collect();
        for &object_ptr in &all_objects {
            map.ensure_object(object_ptr, &synthetic_origin)?;
        }
        for &object_ptr in all_objects.iter().rev() {
            map.copy_key_value_entries(object_ptr, &mut self.env);
        }

        for (prefix, config_data) in schema.iter() {
            for alias in &config_data.aliases {
                map.merge_alias(prefix, alias);
            }

            if let Some(config_map) = map.get_mut(prefix) {
                config_map.normalize_value_types(config_data.metadata);
            };
        }

        Ok(ConfigParser { map, schema })
    }
}

/// Output of parsing configurations using [`ConfigSchema::parser()`].
#[derive(Debug)]
pub struct ConfigParser<'a> {
    schema: &'a ConfigSchema,
    map: ValueWithOrigin,
}

impl ConfigParser<'_> {
    #[cfg(test)]
    pub(crate) fn map(&self) -> &ValueWithOrigin {
        &self.map
    }

    /// Parses a configuration.
    pub fn parse<C>(&self) -> anyhow::Result<C>
    where
        C: DescribeConfig + DeserializeOwned,
    {
        let metadata = C::describe_config();
        let config_ref = self.schema.single(metadata)?;
        let prefix = config_ref.prefix();

        // `unwrap()` is safe due to preparations when constructing the `Parser`; all config prefixes have objects
        let config_map = self.map.get(Pointer(prefix)).unwrap();
        debug_assert!(
            matches!(&config_map.inner, Value::Object(_)),
            "Unexpected value at {prefix:?}: {config_map:?}"
        );

        let deserializer = ValueDeserializer::new(config_map);
        C::deserialize(deserializer)
            .with_context(|| {
                let summary = if let Some(header) = metadata.help_header() {
                    format!(" ({})", header.trim().to_lowercase())
                } else {
                    String::new()
                };
                format!(
                    "error parsing configuration `{name}`{summary} at `{prefix}` (aliases: {aliases:?})",
                    name = metadata.ty.name_in_code(),
                    aliases = config_ref.data.aliases
                )
            })
    }
}

impl ValueWithOrigin {
    /// Ensures that there is an object (possibly empty) at the specified location. Returns an error
    /// if the locations contains anything other than an object.
    fn ensure_object(
        &mut self,
        at: Pointer<'_>,
        synthetic_origin: &Arc<ValueOrigin>,
    ) -> anyhow::Result<()> {
        let Some((parent, last_segment)) = at.split_last() else {
            // Nothing to do.
            return Ok(());
        };

        // `unwrap()` is safe since `ensure_object()` is always called for the parent
        let Value::Object(map) = &mut self.get_mut(parent).unwrap().inner else {
            anyhow::bail!("expected object at {parent:?}");
        };
        if !map.contains_key(last_segment) {
            map.insert(
                last_segment.to_owned(),
                ValueWithOrigin {
                    inner: Value::Object(Map::new()),
                    origin: synthetic_origin.clone(),
                },
            );
        }
        Ok(())
    }

    fn copy_key_value_entries(&mut self, at: Pointer<'_>, entries: &mut Environment) {
        let Value::Object(map) = &mut self.get_mut(at).unwrap().inner else {
            unreachable!("expected object at {at:?}"); // Should be ensured by calling `ensure_object()`
        };
        let matching_entries = entries.take_matching_entries(at);
        map.extend(matching_entries);
    }

    fn merge_alias(&mut self, target_prefix: Pointer<'_>, alias: &Alias<()>) {
        let Value::Object(map) = &self.get(target_prefix).unwrap().inner else {
            unreachable!("expected object at {target_prefix:?}"); // Should be ensured by calling `ensure_object()`
        };
        let new_entries = alias.param_names.iter().filter_map(|&param_name| {
            if map.contains_key(param_name) {
                None // Variable is already set
            } else {
                let value = self.get(Pointer(&alias.prefix.join(param_name))).cloned()?;
                Some((param_name.to_owned(), value))
            }
        });
        let new_entries: Vec<_> = new_entries.collect();

        let Value::Object(map) = &mut self.get_mut(target_prefix).unwrap().inner else {
            unreachable!("expected object at {target_prefix:?}"); // Should be ensured by calling `ensure_object()`
        };
        map.extend(new_entries);
    }

    /// This is necessary to prevent `deserialize_any` errors
    fn normalize_value_types(&mut self, metadata: &ConfigMetadata) {
        let Value::Object(map) = &mut self.inner else {
            unreachable!("expected an object due to previous preprocessing steps");
        };

        for param in &metadata.params {
            if let Some(value) = map.get_mut(param.name) {
                if !matches!(value.origin.as_ref(), ValueOrigin::EnvVar(_)) {
                    continue;
                }
                let Value::String(str) = &value.inner else {
                    continue;
                };

                // Attempt to transform the type to the expected type
                match param.base_type.kind() {
                    Some(TypeKind::Bool) => {
                        if let Ok(bool_value) = str.parse::<bool>() {
                            value.inner = Value::Bool(bool_value);
                        }
                    }
                    Some(TypeKind::Integer) => {
                        if let Ok(number) = str.parse::<serde_json::Number>() {
                            value.inner = Value::Number(number);
                        }
                    }
                    _ => { /* Do nothing */ }
                }
            }
        }
    }
}
