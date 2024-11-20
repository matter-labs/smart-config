use std::{collections::BTreeSet, mem, sync::Arc};

use anyhow::Context as _;
use serde::de::DeserializeOwned;

pub use self::{
    env::{Environment, KeyValueMap},
    json::Json,
    yaml::Yaml,
};
use crate::{
    metadata::{ConfigMetadata, DescribeConfig, TypeKind},
    parsing::ValueDeserializer,
    schema::{Alias, ConfigSchema},
    value::{Map, Pointer, Value, ValueOrigin, WithOrigin},
};

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
    /// Converts this source into config contents.
    fn into_contents(self) -> ConfigContents;
}

/// Configuration repository containing zero or more configuration sources.
#[derive(Debug, Clone, Default)]
pub struct ConfigRepository {
    /// Hierarchical part of the configuration.
    object: Map,
    /// Key-value part of the config that requires pre-processing.
    // TODO: rn, it always has higher priority than `object`; document this or change
    key_value_map: Map<String>,
}

impl<S: ConfigSource> From<S> for ConfigRepository {
    fn from(source: S) -> Self {
        let (object, key_value_map) = match source.into_contents() {
            ConfigContents::Hierarchical(object) => (object, Map::default()),
            ConfigContents::KeyValue(kv) => (Map::default(), kv),
        };

        Self {
            object,
            key_value_map,
        }
    }
}

impl ConfigRepository {
    /// Extends this environment with environment variables / key-value map.
    pub fn with<S: ConfigSource>(mut self, source: S) -> Self {
        match source.into_contents() {
            ConfigContents::KeyValue(kv) => {
                self.key_value_map.extend(kv);
            }
            ConfigContents::Hierarchical(map) => {
                WithOrigin::merge_into_map(&mut self.object, map);
            }
        }
        self
    }

    pub fn parser(mut self, schema: &ConfigSchema) -> anyhow::Result<ConfigParser<'_>> {
        let synthetic_origin = Arc::<ValueOrigin>::default();
        let mut map = WithOrigin {
            inner: Value::Object(self.object),
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
            map.copy_key_value_entries(object_ptr, &mut self.key_value_map);
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
    map: WithOrigin,
}

impl ConfigParser<'_> {
    #[cfg(test)]
    pub(crate) fn map(&self) -> &WithOrigin {
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

impl WithOrigin {
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
                WithOrigin {
                    inner: Value::Object(Map::new()),
                    origin: synthetic_origin.clone(),
                },
            );
        }
        Ok(())
    }

    fn copy_key_value_entries(&mut self, at: Pointer<'_>, entries: &mut Map<String>) {
        let Value::Object(map) = &mut self.get_mut(at).unwrap().inner else {
            unreachable!("expected object at {at:?}"); // Should be ensured by calling `ensure_object()`
        };
        let matching_entries =
            Self::take_matching_kv_entries(entries, at)
                .into_iter()
                .map(|(key, value)| {
                    (
                        key,
                        WithOrigin {
                            inner: Value::String(value.inner),
                            origin: value.origin,
                        },
                    )
                });
        map.extend(matching_entries);
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
                let Value::String(str) = &value.inner else {
                    continue;
                };

                // Attempt to transform the type to the expected type
                match param.base_type_kind {
                    TypeKind::Bool => {
                        if let Ok(bool_value) = str.parse::<bool>() {
                            value.inner = Value::Bool(bool_value);
                        }
                    }
                    TypeKind::Integer | TypeKind::Float => {
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
