//! Configuration schema.

use std::{
    any,
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
    iter,
};

use anyhow::Context;

use self::mount::{MountingPoint, MountingPoints};
use crate::{
    metadata::{BasicTypes, ConfigMetadata, ParamMetadata},
    value::Pointer,
};

mod mount;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub(crate) struct ConfigData {
    pub metadata: &'static ConfigMetadata,
    all_paths: Vec<Cow<'static, str>>,
}

impl ConfigData {
    pub(crate) fn aliases(&self) -> impl Iterator<Item = &str> + '_ {
        self.all_paths.iter().skip(1).map(Cow::as_ref)
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
    pub fn aliases(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.data.aliases()
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
    pub fn aliases(&self) -> impl Iterator<Item = &str> + '_ {
        let data = &self.schema.configs[self.prefix.as_str()].inner[&self.type_id];
        data.aliases()
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

#[derive(Debug, Clone, Default)]
struct ConfigsForPrefix {
    inner: HashMap<any::TypeId, ConfigData>,
    by_depth: BTreeSet<(usize, any::TypeId)>,
}

impl ConfigsForPrefix {
    fn by_depth(&self) -> impl Iterator<Item = &ConfigData> + '_ {
        self.by_depth.iter().map(|(_, ty)| &self.inner[ty])
    }

    fn insert(&mut self, ty: any::TypeId, depth: Option<usize>, data: ConfigData) {
        self.inner.insert(ty, data);
        if let Some(depth) = depth {
            self.by_depth.insert((depth, ty));
        }
    }

    fn extend(&mut self, other: Self) {
        self.inner.extend(other.inner);
        self.by_depth.extend(other.by_depth);
    }
}

/// Schema for configuration. Can contain multiple configs bound to different paths.
// TODO: more docs; e.g., document global aliases
#[derive(Debug, Clone, Default)]
pub struct ConfigSchema {
    // Order configs by canonical prefix for iteration etc. Also, this makes configs iterator topologically
    // sorted, and makes it easy to query prefix ranges, but these properties aren't used for now.
    configs: BTreeMap<Cow<'static, str>, ConfigsForPrefix>,
    mounting_points: MountingPoints,
}

impl ConfigSchema {
    /// Creates a schema consisting of a single configuration at the specified prefix.
    #[allow(clippy::missing_panics_doc)]
    pub fn new(metadata: &'static ConfigMetadata, prefix: &'static str) -> Self {
        let mut this = Self::default();
        this.insert(metadata, prefix)
            .expect("internal error: failed inserting first config to the schema");
        this
    }

    /// Iterates over all configs with their canonical prefixes.
    pub(crate) fn iter_ll(&self) -> impl Iterator<Item = (Pointer<'_>, &ConfigData)> + '_ {
        self.configs
            .iter()
            .flat_map(|(prefix, data)| data.inner.values().map(move |data| (Pointer(prefix), data)))
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
        self.configs.iter().flat_map(|(prefix, data)| {
            data.by_depth().map(move |data| ConfigRef {
                prefix: prefix.as_ref(),
                data,
            })
        })
    }

    /// Lists all prefixes for the specified config. This does not include aliases.
    pub fn locate(&self, metadata: &'static ConfigMetadata) -> impl Iterator<Item = &str> + '_ {
        let config_type_id = metadata.ty.id();
        self.configs.iter().filter_map(move |(prefix, data)| {
            data.inner
                .contains_key(&config_type_id)
                .then_some(prefix.as_ref())
        })
    }

    /// Gets a reference to a config by ist unique key (metadata + canonical prefix).
    pub fn get<'s>(
        &'s self,
        metadata: &'static ConfigMetadata,
        prefix: &'s str,
    ) -> Option<ConfigRef<'s>> {
        let data = self.get_ll(prefix, metadata.ty.id())?;
        Some(ConfigRef { prefix, data })
    }

    fn get_ll(&self, prefix: &str, ty: any::TypeId) -> Option<&ConfigData> {
        self.configs.get(prefix)?.inner.get(&ty)
    }

    /// Gets a reference to a config by ist unique key (metadata + canonical prefix).
    pub fn get_mut(
        &mut self,
        metadata: &'static ConfigMetadata,
        prefix: &str,
    ) -> Option<ConfigMut<'_>> {
        let ty = metadata.ty.id();
        if !self.configs.get(prefix)?.inner.contains_key(&ty) {
            return None;
        }

        Some(ConfigMut {
            schema: self,
            prefix: prefix.to_owned(),
            type_id: ty,
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
                data: &self.configs[prefix].inner[&metadata.ty.id()],
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
    pub fn single_mut(
        &mut self,
        metadata: &'static ConfigMetadata,
    ) -> anyhow::Result<ConfigMut<'_>> {
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
    pub fn insert(
        &mut self,
        metadata: &'static ConfigMetadata,
        prefix: &'static str,
    ) -> anyhow::Result<ConfigMut<'_>> {
        let mut patched = PatchedSchema::new(self);
        patched.insert_config(prefix, metadata)?;
        patched.commit();
        Ok(ConfigMut {
            schema: self,
            type_id: metadata.ty.id(),
            prefix: prefix.to_owned(),
        })
    }

    fn all_names<'a>(
        param: &'a ParamMetadata,
        config_data: &'a ConfigData,
    ) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
        let local_names = iter::once(param.name).chain(param.aliases.iter().copied());
        config_data
            .all_paths
            .iter()
            .flat_map(move |alias| local_names.clone().map(move |name| (alias.as_ref(), name)))
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
        metadata: &'static ConfigMetadata,
    ) -> anyhow::Result<()> {
        self.insert_recursively(
            prefix.into(),
            true,
            ConfigData {
                metadata,
                all_paths: vec![prefix.into()],
            },
        )
    }

