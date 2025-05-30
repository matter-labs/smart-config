use std::{env, fmt, mem, sync::Arc};

use anyhow::Context as _;

use super::{ConfigSource, Flat};
use crate::{
    testing::MOCK_ENV_VARS,
    value::{FileFormat, Map, Value, ValueOrigin, WithOrigin},
    Json,
};

/// Configuration sourced from environment variables.
#[derive(Debug, Clone)]
pub struct Environment {
    origin: Arc<ValueOrigin>,
    map: Map,
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
        MOCK_ENV_VARS.with_borrow(|mock_vars| {
            let mock_vars = mock_vars
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()));
            Self::from_iter(prefix, env::vars().chain(mock_vars))
        })
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
                    inner: Value::from(value.into()),
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
                    inner: Value::from(value),
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
                    inner: Value::from(variable_value.to_owned()),
                    origin: Arc::new(ValueOrigin::Path {
                        source: origin.clone(),
                        path: name.into(),
                    }),
                },
            );
        }
        Ok(Self { origin, map })
    }

    /// Iterates over variables in this container.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &WithOrigin)> + '_ {
        self.map.iter().map(|(name, value)| (name.as_str(), value))
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

    /// Coerces JSON values in env variables which names end with the `__json` / `:json` suffixes and strips this suffix.
    ///
    /// # Errors
    ///
    /// Returns an error if any coercion fails; provides a list of all failed coercions. Successful coercions are still applied in this case.
    pub fn coerce_json(&mut self) -> anyhow::Result<()> {
        let mut coerced_values = vec![];
        let mut errors = vec![];
        for (key, value) in &self.map {
            let stripped_key = key
                .strip_suffix("__json")
                .or_else(|| key.strip_suffix(":json"));
            let Some(stripped_key) = stripped_key else {
                continue;
            };
            let Some(value_str) = value.inner.as_plain_str() else {
                // The value was already transformed, probably.
                continue;
            };

            let val = match serde_json::from_str::<serde_json::Value>(value_str) {
                Ok(val) => val,
                Err(err) => {
                    mem::take(&mut coerced_values);
                    errors.push((value.origin.clone(), err));
                    continue;
                }
            };
            if !errors.is_empty() {
                continue; // No need to record coerced values if there are coercion errors.
            }

            let root_origin = Arc::new(ValueOrigin::Synthetic {
                source: value.origin.clone(),
                transform: "parsed JSON string".into(),
            });
            let coerced_value = Json::map_value(val, &root_origin, String::new());
            coerced_values.push((key.to_owned(), stripped_key.to_owned(), coerced_value));
        }

        for (key, stripped_key, coerced_value) in coerced_values {
            self.map.remove(&key);
            self.map.insert(stripped_key, coerced_value);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(JsonCoercionErrors(errors).into())
        }
    }
}

#[derive(Debug)]
struct JsonCoercionErrors(Vec<(Arc<ValueOrigin>, serde_json::Error)>);

impl fmt::Display for JsonCoercionErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "failed coercing flat configuration params to JSON:"
        )?;
        for (i, (key, err)) in self.0.iter().enumerate() {
            writeln!(formatter, "{}. {key}: {err}", i + 1)?;
        }
        Ok(())
    }
}

impl std::error::Error for JsonCoercionErrors {}

impl ConfigSource for Environment {
    type Kind = Flat;

    fn into_contents(self) -> WithOrigin<Map> {
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
        assert_eq!(env.map["app_test"].inner.as_plain_str(), Some("42"));
        let origin = &env.map["app_test"].origin;
        let ValueOrigin::Path { path, source } = origin.as_ref() else {
            panic!("unexpected origin: {origin:?}");
        };
        assert_eq!(path, "APP_TEST");
        assert_matches!(
            source.as_ref(),
            ValueOrigin::File { name, format: FileFormat::Dotenv } if name == "test.env"
        );
        assert_eq!(
            env.map["app_other"].inner.as_plain_str(),
            Some("test string")
        );

        let env = env.strip_prefix("app_");
        assert_eq!(env.map.len(), 2, "{:?}", env.map);
        assert_eq!(env.map["test"].inner.as_plain_str(), Some("42"));
        assert_matches!(env.map["test"].origin.as_ref(), ValueOrigin::Path { path, .. } if path == "APP_TEST");
        assert_eq!(env.map["other"].inner.as_plain_str(), Some("test string"));
    }
}
