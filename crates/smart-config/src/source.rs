use std::{
    any,
    collections::{BTreeSet, HashMap},
    env, mem,
};

use anyhow::Context as _;
use serde::de::DeserializeOwned;

use crate::{
    metadata::DescribeConfig,
    schema::{Alias, ConfigData, ConfigSchema},
    value::{Map, Pointer, Value, ValueOrigin, ValueWithOrigin},
};

/// A key-value configuration source (e.g., environment variables).
#[derive(Debug, Default)]
pub struct Environment {
    map: HashMap<String, ValueWithOrigin>,
}

impl Environment {
    /// Loads environment variables with the specified prefix.
    pub fn prefixed(prefix: &str) -> Self {
        Self::from_iter(prefix, env::vars())
    }

    /// Creates a custom environment.
    pub fn from_iter<S>(prefix: &str, env: impl IntoIterator<Item = (S, S)>) -> Self
    where
        S: AsRef<str> + Into<String>,
    {
        let map = env.into_iter().filter_map(|(name, value)| {
            let retained_name = name.as_ref().strip_prefix(prefix)?.to_lowercase();
            Some((
                retained_name,
                ValueWithOrigin {
                    inner: Value::String(value.into()),
                    origin: ValueOrigin::env_var(name.as_ref()),
                },
            ))
        });
        Self { map: map.collect() }
    }

    /// Converts a logical prefix like `api.limits` to `API_LIMITS_`.
    fn env_prefix(prefix: &str) -> String {
        let mut prefix = prefix.replace('.', "_");
        if !prefix.is_empty() && !prefix.ends_with('_') {
            prefix.push('_');
        }
        prefix
    }

    fn take_matching_entries(&mut self, prefix: Pointer) -> Vec<(String, ValueWithOrigin)> {
        let mut matching_entries = vec![];
        let env_prefix = Self::env_prefix(prefix.0);
        self.map.retain(|name, value| {
            if let Some(name_suffix) = name.strip_prefix(&env_prefix) {
                let value = mem::replace(value, ValueWithOrigin::empty());
                matching_entries.push((name_suffix.to_owned(), value));
                false
            } else {
                true
            }
        });
        matching_entries
    }
}

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
    pub fn with_env(mut self, env: Environment) -> Self {
        self.env.map.extend(env.map);
        self
    }

    pub fn parser(mut self, schema: &ConfigSchema) -> anyhow::Result<ConfigParser<'_>> {
        let mut map = ValueWithOrigin {
            inner: Value::Object(Map::new()),
            origin: ValueOrigin("global configuration".into()),
        };

        let all_objects: BTreeSet<_> = schema
            .configs
            .values()
            .flat_map(ConfigData::all_prefixes)
            .flat_map(Pointer::with_ancestors)
            .chain([Pointer("")])
            .collect();
        for &object_ptr in &all_objects {
            map.ensure_object(object_ptr)?;
        }
        for &object_ptr in all_objects.iter().rev() {
            map.copy_key_value_entries(object_ptr, &mut self.env);
        }

        for config_data in schema.configs.values() {
            for alias in &config_data.aliases {
                map.merge_alias(Pointer(&config_data.prefix), alias);
            }
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
    /// Parses a configuration.
    pub fn parse<C>(&self) -> anyhow::Result<C>
    where
        C: DescribeConfig + DeserializeOwned,
    {
        let config_data = self
            .schema
            .configs
            .get(&any::TypeId::of::<C>())
            .with_context(|| {
                format!(
                    "Config `{}` is not a part of the schema",
                    any::type_name::<C>()
                )
            })?;
        // FIXME: implement `Deserializer` for `&ValueWithOrigin`
        C::deserialize(self.map.clone())
            .with_context(|| {
                let summary = if let Some(header) = config_data.metadata.help_header() {
                    format!(" ({})", header.trim().to_lowercase())
                } else {
                    String::new()
                };
                format!(
                    "error parsing configuration `{name}`{summary} at `{prefix}` (aliases: {aliases:?})",
                    name = config_data.metadata.ty.name_in_code(),
                    prefix = config_data.prefix,
                    aliases = config_data.aliases
                )
            })
    }
}

impl ValueWithOrigin {
    /// Ensures that there is an object (possibly empty) at the specified location. Returns an error
    /// if the locations contains anything other than an object.
    fn ensure_object(&mut self, at: Pointer<'_>) -> anyhow::Result<()> {
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
                    origin: ValueOrigin("".into()), // FIXME
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
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde::Deserialize;

    use super::*;
    use crate::schema::Alias;

    #[derive(Debug, PartialEq, Eq, Hash, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum SimpleEnum {
        First,
        Second,
    }

    #[derive(Debug, Deserialize, DescribeConfig)]
    #[config(crate = crate)]
    struct NestedConfig {
        #[serde(rename = "renamed")]
        simple_enum: SimpleEnum,
        #[serde(default = "NestedConfig::default_other_int")]
        other_int: u32,
    }

    impl NestedConfig {
        const fn default_other_int() -> u32 {
            42
        }
    }

