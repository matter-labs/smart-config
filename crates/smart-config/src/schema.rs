//! Configuration schema.

use std::{
    any,
    collections::{HashMap, HashSet},
    io, iter,
    marker::PhantomData,
};

use serde::de::DeserializeOwned;

use crate::{
    metadata::{ConfigMetadata, DescribeConfig, ParamMetadata},
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
    pub prefix: Pointer<'static>,
    pub aliases: Vec<Alias<()>>,
    pub metadata: &'static ConfigMetadata,
}

impl ConfigData {
    pub(crate) fn all_prefixes(&self) -> impl Iterator<Item = Pointer<'static>> + '_ {
        iter::once(self.prefix).chain(self.aliases.iter().map(|alias| alias.prefix))
    }
}

/// Schema for configuration. Can contain multiple configs bound to different "locations".
#[derive(Default, Debug)]
pub struct ConfigSchema {
    // FIXME: no guarantee that configs are unique by type ID only (they are unique by type ID + location)
    pub(crate) configs: HashMap<any::TypeId, ConfigData>,
}

impl ConfigSchema {
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
        // FIXME: insert nested configs as well
        let metadata = C::describe_config();
        self.configs.insert(
            any::TypeId::of::<C>(),
            ConfigData {
                prefix: Pointer(prefix),
                aliases: aliases.into_iter().map(Alias::drop_type_param).collect(),
                metadata,
            },
        );
        self
    }

    /// Writes help about this schema to the provided writer.
    ///
    /// `param_filter` can be used to filter displayed parameters.
    pub fn write_help(
        &self,
        writer: &mut impl io::Write,
        mut param_filter: impl FnMut(&ParamMetadata) -> bool,
    ) -> io::Result<()> {
        for config_data in self.configs.values() {
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
                Self::write_parameter(writer, param, config_data)?;
                writeln!(writer)?;
            }
        }
        Ok(())
    }

    fn write_parameter(
        writer: &mut impl io::Write,
        param: &ParamMetadata,
        config_data: &ConfigData,
    ) -> io::Result<()> {
        let prefix = config_data.prefix.0;
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
    use crate::{metadata::DescribeConfig, ConfigRepository, Environment};

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
        assert_eq!(optional_metadata.ty.name_in_code(), "Option<u32>");
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
    Type: integer [Rust: Option<u32>], default: None
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
        let env =
            Environment::from_iter("APP_", [("APP_TEST_STR", "test"), ("APP_OPTIONAL", "123")]);

        let config: TestConfig = ConfigRepository::from(env)
            .parser(&schema)
            .unwrap()
            .parse()
            .unwrap();
        panic!("{config:?}");
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
        panic!("{config:?}");
    }
}
