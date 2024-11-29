//! Configuration schema.

use std::{
    any,
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
    io, iter,
    marker::PhantomData,
};

use anyhow::Context;

use crate::{
    metadata::{BasicTypes, ConfigMetadata, NestedConfigMetadata, ParamMetadata},
    value::Pointer,
    DescribeConfig,
};

const INDENT: &str = "    ";

/// Alias specification for a config.
// FIXME: simplify aliases by removing param names?
#[derive(Debug, Clone)]
pub struct Alias<C> {
    pub(crate) prefix: Pointer<'static>,
    pub(crate) param_names: HashSet<&'static str>,
    _config: PhantomData<C>,
}

impl<C: DescribeConfig> Alias<C> {
    /// Specifies that aliased parameters (which is all config params) start at the specified `prefix`.
    pub fn prefix(prefix: &'static str) -> Self {
        Self {
            prefix: Pointer(prefix),
            param_names: C::DESCRIPTION
                .params
                .iter()
                .flat_map(|param| param.aliases.iter().copied().chain([param.name]))
                .collect(),
            _config: PhantomData,
        }
    }

    /// Excludes parameters from this alias rule according to the provided predicate.
    #[must_use]
    pub fn exclude(mut self, mut predicate: impl FnMut(&str) -> bool) -> Self {
        self.param_names.retain(|name| !predicate(name));
        self
    }

    fn drop_type_param(self) -> Alias<()> {
        Alias {
            prefix: self.prefix,
            param_names: self.param_names,
            _config: PhantomData,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ConfigData {
    pub metadata: &'static ConfigMetadata,
    pub aliases: Vec<Alias<()>>,
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

    /// Iterates over all aliases for this config.
    pub fn aliases(&self) -> impl Iterator<Item = &'a Alias<()>> + '_ {
        self.data.aliases.iter()
    }
}

/// Mutable reference to a specific configuration inside [`ConfigSchema`].
#[derive(Debug)]
pub struct ConfigMut<'a, C> {
    prefix: String,
    pub(crate) data: &'a mut ConfigData,
    _config: PhantomData<C>,
}

impl<C: DescribeConfig> ConfigMut<'_, C> {
    /// Gets the config prefix.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Iterates over all aliases for this config.
    pub fn aliases(&self) -> impl Iterator<Item = &Alias<()>> + '_ {
        self.data.aliases.iter()
    }

    /// Pushes an additional alias for the config.
    // FIXME: update mounting points; maybe leave as the only way to add aliases
    pub fn push_alias(&mut self, alias: Alias<C>) {
        self.data.aliases.push(alias.drop_type_param());
    }
}

/// Mounting point info sufficient to resolve the mounted config / param.
// TODO: add refs
#[derive(Debug, Clone)]
enum MountingPoint {
    /// Contains type IDs of mounted config(s).
    Config,
    Param {
        expecting: BasicTypes,
    },
}

/// Schema for configuration. Can contain multiple configs bound to different paths.
#[derive(Debug, Clone, Default)]
pub struct ConfigSchema {
    configs: HashMap<(any::TypeId, Cow<'static, str>), ConfigData>,
    mounting_points: BTreeMap<String, MountingPoint>,
}

impl ConfigSchema {
    /// Lists prefixes and aliases for all configs. There may be duplicates!
    pub(crate) fn prefixes_with_aliases(&self) -> impl Iterator<Item = Pointer<'_>> + '_ {
        self.configs.iter().flat_map(|((_, prefix), data)| {
            iter::once(Pointer(prefix)).chain(data.aliases.iter().map(|alias| alias.prefix))
        })
    }

