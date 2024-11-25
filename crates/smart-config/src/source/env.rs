use std::{env, sync::Arc};

use anyhow::Context as _;

use super::{ConfigContents, ConfigSource};
use crate::value::{Map, ValueOrigin, WithOrigin};

/// Configuration sourced from environment variables.
///
/// Use [`KeyValueMap`] for string key–value entries that are not env variables (e.g., command-line args).
#[derive(Debug, Clone, Default)]
pub struct Environment {
    map: Map<String>,
}

impl Environment {
    /// Loads environment variables with the specified prefix.
    pub fn prefixed(prefix: &str) -> Self {
        Self::from_iter(prefix, env::vars())
    }

    /// Creates a custom environment.
    pub fn from_iter<K, V>(prefix: &str, env: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: AsRef<str> + Into<String>,
        V: Into<String>,
    {
        let map = env.into_iter().filter_map(|(name, value)| {
            let retained_name = name.as_ref().strip_prefix(prefix)?.to_lowercase();
            Some((
                retained_name,
                WithOrigin {
                    inner: value.into(),
                    origin: Arc::new(ValueOrigin::EnvVar(name.into())),
                },
            ))
        });
        Self { map: map.collect() }
    }

    /// Adds additional variables to this environment. This is useful if the added vars don't have the necessary prefix.
    pub fn with_vars(mut self, var_names: &[&str]) -> Self {
        let defined_vars = var_names.iter().filter_map(|&name| {
            let value = env::var_os(name)?.into_string().ok()?;
            Some((
                name.to_owned(),
                WithOrigin {
                    inner: value,
                    origin: Arc::new(ValueOrigin::EnvVar(name.to_owned())),
                },
            ))
        });
        self.map.extend(defined_vars);
        self
    }

    // FIXME: functionally incomplete ('' strings, interpolation, comments after vars)
    pub fn from_dotenv(contents: &str) -> anyhow::Result<Self> {
        let mut map = Map::default();
        for line in contents.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (name, variable_value) = line.split_once('=').with_context(|| {
                format!("Incorrect line for setting environment variable: {line}")
            })?;
            let variable_value = variable_value.trim_matches('"');
            map.insert(
                name.to_owned(),
                WithOrigin {
                    inner: variable_value.to_owned(),
                    origin: Arc::new(ValueOrigin::EnvVar(name.into())),
                },
            );
        }
        Ok(Self { map })
    }
}

impl ConfigSource for Environment {
    fn into_contents(self) -> ConfigContents {
        ConfigContents::KeyValue(self.map)
    }
}

/// Generic key–value configuration source.
#[derive(Debug)]
pub struct KeyValueMap {
    map: Map<String>,
}

impl KeyValueMap {
    /// Creates a new key–value map with the specified name and contents.
    pub fn new<K, V>(name: &str, entries: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        let map_name: Arc<str> = name.into();
        let map = entries
            .into_iter()
            .map(|(key, value)| {
                let key = key.into();
                let value = WithOrigin {
                    inner: value.into(),
                    origin: Arc::new(ValueOrigin::Map {
                        map_name: map_name.clone(),
                        key: key.clone(),
                    }),
                };
                (key, value)
            })
            .collect();
        Self { map }
    }
}

impl ConfigSource for KeyValueMap {
    fn into_contents(self) -> ConfigContents {
        ConfigContents::KeyValue(self.map)
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn parsing_dotenv_contents() {
        let env = Environment::from_dotenv(
            r#"
            TEST=what
            OTHER="test string"

            # Overwriting vars should be supported
            TEST=42
            "#,
        )
        .unwrap();

        assert_eq!(env.map.len(), 2, "{:?}", env.map);
        assert_eq!(env.map["TEST"].inner, "42");
        assert_matches!(env.map["TEST"].origin.as_ref(), ValueOrigin::EnvVar(name) if name == "TEST");
        assert_eq!(env.map["OTHER"].inner, "test string");
    }
}
