use std::{collections::HashMap, env, mem, sync::Arc};

use crate::value::{Pointer, Value, ValueOrigin, ValueWithOrigin};

/// A key-value configuration source (e.g., environment variables).
#[derive(Debug, Clone, Default)]
pub struct Environment {
    map: HashMap<String, ValueWithOrigin>,
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
                ValueWithOrigin {
                    inner: Value::String(value.into()),
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
                ValueWithOrigin {
                    inner: Value::String(value),
                    origin: Arc::new(ValueOrigin::EnvVar(name.to_owned())),
                },
            ))
        });
        self.map.extend(defined_vars);
        self
    }

    /// Converts a logical prefix like `api.limits` to `API_LIMITS_`.
    fn env_prefix(prefix: &str) -> String {
        let mut prefix = prefix.replace('.', "_");
        if !prefix.is_empty() && !prefix.ends_with('_') {
            prefix.push('_');
        }
        prefix
    }

    pub(super) fn take_matching_entries(
        &mut self,
        prefix: Pointer,
    ) -> Vec<(String, ValueWithOrigin)> {
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

    pub(super) fn extend(&mut self, other: Self) {
        self.map.extend(other.map);
    }

    #[cfg(test)]
    pub(super) fn into_map(self) -> HashMap<String, ValueWithOrigin> {
        self.map
    }
}