    /// Iterates over all configs with their canonical prefixes.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (Pointer<'_>, &ConfigData)> + '_ {
        self.configs
            .iter()
            .map(|((_, prefix), data)| (Pointer(prefix), data))
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
    pub fn single_mut<C: DescribeConfig>(&mut self) -> anyhow::Result<ConfigMut<'_, C>> {
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
            data: self
                .configs
                .get_mut(&(metadata.ty.id(), prefix.clone().into()))
                .unwrap(),
            prefix,
            _config: PhantomData,
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
    pub fn insert<C>(self, prefix: &'static str) -> anyhow::Result<Self>
    where
        C: DescribeConfig,
    {
        self.insert_aliased::<C>(prefix, [])
    }

    /// Inserts a new configuration type at the specified place with potential aliases.
    ///
    /// # Errors
    ///
    /// Returns errors in the same situations as [`Self::insert()`], with aliases for configs / params
    /// extended to both local aliases and aliases passed as the second arg.
    pub fn insert_aliased<C>(
        mut self,
        prefix: &'static str,
        aliases: impl IntoIterator<Item = Alias<C>>,
    ) -> anyhow::Result<Self>
    where
        C: DescribeConfig,
    {
        let metadata = &C::DESCRIPTION;
        let config_id = any::TypeId::of::<C>();
        let aliases = aliases.into_iter().map(Alias::drop_type_param).collect();

        let mut patched = PatchedSchema::new(&mut self);
        patched.insert_new_config(prefix, aliases, config_id, metadata)?;
        patched.commit();
        Ok(self)
    }

    /// Writes help about this schema to the provided writer. `param_filter` can be used to filter displayed parameters.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors should they occur when writing to the writer.
    pub fn write_help(
        &self,
        writer: &mut impl io::Write,
        mut param_filter: impl FnMut(&ParamMetadata) -> bool,
    ) -> io::Result<()> {
        for ((_, prefix), config_data) in &self.configs {
            let filtered_params: Vec<_> = config_data
                .metadata
                .params
                .iter()
                .filter(|&param| param_filter(param))
                .collect();
            if filtered_params.is_empty() {
                continue;
            }

            writeln!(writer, "{}\n", config_data.metadata.help)?;
            for param in filtered_params {
                Self::write_parameter(writer, prefix, param, config_data)?;
                writeln!(writer)?;
            }
        }
        Ok(())
    }

    fn all_names<'a>(
        canonical_prefix: &'a str,
        param: &'a ParamMetadata,
        config_data: &'a ConfigData,
    ) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
        let local_aliases = iter::once(param.name).chain(param.aliases.iter().copied());
        let local_aliases_ = local_aliases.clone();
        let global_aliases = config_data.aliases.iter().flat_map(move |alias| {
            local_aliases_.clone().filter_map(|name| {
                alias
                    .param_names
                    .contains(name)
                    .then_some((alias.prefix.0, name))
            })
        });
        let local_aliases = local_aliases
            .clone()
            .map(move |name| (canonical_prefix, name));
        local_aliases.chain(global_aliases)
    }