    #[derive(Debug, Deserialize)]
    struct TestConfig {
        int: u64,
        bool: bool,
        string: String,
        optional: Option<i64>,
        array: Vec<u32>,
        repeated: HashSet<SimpleEnum>,
        #[serde(flatten)]
        nested: NestedConfig,
    }

    fn wrap_into_value(env: Environment) -> ValueWithOrigin {
        ValueWithOrigin {
            inner: Value::Object(env.map),
            origin: ValueOrigin("test".into()),
        }
    }

    #[test]
    fn parsing() {
        let env = Environment::from_iter(
            "",
            [
                ("int".to_owned(), "1".to_owned()),
                ("bool".to_owned(), "true".to_owned()),
                ("string".to_owned(), "??".to_owned()),
                ("array".to_owned(), "1,2,3".to_owned()),
                ("renamed".to_owned(), "first".to_owned()),
                ("repeated".to_owned(), "second,first".to_owned()),
            ],
        );
        let env = wrap_into_value(env);

        let config = TestConfig::deserialize(env).unwrap();
        assert_eq!(config.int, 1);
        assert_eq!(config.optional, None);
        assert!(config.bool);
        assert_eq!(config.string, "??");
        assert_eq!(config.array, [1, 2, 3]);
        assert_eq!(
            config.repeated,
            HashSet::from([SimpleEnum::First, SimpleEnum::Second])
        );
        assert_eq!(config.nested.simple_enum, SimpleEnum::First);
        assert_eq!(config.nested.other_int, 42);
    }

    #[test]
    fn parsing_errors() {
        let env = Environment::from_iter(
            "",
            [
                ("renamed".to_owned(), "first".to_owned()),
                ("other_int".to_owned(), "what".to_owned()),
            ],
        );
        let err = NestedConfig::deserialize(wrap_into_value(env)).unwrap_err();

        assert!(err.inner.to_string().contains("u32 value 'what'"), "{err}");
        assert!(
            err.origin.as_ref().unwrap().0.contains("other_int"),
            "{err}"
        );
    }

    #[derive(Debug, Deserialize, DescribeConfig)]
    #[config(crate = crate)]
    struct ConfigWithNesting {
        value: u32,
        #[serde(default)]
        not_merged: String,
        #[config(nested)]
        nested: NestedConfig,
    }

    #[test]
    fn nesting_json() {
        let env = Environment::from_iter(
            "",
            [
                ("value".to_owned(), "123".to_owned()),
                ("nested_renamed".to_owned(), "first".to_owned()),
                ("nested_other_int".to_owned(), "321".to_owned()),
            ],
        );

        let schema = ConfigSchema::default().insert::<ConfigWithNesting>("");
        let map = ConfigRepository::from(env).parser(&schema).unwrap().map;

        assert_eq!(
            map.get(Pointer("value")).unwrap().inner,
            Value::String("123".to_owned())
        );
        assert_eq!(
            map.get(Pointer("nested.renamed")).unwrap().inner,
            Value::String("first".to_owned())
        );
        assert_eq!(
            map.get(Pointer("nested.other_int")).unwrap().inner,
            Value::String("321".to_owned())
        );

        let Value::Object(global) = &map.inner else {
            panic!("unexpected map: {map:#?}");
        };
        let nested = &global["nested"];
        let Value::Object(nested) = &nested.inner else {
            panic!("unexpected nested value: {nested:#?}");
        };

        assert_eq!(nested["renamed"].inner, Value::String("first".into()));
        assert_eq!(nested["other_int"].inner, Value::String("321".into()));

        let config = ConfigWithNesting::deserialize(map).unwrap();
        assert_eq!(config.value, 123);
        assert_eq!(config.nested.simple_enum, SimpleEnum::First);
        assert_eq!(config.nested.other_int, 321);
    }

    #[test]
    fn merging_config_parts() {
        let env = Environment::from_iter(
            "",
            [
                ("deprecated_value".to_owned(), "4".to_owned()),
                ("nested_renamed".to_owned(), "first".to_owned()),
            ],
        );

        let alias = Alias::prefix("deprecated").exclude(|name| name == "not_merged");
        let schema = ConfigSchema::default().insert_aliased::<ConfigWithNesting>("", [alias]);
        let config: ConfigWithNesting = ConfigRepository::from(env)
            .parser(&schema)
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(config.value, 4);
        assert_eq!(config.nested.simple_enum, SimpleEnum::First);
        assert_eq!(config.nested.other_int, 42);

        let env = Environment::from_iter(
            "",
            [
                ("value".to_owned(), "123".to_owned()),
                ("deprecated_value".to_owned(), "4".to_owned()),
                ("nested_renamed".to_owned(), "first".to_owned()),
                ("deprecated_other_int".to_owned(), "321".to_owned()),
                ("deprecated_not_merged".to_owned(), "!".to_owned()),
            ],
        );

        let config: ConfigWithNesting = ConfigRepository::from(env)
            .parser(&schema)
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(config.value, 123);
        assert_eq!(config.not_merged, "");
        assert_eq!(config.nested.simple_enum, SimpleEnum::First);
        assert_eq!(config.nested.other_int, 321);
    }
}