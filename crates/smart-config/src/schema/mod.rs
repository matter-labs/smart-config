//! Configuration schema.

use std::{any, borrow::Cow, collections::HashMap, iter};

use anyhow::Context;

use self::mount::{MountingPoint, MountingPoints};
use crate::{
    metadata::{BasicTypes, ConfigMetadata, NestedConfigMetadata, ParamMetadata},
    value::Pointer,
    DescribeConfig,
};

mod mount;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub(crate) struct ConfigData {
    pub metadata: &'static ConfigMetadata,
    pub aliases: Vec<Pointer<'static>>,
}

impl ConfigData {
    fn new(metadata: &'static ConfigMetadata) -> Self {
        Self {
            metadata,
            aliases: vec![],
        }
    }
}

/// Reference to a specific configuration inside [`ConfigSchema`].
#[derive(Debug, Clone, Copy)]
pub struct ConfigRef<'a> {
    prefix: &'a str,
    pub(crate) data: &'a ConfigData,
}

impl<'a> ConfigRef<'a> {
    /// Gets the config prefix.
    pub fn prefix(&self) -> &'a str {
        self.prefix
    }

    /// Gets the config metadata.
    pub fn metadata(&self) -> &'static ConfigMetadata {
        self.data.metadata
    }

    /// Iterates over all aliases for this config.
    pub fn aliases(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.data.aliases.iter().map(|ptr| ptr.0)
    }
}

/// Mutable reference to a specific configuration inside [`ConfigSchema`].
#[derive(Debug)]
pub struct ConfigMut<'a> {
    schema: &'a mut ConfigSchema,
    prefix: String,
    type_id: any::TypeId,
}

impl ConfigMut<'_> {
    /// Gets the config prefix.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Iterates over all aliases for this config.
    pub fn aliases(&self) -> impl Iterator<Item = &'static str> + '_ {
        let data = &self.schema.configs[&(self.type_id, self.prefix.as_str().into())];
        data.aliases.iter().map(|ptr| ptr.0)
    }

    /// Pushes an additional alias for the config.
    ///
    /// # Errors
    ///
    /// Returns an error if adding a config leads to violations of fundamental invariants
    /// (same as for [`ConfigSchema::insert()`]).
    pub fn push_alias(self, alias: &'static str) -> anyhow::Result<Self> {
        let mut patched = PatchedSchema::new(self.schema);
        patched.insert_alias(self.prefix.clone(), self.type_id, Pointer(alias))?;
        patched.commit();
        Ok(self)
    }
}