    fn write_parameter(
        writer: &mut impl io::Write,
        prefix: &str,
        param: &ParamMetadata,
        config_data: &ConfigData,
    ) -> io::Result<()> {
        let all_names = Self::all_names(prefix, param, config_data);
        for (prefix, name) in all_names {
            let prefix_sep = if prefix.is_empty() || prefix.ends_with('.') {
                ""
            } else {
                "."
            };
            writeln!(writer, "{prefix}{prefix_sep}{name}")?;
        }

        let kind = param.expecting;
        let ty = format!("{kind} [Rust: {}]", param.rust_type.name_in_code());
        let default = if let Some(default) = param.default_value() {
            format!(", default: {default:?}")
        } else {
            String::new()
        };
        writeln!(writer, "{INDENT}Type: {ty}{default}")?;

        if !param.help.is_empty() {
            for line in param.help.lines() {
                writeln!(writer, "{INDENT}{line}")?;
            }
        }
        Ok(())
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

    fn insert_new_config(
        &mut self,
        prefix: &'static str,
        aliases: Vec<Alias<()>>,
        config_id: any::TypeId,
        metadata: &'static ConfigMetadata,
    ) -> anyhow::Result<()> {
        self.insert_inner(prefix.into(), config_id, ConfigData { metadata, aliases })?;

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
        type_id: any::TypeId,
        data: ConfigData,
    ) -> anyhow::Result<()> {
        let config_name = data.metadata.ty.name_in_code();
        let config_paths = data.aliases.iter().map(|alias| alias.prefix.0);
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
            for (prefix, name) in all_names {
                let full_name = Pointer(prefix).join(name);
                let prev_expecting = if let Some(mount) = self.mount(&full_name) {
                    match mount {
                        MountingPoint::Param { expecting } => *expecting,
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
                    BasicTypes::ANY
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
                self.patch
                    .mounting_points
                    .insert(full_name, MountingPoint::Param { expecting });
            }
        }

        self.patch.configs.insert((type_id, prefix), data);
        Ok(())
    }

    fn commit(self) {
        self.base.configs.extend(self.patch.configs);
        self.base.mounting_points.extend(self.patch.mounting_points);
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;
    use crate::{
        metadata::BasicTypes, value::Value, ConfigRepository, DescribeConfig, DeserializeConfig,
        Environment,
    };

    /// # Test configuration
    ///
    /// Extended description.
    #[derive(Debug, Default, PartialEq, DescribeConfig, DeserializeConfig)]
    #[config(crate = crate)]
    struct TestConfig {
        /// String value.
        #[config(alias = "string", default = TestConfig::default_str)]
        str: String,
        /// Optional value.
        #[config(rename = "optional")]
        optional_int: Option<u32>,
    }

    impl TestConfig {
        fn default_str() -> String {
            "default".to_owned()
        }
    }

    #[derive(Debug, Default, PartialEq, DescribeConfig, DeserializeConfig)]
    #[config(crate = crate)]
    struct NestingConfig {
        #[config(default)]
        bool_value: bool,
        /// Hierarchical nested config.
        #[config(default, nest)]
        hierarchical: TestConfig,
        #[config(default, flatten)]
        flattened: TestConfig,
    }

    #[test]
    fn getting_config_metadata() {
        let metadata = &TestConfig::DESCRIPTION;
        assert_eq!(metadata.ty.name_in_code(), "TestConfig");
        assert_eq!(metadata.help, "# Test configuration\nExtended description.");
        assert_eq!(metadata.help_header(), Some("Test configuration"));
        assert_eq!(metadata.params.len(), 2);

        let str_metadata = &metadata.params[0];
        assert_eq!(str_metadata.name, "str");
        assert_eq!(str_metadata.aliases, ["string"]);
        assert_eq!(str_metadata.help, "String value.");
        assert_eq!(str_metadata.rust_type.name_in_code(), "String");
        assert_eq!(
            format!("{:?}", str_metadata.default_value().unwrap()),
            "\"default\""
        );

        let optional_metadata = &metadata.params[1];
        assert_eq!(optional_metadata.name, "optional");
        assert_eq!(optional_metadata.aliases, [] as [&str; 0]);
        assert_eq!(optional_metadata.help, "Optional value.");
        assert_eq!(optional_metadata.rust_type.name_in_code(), "Option"); // FIXME: does `Option<u32>` get printed only for nightly Rust?
        assert_eq!(optional_metadata.expecting, BasicTypes::INTEGER);
    }

    const EXPECTED_HELP: &str = r#"
# Test configuration
Extended description.

str
string
    Type: string [Rust: String], default: "default"
    String value.

optional
    Type: integer [Rust: Option], default: None
    Optional value.
"#;

    #[test]
    fn printing_schema_help() {
        let schema = ConfigSchema::default().insert::<TestConfig>("").unwrap();
        let mut buffer = vec![];
        schema.write_help(&mut buffer, |_| true).unwrap();
        let buffer = String::from_utf8(buffer).unwrap();
        assert_eq!(buffer.trim(), EXPECTED_HELP.trim(), "{buffer}");
    }

    #[test]
    fn using_alias() {
        let schema = ConfigSchema::default()
            .insert_aliased::<TestConfig>("test", [Alias::prefix("")])
            .unwrap();

        let all_prefixes: HashSet<_> = schema.prefixes_with_aliases().collect();
        assert_eq!(all_prefixes, HashSet::from([Pointer("test"), Pointer("")]));
        let config_prefixes: Vec<_> = schema.locate(&TestConfig::DESCRIPTION).collect();
        assert_eq!(config_prefixes, ["test"]);
        let config_ref = schema.single(&TestConfig::DESCRIPTION).unwrap();
        assert_eq!(config_ref.prefix(), "test");
        assert_eq!(config_ref.aliases().count(), 1);

        let env =
            Environment::from_iter("APP_", [("APP_TEST_STR", "test"), ("APP_OPTIONAL", "123")]);

        let parser = ConfigRepository::new(&schema).with(env);
        assert_eq!(
            parser.merged().get(Pointer("test.str")).unwrap().inner,
            Value::String("test".into())
        );
        assert_eq!(
            parser.merged().get(Pointer("test.optional")).unwrap().inner,
            Value::Number(123_u64.into())
        );

        let config: TestConfig = parser.single().unwrap().parse().unwrap();
        assert_eq!(config.str, "test");
        assert_eq!(config.optional_int, Some(123));
    }

    #[test]
    fn using_multiple_aliases() {
        let schema = ConfigSchema::default()
            .insert_aliased::<TestConfig>(
                "test",
                [
                    Alias::prefix("").exclude(|name| name == "optional"),
                    Alias::prefix("deprecated"),
                ],
            )
            .unwrap();

        let all_prefixes: HashSet<_> = schema.prefixes_with_aliases().collect();
        assert_eq!(
            all_prefixes,
            HashSet::from([Pointer("test"), Pointer(""), Pointer("deprecated")])
        );
        let config_prefixes: Vec<_> = schema.locate(&TestConfig::DESCRIPTION).collect();
        assert_eq!(config_prefixes, ["test"]);
        let config_ref = schema.single(&TestConfig::DESCRIPTION).unwrap();
        assert_eq!(config_ref.prefix(), "test");
        assert_eq!(config_ref.aliases().count(), 2);

        let env = Environment::from_iter(
            "APP_",
            [
                ("APP_TEST_STR", "?"),
                ("APP_OPTIONAL", "123"), // should not be used (excluded from alias)
                ("APP_DEPRECATED_STR", "!"), // should not be used (original var is defined)
                ("APP_DEPRECATED_OPTIONAL", "321"),
            ],
        );
        let config: TestConfig = ConfigRepository::new(&schema)
            .with(env)
            .single()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(config.str, "?");
        assert_eq!(config.optional_int, Some(321));
    }

    #[test]
    fn using_nesting() {
        let schema = ConfigSchema::default().insert::<NestingConfig>("").unwrap();

        let all_prefixes: HashSet<_> = schema.prefixes_with_aliases().collect();
        assert_eq!(
            all_prefixes,
            HashSet::from([Pointer(""), Pointer("hierarchical")])
        );

        let config_prefixes: Vec<_> = schema.locate(&NestingConfig::DESCRIPTION).collect();
        assert_eq!(config_prefixes, [""]);
        let config_prefixes: HashSet<_> = schema.locate(&TestConfig::DESCRIPTION).collect();
        assert_eq!(config_prefixes, HashSet::from(["", "hierarchical"]));

        let err = schema
            .single(&TestConfig::DESCRIPTION)
            .unwrap_err()
            .to_string();
        assert!(err.contains("at least 2 locations"), "{err}");

        let env = Environment::from_iter(
            "",
            [
                ("bool_value", "true"),
                ("hierarchical_string", "???"),
                ("str", "!!!"),
                ("optional", "777"),
            ],
        );
        let repo = ConfigRepository::new(&schema).with(env);
        assert_eq!(
            repo.merged().get(Pointer("bool_value")).unwrap().inner,
            Value::Bool(true)
        );
        assert_eq!(
            repo.merged()
                .get(Pointer("hierarchical.str"))
                .unwrap()
                .inner,
            Value::String("???".into())
        );
        assert_eq!(
            repo.merged().get(Pointer("optional")).unwrap().inner,
            Value::Number("777".parse().unwrap())
        );

        let config: NestingConfig = repo.single().unwrap().parse().unwrap();
        assert!(config.bool_value);
        assert_eq!(config.hierarchical.str, "???");
        assert_eq!(config.hierarchical.optional_int, None);
        assert_eq!(config.flattened.str, "!!!");
        assert_eq!(config.flattened.optional_int, Some(777));
    }

    #[derive(Debug, DescribeConfig)]
    #[config(crate = crate)]
    struct BogusParamConfig {
        #[allow(dead_code)]
        hierarchical: u64,
    }

    #[derive(Debug, DescribeConfig)]
    #[config(crate = crate)]
    struct BogusParamTypeConfig {
        #[allow(dead_code)]
        bool_value: u64,
    }

    #[derive(Debug, DescribeConfig)]
    #[config(crate = crate)]
    struct BogusNestedConfig {
        #[allow(dead_code)]
        #[config(nest)]
        str: TestConfig,
    }

    #[test]
    fn mountpoint_errors() {
        let schema = ConfigSchema::default()
            .insert::<NestingConfig>("test")
            .unwrap();
        assert_matches!(
            schema.mounting_points["test.hierarchical"],
            MountingPoint::Config
        );
        assert_matches!(
            schema.mounting_points["test.bool_value"],
            MountingPoint::Param {
                expecting: BasicTypes::BOOL
            }
        );
        assert_matches!(
            schema.mounting_points["test.str"],
            MountingPoint::Param {
                expecting: BasicTypes::STRING
            }
        );
        assert_matches!(
            schema.mounting_points["test.string"],
            MountingPoint::Param {
                expecting: BasicTypes::STRING
            }
        );
        assert_matches!(
            schema.mounting_points["test.hierarchical.str"],
            MountingPoint::Param {
                expecting: BasicTypes::STRING
            }
        );

        let err = schema
            .clone()
            .insert::<BogusParamConfig>("test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("[Rust field: `hierarchical`]"), "{err}");
        assert!(err.contains("config(s) are already mounted"), "{err}");

        let err = schema
            .clone()
            .insert::<BogusNestedConfig>("test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Cannot mount config"), "{err}");
        assert!(err.contains("at `test.str`"), "{err}");
        assert!(err.contains("parameter(s) are already mounted"), "{err}");

        let err = schema
            .clone()
            .insert::<BogusNestedConfig>("test.bool_value")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Cannot mount config"), "{err}");
        assert!(err.contains("at `test.bool_value`"), "{err}");
        assert!(err.contains("parameter(s) are already mounted"), "{err}");

        let err = schema
            .clone()
            .insert::<BogusParamTypeConfig>("test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Cannot insert param"), "{err}");
        assert!(err.contains("at `test.bool_value`"), "{err}");
        assert!(err.contains("expects integer"), "{err}");
    }
}
