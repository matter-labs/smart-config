use std::{env, sync::Arc};

use anyhow::Context as _;

use super::ConfigSource;
use crate::value::{FileFormat, Map, ValueOrigin, WithOrigin};

/// Configuration sourced from environment variables.
#[derive(Debug, Clone)]
pub struct Environment {
    origin: Arc<ValueOrigin>,
    map: Map<String>,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            origin: Arc::new(ValueOrigin::EnvVars),
            map: Map::new(),
        }
    }
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
        let origin = Arc::new(ValueOrigin::EnvVars);
        let map = env.into_iter().filter_map(|(name, value)| {
            let retained_name = name.as_ref().strip_prefix(prefix)?.to_lowercase();
            Some((
                retained_name,
                WithOrigin {
                    inner: value.into(),
                    origin: Arc::new(ValueOrigin::Path {
                        source: origin.clone(),
                        path: name.into(),
                    }),
                },
            ))
        });
        let map = map.collect();
        Self { origin, map }
    }

    /// Adds additional variables to this environment. This is useful if the added vars don't have the necessary prefix.
    #[must_use]
    pub fn with_vars(mut self, var_names: &[&str]) -> Self {
        let origin = Arc::new(ValueOrigin::EnvVars);
        let defined_vars = var_names.iter().filter_map(|&name| {
            let value = env::var_os(name)?.into_string().ok()?;
            Some((
                name.to_owned(),
                WithOrigin {
                    inner: value,
                    origin: Arc::new(ValueOrigin::Path {
                        source: origin.clone(),
                        path: name.to_owned(),
                    }),
                },
            ))
        });
        self.map.extend(defined_vars);
        self
    }

    #[doc(hidden)] // FIXME: functionally incomplete ('' strings, interpolation, comments after vars)
    pub fn from_dotenv(filename: &str, contents: &str) -> anyhow::Result<Self> {
        let origin = Arc::new(ValueOrigin::File {
            name: filename.to_owned(),
            format: FileFormat::Dotenv,
        });
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
                name.to_lowercase(),
                WithOrigin {
                    inner: variable_value.to_owned(),
                    origin: Arc::new(ValueOrigin::Path {
                        source: origin.clone(),
                        path: name.into(),
                    }),
                },
            );
        }
        Ok(Self { origin, map })
    }

    /// Strips a prefix from all contained vars and returns the filtered vars.
    #[must_use]
    pub fn strip_prefix(self, prefix: &str) -> Self {
        let prefix = prefix.to_lowercase();
        let filtered = self
            .map
            .into_iter()
            .filter_map(|(name, value)| Some((name.strip_prefix(&prefix)?.to_owned(), value)));
        Self {
            origin: self.origin,
            map: filtered.collect(),
        }
    }
}

impl ConfigSource for Environment {
    type Map = Map<String>;

    fn into_contents(self) -> WithOrigin<Self::Map> {
        WithOrigin::new(self.map, self.origin)
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn parsing_dotenv_contents() {
        let env = Environment::from_dotenv(
            "test.env",
            r#"
            APP_TEST=what
            APP_OTHER="test string"

            # Overwriting vars should be supported
            APP_TEST=42
            "#,
        )
        .unwrap();

        assert_eq!(env.map.len(), 2, "{:?}", env.map);
        assert_eq!(env.map["app_test"].inner, "42");
        let origin = &env.map["app_test"].origin;
        let ValueOrigin::Path { path, source } = origin.as_ref() else {
            panic!("unexpected origin: {origin:?}");
        };
        assert_eq!(path, "APP_TEST");
        assert_matches!(
            source.as_ref(),
            ValueOrigin::File { name, format: FileFormat::Dotenv } if name == "test.env"
        );
        assert_eq!(env.map["app_other"].inner, "test string");

        let env = env.strip_prefix("app_");
        assert_eq!(env.map.len(), 2, "{:?}", env.map);
        assert_eq!(env.map["test"].inner, "42");
        assert_matches!(env.map["test"].origin.as_ref(), ValueOrigin::Path { path, .. } if path == "APP_TEST");
        assert_eq!(env.map["other"].inner, "test string");
    }
}