/// Schema for configuration. Can contain multiple configs bound to different paths.
#[derive(Debug, Clone, Default)]
pub struct ConfigSchema {
    configs: HashMap<(any::TypeId, Cow<'static, str>), ConfigData>,
    mounting_points: MountingPoints,
}

impl ConfigSchema {
    /// Iterates over all configs with their canonical prefixes.
    pub(crate) fn iter_ll(&self) -> impl Iterator<Item = (Pointer<'_>, &ConfigData)> + '_ {
        self.configs
            .iter()
            .map(|((_, prefix), data)| (Pointer(prefix), data))
    }

    pub(crate) fn contains_canonical_param(&self, at: Pointer<'_>) -> bool {
        self.mounting_points.get(at.0).map_or(false, |mount| {
            matches!(
                mount,
                MountingPoint::Param {
                    is_canonical: true,
                    ..
                }
            )
        })
    }

    pub(crate) fn params_with_kv_path<'s>(
        &'s self,
        kv_path: &'s str,
    ) -> impl Iterator<Item = (Pointer<'s>, BasicTypes)> + 's {
        self.mounting_points
            .by_kv_path(kv_path)
            .filter_map(|(path, mount)| {
                let expecting = match mount {
                    MountingPoint::Param { expecting, .. } => *expecting,
                    MountingPoint::Config => return None,
                };
                Some((path, expecting))
            })
    }

    /// Iterates over all configs contained in this schema. A unique key for a config is its type + location;
    /// i.e., multiple returned refs may have the same config type xor same location (never both).
    pub fn iter(&self) -> impl Iterator<Item = ConfigRef<'_>> + '_ {
        self.configs
            .iter()
            .map(|((_, prefix), data)| ConfigRef { prefix, data })
    }

    /// Lists all prefixes for the specified config. This does not include aliases.
    pub fn locate(&self, metadata: &'static ConfigMetadata) -> impl Iterator<Item = &str> + '_ {
        let config_type_id = metadata.ty.id();
        self.configs.keys().filter_map(move |(type_id, prefix)| {
            (*type_id == config_type_id).then_some(prefix.as_ref())
        })
    }

    /// Returns a single reference to the specified config.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is not registered or has more than one mount point.
    #[allow(clippy::missing_panics_doc)] // false positive
    pub fn single(&self, metadata: &'static ConfigMetadata) -> anyhow::Result<ConfigRef<'_>> {
        let prefixes: Vec<_> = self.locate(metadata).take(2).collect();
        match prefixes.as_slice() {
            [] => anyhow::bail!(
                "configuration `{}` is not registered in schema",
                metadata.ty.name_in_code()
            ),
            &[prefix] => Ok(ConfigRef {
                prefix,
                data: self
                    .configs
                    .get(&(metadata.ty.id(), prefix.into()))
                    .unwrap(),
            }),
            [first, second] => anyhow::bail!(
                "configuration `{}` is registered in at least 2 locations: {first:?}, {second:?}",
                metadata.ty.name_in_code()
            ),
            _ => unreachable!(),
        }
    }

    /// Returns a single mutable reference to the specified config.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is not registered or has more than one mount point.
    #[allow(clippy::missing_panics_doc)] // false positive
    pub fn single_mut<C: DescribeConfig>(&mut self) -> anyhow::Result<ConfigMut<'_>> {
        let metadata = &C::DESCRIPTION;
        let mut it = self.locate(metadata);
        let first_prefix = it.next().with_context(|| {
            format!(
                "configuration `{}` is not registered in schema",
                metadata.ty.name_in_code()
            )
        })?;
        if let Some(second_prefix) = it.next() {
            anyhow::bail!(
                "configuration `{}` is registered in at least 2 locations: {first_prefix:?}, {second_prefix:?}",
                metadata.ty.name_in_code()
            );
        }

        drop(it);
        let prefix = first_prefix.to_owned();
        Ok(ConfigMut {
            schema: self,
            type_id: metadata.ty.id(),
            prefix,
        })
    }

    /// Inserts a new configuration type at the specified place.
    ///
    /// # Errors
    ///
    /// Returns an error if adding a config leads to violations of fundamental invariants:
    ///
    /// - If a parameter in the new config (taking aliases into account, and params in nested / flattened configs)
    ///   is mounted at the location of an existing config.
    /// - Vice versa, if a config or nested config is mounted at the location of an existing param.
    /// - If a parameter is mounted at the location of a parameter with disjoint [expected types](ParamMetadata.expecting).
    pub fn insert<C>(&mut self, prefix: &'static str) -> anyhow::Result<ConfigMut<'_>>
    where
        C: DescribeConfig,
    {
        let metadata = &C::DESCRIPTION;
        let config_id = any::TypeId::of::<C>();

        let mut patched = PatchedSchema::new(self);
        patched.insert_config(prefix, config_id, metadata)?;
        patched.commit();
        Ok(ConfigMut {
            schema: self,
            type_id: metadata.ty.id(),
            prefix: prefix.to_owned(),
        })
    }

    fn all_names<'a>(
        canonical_prefix: &'a str,
        param: &'a ParamMetadata,
        config_data: &'a ConfigData,
    ) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
        let local_names = iter::once(param.name).chain(param.aliases.iter().copied());
        let local_names_ = local_names.clone();
        let global_aliases = config_data
            .aliases
            .iter()
            .flat_map(move |alias| local_names_.clone().map(move |name| (alias.0, name)));
        let local_aliases = local_names
            .clone()
            .map(move |name| (canonical_prefix, name));
        local_aliases.chain(global_aliases)
    }
}

/// [`ConfigSchema`] together with a patch that can be atomically committed.
#[derive(Debug)]
#[must_use = "Should be `commit()`ted"]
struct PatchedSchema<'a> {
    base: &'a mut ConfigSchema,
    patch: ConfigSchema,
}

impl<'a> PatchedSchema<'a> {
    fn new(base: &'a mut ConfigSchema) -> Self {
        Self {
            base,
            patch: ConfigSchema::default(),
        }
    }

    fn mount(&self, path: &str) -> Option<&MountingPoint> {
        self.patch
            .mounting_points
            .get(path)
            .or_else(|| self.base.mounting_points.get(path))
    }

