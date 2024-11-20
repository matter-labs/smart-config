use std::{collections::BTreeSet, mem, sync::Arc};

use anyhow::Context as _;

pub use self::{
    env::{Environment, KeyValueMap},
    json::Json,
    yaml::Yaml,
};
use crate::{
    de::{DeserializeConfig, ValueDeserializer},
    metadata::{ConfigMetadata, PrimitiveType, SchemaType},
    schema::{Alias, ConfigSchema},
    value::{Map, Pointer, Value, ValueOrigin, WithOrigin},
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
    /// Converts this source into config contents.
    fn into_contents(self) -> ConfigContents;
}

/// Configuration repository containing zero or more configuration sources.
#[derive(Debug, Clone)]
pub struct ConfigRepository<'a> {
    schema: &'a ConfigSchema,
    all_prefixes_with_aliases: BTreeSet<Pointer<'a>>,
    merged: WithOrigin,
}

impl<'a> ConfigRepository<'a> {
    pub fn new(schema: &'a ConfigSchema) -> Self {
        let (all_prefixes_with_aliases, merged) = Self::initialize_object(schema);
        Self {
            schema,
            all_prefixes_with_aliases,
            merged,
        }
    }

    fn initialize_object(schema: &'a ConfigSchema) -> (BTreeSet<Pointer<'a>>, WithOrigin) {
        let synthetic_origin = Arc::<ValueOrigin>::default();
        let mut map = WithOrigin {
            inner: Value::Object(Map::default()),
            origin: synthetic_origin.clone(),
        };

        let all_prefixes_with_aliases: BTreeSet<_> = schema
            .prefixes_with_aliases()
            .flat_map(Pointer::with_ancestors)
            .chain([Pointer("")])
            .collect();
        for &object_ptr in &all_prefixes_with_aliases {
            map.ensure_object(object_ptr, || synthetic_origin.clone());
        }
        (all_prefixes_with_aliases, map)
    }

    /// Extends this environment with a new configuration source.
    // FIXME: is it possible to break invariants (e.g., overwrite objects at mounting points)?
    pub fn with<S: ConfigSource>(mut self, source: S) -> Self {
        match source.into_contents() {
            ConfigContents::KeyValue(mut kv) => {
                for &object_ptr in self.all_prefixes_with_aliases.iter().rev() {
                    self.merged.copy_key_value_entries(object_ptr, &mut kv);
                }
            }
            ConfigContents::Hierarchical(map) => {
                self.merged.merge(WithOrigin {
                    inner: Value::Object(map),
                    origin: Arc::default(),
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
            let config_object = self.merged.get_mut(prefix).unwrap();
            let Value::Object(config_object) = &mut config_object.inner else {
                unreachable!();
            };

            for param in &config_data.metadata.params {
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
                config_map.normalize_value_types(config_data.metadata);
            }
        }

        self
    }

    // TODO: probably makes sense to make public
    #[cfg(test)]
    pub(crate) fn merged(&self) -> &WithOrigin {
        &self.merged
    }

    /// Parses a configuration.
    pub fn parse<C>(&self) -> anyhow::Result<C>
    where
        C: DeserializeConfig,
    {
        let metadata = C::describe_config();
        let config_ref = self.schema.single(metadata)?;
        let prefix = config_ref.prefix();

        // `unwrap()` is safe due to preparations when constructing the repo; all config prefixes have objects
        let config_map = self.merged.get(Pointer(prefix)).unwrap();
        debug_assert!(
            matches!(&config_map.inner, Value::Object(_)),
            "Unexpected value at {prefix:?}: {config_map:?}"
        );

        let deserializer = ValueDeserializer::new(config_map, prefix.to_owned());
        C::deserialize_config(deserializer)
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
    /// Ensures that there is an object (possibly empty) at the specified location.
    fn ensure_object(&mut self, at: Pointer<'_>, create_origin: impl FnOnce() -> Arc<ValueOrigin>) {
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
                    origin: create_origin(),
                },
            );
        }
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
                match param.type_kind {
                    SchemaType::Primitive(PrimitiveType::Bool) => {
                        if let Ok(bool_value) = str.parse::<bool>() {
                            value.inner = Value::Bool(bool_value);
                        }
                    }
                    SchemaType::Primitive(PrimitiveType::Integer | PrimitiveType::Float) => {
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