    fn insert_recursively(
        &mut self,
        prefix: Cow<'static, str>,
        is_new: bool,
        data: ConfigData,
    ) -> anyhow::Result<()> {
        let depth = is_new.then_some(0_usize);
        let mut pending_configs = vec![(prefix, data, depth)];

        // Insert / update all nested configs recursively.
        while let Some((prefix, data, depth)) = pending_configs.pop() {
            // Check whether the config is already present; if so, no need to insert the config
            // or any nested configs.
            if is_new && self.base.get_ll(&prefix, data.metadata.ty.id()).is_some() {
                continue;
            }

            let child_depth = depth.map(|d| d + 1);
            let new_configs = Self::list_nested_configs(Pointer(&prefix), &data)
                .map(|(prefix, data)| (prefix.into(), data, child_depth));
            pending_configs.extend(new_configs);
            self.insert_inner(prefix, depth, data)?;
        }
        Ok(())
    }

    fn insert_alias(
        &mut self,
        prefix: String,
        config_id: any::TypeId,
        alias: Pointer<'static>,
    ) -> anyhow::Result<()> {
        let config_data = &self.base.configs[prefix.as_str()].inner[&config_id];
        if config_data.all_paths.contains(&Cow::Borrowed(alias.0)) {
            return Ok(()); // shortcut in the no-op case
        }

        let metadata = config_data.metadata;
        self.insert_recursively(
            prefix.into(),
            false,
            ConfigData {
                metadata,
                all_paths: vec![alias.0.into()],
            },
        )
    }

    fn list_nested_configs<'i>(
        prefix: Pointer<'i>,
        data: &'i ConfigData,
    ) -> impl Iterator<Item = (String, ConfigData)> + 'i {
        let all_prefixes = data.all_paths.iter().map(|alias| Pointer(alias));
        data.metadata.nested_configs.iter().map(move |nested| {
            let local_names = iter::once(nested.name).chain(nested.aliases.iter().copied());
            let all_paths = all_prefixes.clone().flat_map(|prefix| {
                local_names
                    .clone()
                    .map(move |name| prefix.join(name).into())
            });

            let config_data = ConfigData {
                metadata: nested.meta,
                all_paths: all_paths.collect(),
            };
            (prefix.join(nested.name), config_data)
        })
    }

    fn insert_inner(
        &mut self,
        prefix: Cow<'static, str>,
        depth: Option<usize>,
        mut data: ConfigData,
    ) -> anyhow::Result<()> {
        let config_name = data.metadata.ty.name_in_code();
        let config_paths = data.all_paths.iter().map(Cow::as_ref);
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
            let all_names = ConfigSchema::all_names(param, &data);

            for (name_i, (prefix, name)) in all_names.enumerate() {
                let full_name = Pointer(prefix).join(name);
                let mut was_canonical = false;
                if let Some(mount) = self.mount(&full_name) {
                    let prev_expecting = match mount {
                        MountingPoint::Param {
                            expecting,
                            is_canonical,
                        } => {
                            was_canonical = *is_canonical;
                            *expecting
                        }
                        MountingPoint::Config => {
                            anyhow::bail!(
                                "Cannot insert param `{name}` [Rust field: `{field}`] from config `{config_name}` at `{full_name}`: \
                                 config(s) are already mounted at this path",
                                name = param.name,
                                field = param.rust_field_name
                            );
                        }
                    };

                    if prev_expecting != param.expecting {
                        anyhow::bail!(
                            "Cannot insert param `{name}` [Rust field: `{field}`] from config `{config_name}` at `{full_name}`: \
                             it expects {expecting}, while the existing param(s) mounted at this path expect {prev_expecting}",
                            name = param.name,
                            field = param.rust_field_name,
                            expecting = param.expecting
                        );
                    }
                }
                let is_canonical = was_canonical || name_i == 0;
                self.patch.mounting_points.insert(
                    full_name,
                    MountingPoint::Param {
                        expecting: param.expecting,
                        is_canonical,
                    },
                );
            }
        }

        // `data` is the new data for the config, so we need to consult `base` for existing data.
        // Unlike with params, by design we never insert same config entries in the same patch,
        // so it's safe to *only* consult `base`.
        let config_id = data.metadata.ty.id();
        let prev_data = self.base.get_ll(&prefix, config_id);
        if let Some(prev_data) = prev_data {
            // Append new aliases to the end since their ordering determines alias priority
            let mut all_paths = prev_data.all_paths.clone();
            all_paths.extend_from_slice(&data.all_paths);
            data.all_paths = all_paths;
        }

        self.patch
            .configs
            .entry(prefix)
            .or_default()
            .insert(config_id, depth, data);
        Ok(())
    }

    fn commit(self) {
        for (prefix, data) in self.patch.configs {
            let prev_data = self.base.configs.entry(prefix).or_default();
            prev_data.extend(data);
        }
        self.base.mounting_points.extend(self.patch.mounting_points);
    }
}
