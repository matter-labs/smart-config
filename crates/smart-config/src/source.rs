use std::{collections::HashMap, env, iter};

use anyhow::Context as _;
use serde::de::DeserializeOwned;

use crate::{
    metadata::{ConfigMetadata, DescribeConfig},
    value::{Map, Value, ValueOrigin, ValueWithOrigin},
};

/// A key-value configuration source (e.g., environment variables).
#[derive(Debug)]
pub struct Environment {
    map: HashMap<String, ValueWithOrigin>,
}

impl Environment {
    /// Loads environment variables with the specified prefix.
    pub fn prefixed(prefix: &str) -> Self {
        Self::custom(prefix, env::vars())
    }

    /// Creates a custom environment.
    pub fn custom(prefix: &str, env: impl IntoIterator<Item = (String, String)>) -> Self {
        let map = env.into_iter().filter_map(|(name, value)| {
            let retained_name = name.strip_prefix(prefix)?.to_lowercase();
            Some((
                retained_name,
                ValueWithOrigin {
                    inner: Value::String(value),
                    origin: ValueOrigin::env_var(&name),
                },
            ))
        });
        Self { map: map.collect() }
    }
}

#[derive(Debug)]
enum ConfigSource {
    Env(Environment),
}

/// Configuration repository containing zero or more configuration sources.
#[derive(Debug, Default)]
pub struct ConfigRepository {
    sources: Vec<ConfigSource>,
}

impl ConfigRepository {
    pub fn with_env(mut self, env: Environment) -> Self {
        self.sources.push(ConfigSource::Env(env));
        self
    }

    // FIXME: decouple parsing and building config
    pub fn build<C: DescribeConfig + DeserializeOwned>(self) -> anyhow::Result<C> {
        let map = self.sources.into_iter().flat_map(|source| match source {
            ConfigSource::Env(env) => env.map,
        });
        let mut map = ValueWithOrigin {
            inner: Value::Object(map.collect()),
            origin: ValueOrigin("global configuration".into()),
        };

        map.inner.nest(C::describe_config())?;
        let original = map.clone();
        map.inner.merge_params(&original, C::describe_config())?;
        C::deserialize(map).map_err(Into::into)
    }
}

impl Value {
    fn nest(&mut self, config: &ConfigMetadata) -> anyhow::Result<()> {
        let Self::Object(map) = self else {
            anyhow::bail!("expected object");
        };

        for nested in &*config.nested_configs {
            let name = nested.name;
            if name.is_empty() {
                continue;
            }

            if !map.contains_key(name) {
                let matching_keys = map.iter().filter_map(|(key, value)| {
                    if key.starts_with(name) && key.as_bytes().get(name.len()) == Some(&b'_') {
                        return Some((key[name.len() + 1..].to_owned(), value.clone()));
                    }
                    None
                });
                map.insert(
                    name.to_owned(),
                    ValueWithOrigin {
                        inner: Value::Object(matching_keys.collect()),
                        origin: ValueOrigin::group(nested),
                    },
                );
            }

            // `unwrap` is safe: the value has just been inserted
            let nested_value = &mut map.get_mut(name).unwrap().inner;
            nested_value
                .nest(nested.meta)
                .with_context(|| format!("nesting {}", nested.name))?;
        }
        Ok(())
    }

    fn merge_params(
        &mut self,
        original: &ValueWithOrigin,
        config: &ConfigMetadata,
    ) -> anyhow::Result<()> {
        let Self::Object(map) = self else {
            anyhow::bail!("expected object");
        };

        for param in &*config.params {
            if param.merge_from.is_empty() {
                continue; // Skip computations in the common case.
            }

            let all_param_names = iter::once(param.name).chain(param.aliases.iter().copied());
            let value_is_set = all_param_names.clone().any(|name| map.contains_key(name));
            if value_is_set {
                continue;
            }

            for &pointer in param.merge_from {
                for name in all_param_names.clone() {
                    if let Some(value) = original.pointer(pointer, name) {
                        map.insert(param.name.to_owned(), value.clone());
                        break;
                    }
                }
            }
        }

        // Recurse into nested configs
        for nested in &*config.nested_configs {
            let name = nested.name;
            let nested_value = if name.is_empty() {
                &mut *self
            } else {
                let Self::Object(map) = self else {
                    unreachable!()
                };
                if !map.contains_key(name) {
                    map.insert(
                        name.to_owned(),
                        ValueWithOrigin {
                            inner: Value::Object(Map::new()),
                            origin: ValueOrigin::group(nested),
                        },
                    );
                }
                &mut map.get_mut(name).unwrap().inner
            };
            nested_value
                .merge_params(original, nested.meta)
                .with_context(|| format!("merging params as {}", nested.name))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde::Deserialize;

    use super::*;
    use crate::metadata::EmptyConfig;

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
        #[config(merge_from("/deprecated"))]
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
        let env = Environment::custom(
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
        let env = Environment::custom(
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
    #[config(crate = crate, merge_from("/deprecated"))]
    struct ConfigWithNesting {
        value: u32,
        #[config(merge_from())]
        #[serde(default)]
        not_merged: String,
        #[config(nested)]
        nested: NestedConfig,

        #[config(nested)]
        #[serde(rename = "deprecated")]
        _deprecated: EmptyConfig,
    }

    #[test]
    fn nesting_json() {
        let env = Environment::custom(
            "",
            [
                ("value".to_owned(), "123".to_owned()),
                ("nested_renamed".to_owned(), "first".to_owned()),
                ("nested_other_int".to_owned(), "321".to_owned()),
            ],
        );
        let mut map = ValueWithOrigin {
            inner: Value::Object(env.map),
            origin: ValueOrigin("test".into()),
        };

        map.inner
            .nest(ConfigWithNesting::describe_config())
            .unwrap();
        assert_eq!(
            map.pointer("/", "value").unwrap().inner,
            Value::String("123".to_owned())
        );
        assert_eq!(
            map.pointer("/nested", "renamed").unwrap().inner,
            Value::String("first".to_owned())
        );
        assert_eq!(
            map.pointer("/nested/", "other_int").unwrap().inner,
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
        let env = Environment::custom(
            "",
            [
                ("deprecated_value".to_owned(), "4".to_owned()),
                ("nested_renamed".to_owned(), "first".to_owned()),
            ],
        );

        let config: ConfigWithNesting = ConfigRepository::default().with_env(env).build().unwrap();
        assert_eq!(config.value, 4);
        assert_eq!(config.nested.simple_enum, SimpleEnum::First);
        assert_eq!(config.nested.other_int, 42);

        let env = Environment::custom(
            "",
            [
                ("value".to_owned(), "123".to_owned()),
                ("deprecated_value".to_owned(), "4".to_owned()),
                ("nested_renamed".to_owned(), "first".to_owned()),
                ("deprecated_other_int".to_owned(), "321".to_owned()),
                ("deprecated_not_merged".to_owned(), "!".to_owned()),
            ],
        );

        let config: ConfigWithNesting = ConfigRepository::default().with_env(env).build().unwrap();
        assert_eq!(config.value, 123);
        assert_eq!(config.not_merged, "");
        assert_eq!(config.nested.simple_enum, SimpleEnum::First);
        assert_eq!(config.nested.other_int, 321);
    }
}