    fn insert_config(
        &mut self,
        prefix: &'static str,
        config_id: any::TypeId,
        metadata: &'static ConfigMetadata,
    ) -> anyhow::Result<()> {
        self.insert_inner(prefix.into(), config_id, ConfigData::new(metadata))?;

        // Insert all nested configs recursively.
        let mut pending_configs: Vec<_> =
            Self::list_nested_configs(prefix, metadata.nested_configs).collect();
        while let Some((prefix, metadata)) = pending_configs.pop() {
            let new_configs = Self::list_nested_configs(&prefix, metadata.nested_configs);
            pending_configs.extend(new_configs);

            self.insert_inner(prefix.into(), metadata.ty.id(), ConfigData::new(metadata))?;
        }
        Ok(())
    }

    fn insert_alias(
        &mut self,
        prefix: String,
        config_id: any::TypeId,
        alias: Pointer<'static>,
    ) -> anyhow::Result<()> {
        let config_data = &self.base.configs[&(config_id, prefix.as_str().into())];
        if config_data.aliases.contains(&alias) {
            return Ok(()); // shortcut in the no-op case
        }

        let new_data = ConfigData {
            metadata: config_data.metadata,
            aliases: vec![alias],
        };
        self.insert_inner(prefix.into(), config_id, new_data)
    }

    fn list_nested_configs<'i>(
        prefix: &'i str,
        nested: &'i [NestedConfigMetadata],
    ) -> impl Iterator<Item = (String, &'static ConfigMetadata)> + 'i {
        nested
            .iter()
            .map(|nested| (Pointer(prefix).join(nested.name), nested.meta))
    }

    fn insert_inner(
        &mut self,
        prefix: Cow<'static, str>,
        config_id: any::TypeId,
        mut data: ConfigData,
    ) -> anyhow::Result<()> {
        let config_name = data.metadata.ty.name_in_code();
        let config_paths = data.aliases.iter().map(|ptr| ptr.0);
        let config_paths = iter::once(prefix.as_ref()).chain(config_paths);

        for path in config_paths {
            if let Some(mount) = self.mount(path) {
                match mount {
                    MountingPoint::Config => { /* OK */ }
                    MountingPoint::Param { .. } => {
                        anyhow::bail!(
                            "Cannot mount config `{}` at `{path}` because parameter(s) are already mounted at this path",
                            data.metadata.ty.name_in_code()
                        );
                    }
                }
            }
            self.patch
                .mounting_points
                .insert(path.to_owned(), MountingPoint::Config);
        }

        for param in data.metadata.params {
            let all_names = ConfigSchema::all_names(&prefix, param, &data);

            for (name_i, (prefix, name)) in all_names.enumerate() {
                let full_name = Pointer(prefix).join(name);
                let (prev_expecting, was_canonical) = if let Some(mount) = self.mount(&full_name) {
                    match mount {
                        &MountingPoint::Param {
                            expecting,
                            is_canonical,
                        } => (expecting, is_canonical),

                        MountingPoint::Config => {
                            anyhow::bail!(
                                "Cannot insert param `{name}` [Rust field: `{field}`] from config `{config_name}` at `{full_name}`: \
                                 config(s) are already mounted at this path",
                                name = param.name,
                                field = param.rust_field_name
                            );
                        }
                    }
                } else {
                    (BasicTypes::ANY, false)
                };

                let Some(expecting) = prev_expecting.and(param.expecting) else {
                    anyhow::bail!(
                        "Cannot insert param `{name}` [Rust field: `{field}`] from config `{config_name}` at `{full_name}`: \
                         it expects {expecting}, while the existing param(s) mounted at this path expect {prev_expecting}",
                        name = param.name,
                        field = param.rust_field_name,
                        expecting = param.expecting
                    );
                };
                let is_canonical = was_canonical || name_i == 0;

                self.patch.mounting_points.insert(
                    full_name,
                    MountingPoint::Param {
                        expecting,
                        is_canonical,
                    },
                );
            }
        }

        // `data` is the new data for the config, so we need to consult `base` for existing data.
        // Unlike with params, by design we never insert same config entries in the same patch,
        // so it's safe to *only* consult `base`.
        if let Some(prev_data) = self.base.configs.get(&(config_id, prefix.as_ref().into())) {
            // Append new aliases to the end since their ordering determines alias priority
            let mut all_aliases = prev_data.aliases.clone();
            all_aliases.extend_from_slice(&data.aliases);
            data.aliases = all_aliases;
        }
        self.patch.configs.insert((config_id, prefix), data);
        Ok(())
    }

    fn commit(self) {
        self.base.configs.extend(self.patch.configs);
        self.base.mounting_points.extend(self.patch.mounting_points);
    }
}
