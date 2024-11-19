//! Configuration schema.

use std::{
    any,
    borrow::Cow,
    collections::{HashMap, HashSet},
    io, iter,
    marker::PhantomData,
};

use serde::de::DeserializeOwned;

use crate::{
    metadata::{ConfigMetadata, DescribeConfig, NestedConfigMetadata, ParamMetadata},
    value::Pointer,
};

const INDENT: &str = "    ";

/// Alias specification for a config.
#[derive(Debug)]
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
            param_names: C::describe_config()
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

#[derive(Debug)]
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

/// Schema for configuration. Can contain multiple configs bound to different "locations".
#[derive(Default, Debug)]
pub struct ConfigSchema {
    configs: HashMap<(any::TypeId, Cow<'static, str>), ConfigData>,
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

    /// Inserts a new configuration type at the specified place.
    #[must_use]
    pub fn insert<C>(self, prefix: &'static str) -> Self
    where
        C: DescribeConfig + DeserializeOwned,
    {
        self.insert_aliased::<C>(prefix, [])
    }

    /// Inserts a new configuration type at the specified place with potential aliases.
    #[must_use]
    pub fn insert_aliased<C>(
        mut self,
        prefix: &'static str,
        aliases: impl IntoIterator<Item = Alias<C>>,
    ) -> Self
    where
        C: DescribeConfig + DeserializeOwned,
    {
        let metadata = C::describe_config();
        self.insert_inner(
            prefix.into(),
            any::TypeId::of::<C>(),
            ConfigData {
                aliases: aliases.into_iter().map(Alias::drop_type_param).collect(),
                metadata,
            },
        );

        // Insert all nested configs recursively.
        let mut pending_configs: Vec<_> =
            Self::list_nested_configs(prefix, &metadata.nested_configs).collect();
        while let Some((prefix, metadata)) = pending_configs.pop() {
            let new_configs = Self::list_nested_configs(&prefix, &metadata.nested_configs);
            pending_configs.extend(new_configs);

            self.insert_inner(prefix.into(), metadata.ty.id(), ConfigData::new(metadata));
        }
        self
    }

    fn list_nested_configs<'a>(
        prefix: &'a str,
        nested: &'a [NestedConfigMetadata],
    ) -> impl Iterator<Item = (String, &'static ConfigMetadata)> + 'a {
        nested
            .iter()
            .map(|nested| (Pointer(prefix).join(nested.name), nested.meta))
    }

    fn insert_inner(&mut self, prefix: Cow<'static, str>, type_id: any::TypeId, data: ConfigData) {
        self.configs.insert((type_id, prefix), data);
    }

    /// Writes help about this schema to the provided writer.
    ///
    /// `param_filter` can be used to filter displayed parameters.
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

    fn write_parameter(
        writer: &mut impl io::Write,
        prefix: &str,
        param: &ParamMetadata,
        config_data: &ConfigData,
    ) -> io::Result<()> {
        let prefix_sep = if prefix.is_empty() || prefix.ends_with('.') {
            ""
        } else {
            "."
        };
        writeln!(writer, "{prefix}{prefix_sep}{}", param.name)?;

        let local_aliases = param.aliases.iter().copied();
        let global_aliases = config_data.aliases.iter().flat_map(|alias| {
            local_aliases
                .clone()
                .chain([param.name])
                .filter_map(|name| {
                    alias
                        .param_names
                        .contains(name)
                        .then_some((alias.prefix.0, name))
                })
        });
        let local_aliases = local_aliases.clone().map(|name| (prefix, name));
        for (prefix, alias) in local_aliases.chain(global_aliases) {
            let prefix_sep = if prefix.is_empty() || prefix.ends_with('.') {
                ""
            } else {
                "."
            };
            writeln!(writer, "{prefix}{prefix_sep}{alias}")?;
        }

        let ty = if let Some(kind) = param.base_type.kind() {
            format!("{kind} [Rust: {}]", param.ty.name_in_code())
        } else {
            param.ty.name_in_code().to_owned()
        };
        let default = if let Some(default) = param.default_value() {
            format!(", default: {default:?}")
        } else {
            String::new()
        };
        let unit = if let Some(unit) = &param.unit {
            format!(" [unit: {unit}]")
        } else {
            String::new()
        };
        writeln!(writer, "{INDENT}Type: {ty}{default}{unit}")?;

        if !param.help.is_empty() {
            for line in param.help.lines() {
                writeln!(writer, "{INDENT}{line}")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::{metadata::DescribeConfig, value::Value, ConfigRepository, Environment};

    /// # Test configuration
    ///
    /// Extended description.
    #[derive(Debug, Default, PartialEq, Deserialize, DescribeConfig)]
    #[config(crate = crate)]
    struct TestConfig {
        /// String value.
        #[serde(alias = "string", default = "TestConfig::default_str")]
        str: String,
        /// Optional value.
        #[serde(rename = "optional")]
        optional_int: Option<u32>,
    }

    impl TestConfig {
        fn default_str() -> String {
            "default".to_owned()
        }
    }

    #[derive(Debug, Default, PartialEq, Deserialize, DescribeConfig)]
    #[config(crate = crate)]
    struct NestingConfig {
        #[serde(default)]
        bool_value: bool,
        /// Hierarchical nested config.
        #[serde(default)]
        #[config(nested)]
        hierarchical: TestConfig,
        #[serde(flatten, default)]
        #[config(nested)]
        flattened: TestConfig,
    }

    #[test]
    fn getting_config_metadata() {
        let metadata = TestConfig::describe_config();
        assert_eq!(metadata.ty.name_in_code(), "TestConfig");
        assert_eq!(metadata.help, "# Test configuration\nExtended description.");
        assert_eq!(metadata.help_header(), Some("Test configuration"));
        assert_eq!(metadata.params.len(), 2);

        let str_metadata = &metadata.params[0];
        assert_eq!(str_metadata.name, "str");
        assert_eq!(str_metadata.aliases, ["string"]);
        assert_eq!(str_metadata.help, "String value.");
        assert_eq!(str_metadata.ty.name_in_code(), "String");
        assert_eq!(
            format!("{:?}", str_metadata.default_value().unwrap()),
            "\"default\""
        );

        let optional_metadata = &metadata.params[1];
        assert_eq!(optional_metadata.name, "optional");
        assert_eq!(optional_metadata.aliases, [] as [&str; 0]);
        assert_eq!(optional_metadata.help, "Optional value.");
        assert_eq!(optional_metadata.ty.name_in_code(), "Option"); // FIXME: does `Option<u32>` get printed only for nightly Rust?
        assert_eq!(optional_metadata.base_type.name_in_code(), "u32");
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
        let schema = ConfigSchema::default().insert::<TestConfig>("");
        let mut buffer = vec![];
        schema.write_help(&mut buffer, |_| true).unwrap();
        let buffer = String::from_utf8(buffer).unwrap();
        assert_eq!(buffer.trim(), EXPECTED_HELP.trim(), "{buffer}");
    }

    #[test]
    fn using_alias() {
        let schema =
            ConfigSchema::default().insert_aliased::<TestConfig>("test", [Alias::prefix("")]);

        let all_prefixes: HashSet<_> = schema.prefixes_with_aliases().collect();
        assert_eq!(all_prefixes, HashSet::from([Pointer("test"), Pointer("")]));
        let config_prefixes: Vec<_> = schema.locate(TestConfig::describe_config()).collect();
        assert_eq!(config_prefixes, ["test"]);
        let config_ref = schema.single(TestConfig::describe_config()).unwrap();
        assert_eq!(config_ref.prefix(), "test");
        assert_eq!(config_ref.aliases().count(), 1);

        let env =
            Environment::from_iter("APP_", [("APP_TEST_STR", "test"), ("APP_OPTIONAL", "123")]);

        let parser = ConfigRepository::from(env).parser(&schema).unwrap();
        assert_eq!(
            parser.map().get(Pointer("test.str")).unwrap().inner,
            Value::String("test".into())
        );
        assert_eq!(
            parser.map().get(Pointer("test.optional")).unwrap().inner,
            Value::String("123".into())
        );

        let config: TestConfig = parser.parse().unwrap();
        assert_eq!(config.str, "test");
        assert_eq!(config.optional_int, Some(123));
    }

    #[test]
    fn using_multiple_aliases() {
        let schema = ConfigSchema::default().insert_aliased::<TestConfig>(
            "test",
            [
                Alias::prefix("").exclude(|name| name == "optional"),
                Alias::prefix("deprecated"),
            ],
        );

        let all_prefixes: HashSet<_> = schema.prefixes_with_aliases().collect();
        assert_eq!(
            all_prefixes,
            HashSet::from([Pointer("test"), Pointer(""), Pointer("deprecated")])
        );
        let config_prefixes: Vec<_> = schema.locate(TestConfig::describe_config()).collect();
        assert_eq!(config_prefixes, ["test"]);
        let config_ref = schema.single(TestConfig::describe_config()).unwrap();
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
        let config: TestConfig = ConfigRepository::from(env)
            .parser(&schema)
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(config.str, "?");
        assert_eq!(config.optional_int, Some(321));
    }

    #[test]
    fn using_nesting() {
        let schema = ConfigSchema::default().insert::<NestingConfig>("");

        let all_prefixes: HashSet<_> = schema.prefixes_with_aliases().collect();
        assert_eq!(
            all_prefixes,
            HashSet::from([Pointer(""), Pointer("hierarchical")])
        );

        let config_prefixes: Vec<_> = schema.locate(NestingConfig::describe_config()).collect();
        assert_eq!(config_prefixes, [""]);
        let config_prefixes: HashSet<_> = schema.locate(TestConfig::describe_config()).collect();
        assert_eq!(config_prefixes, HashSet::from(["", "hierarchical"]));

        let err = schema
            .single(TestConfig::describe_config())
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
        let parser = ConfigRepository::from(env).parser(&schema).unwrap();
        assert_eq!(
            parser.map().get(Pointer("bool_value")).unwrap().inner,
            Value::String("true".into())
        );
        assert_eq!(
            parser
                .map()
                .get(Pointer("hierarchical.string"))
                .unwrap()
                .inner,
            Value::String("???".into())
        );

        let config: NestingConfig = parser.parse().unwrap(); // FIXME: doesn't work because of serde(flatten)
        panic!("{config:?}");
    }
}
