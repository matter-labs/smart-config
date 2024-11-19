//! Enriched JSON object model that allows to associate values with origins.

use std::{collections::HashMap, fmt, iter, sync::Arc};

#[derive(Debug, Default)]
pub(crate) enum ValueOrigin {
    #[default]
    Unknown,
    SyntheticObject,
    EnvVar(String),
    Json {
        filename: Arc<str>,
        path: String,
    },
}

impl fmt::Display for ValueOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => formatter.write_str("unknown"),
            Self::SyntheticObject => formatter
                .write_str("synthetic object (configuration mounting point or its ancestor)"),
            Self::EnvVar(name) => write!(formatter, "env variable '{name}'"),
            Self::Json { filename, path } => {
                write!(formatter, "variable at {path} in JSON file '{filename}'")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<ValueWithOrigin>),
    Object(Map),
}

pub(crate) type Map = HashMap<String, ValueWithOrigin>;

#[derive(Debug, Clone)]
pub(crate) struct ValueWithOrigin {
    pub inner: Value,
    pub origin: Arc<ValueOrigin>,
}

impl PartialEq for ValueWithOrigin {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl ValueWithOrigin {
    pub fn empty() -> Self {
        Self {
            inner: Value::Null,
            origin: Arc::default(),
        }
    }

    pub fn get(&self, pointer: Pointer) -> Option<&Self> {
        pointer
            .segments()
            .try_fold(self, |ptr, segment| match &ptr.inner {
                Value::Object(map) => map.get(segment),
                Value::Array(array) => array.get(segment.parse::<usize>().ok()?),
                _ => None,
            })
    }

    pub fn get_mut(&mut self, pointer: Pointer) -> Option<&mut Self> {
        pointer
            .segments()
            .try_fold(self, |ptr, segment| match &mut ptr.inner {
                Value::Object(map) => map.get_mut(segment),
                Value::Array(array) => array.get_mut(segment.parse::<usize>().ok()?),
                _ => None,
            })
    }

    /// Only objects are meaningfully merged; all other values are replaced.
    pub fn merge(&mut self, other: Self) {
        match (&mut self.inner, other.inner) {
            (Value::Object(this), Value::Object(other)) => {
                Self::merge_into_map(this, other);
            }
            (this, value) => {
                *this = value;
                self.origin = other.origin;
            }
        }
    }

    pub fn merge_into_map(dest: &mut Map, source: Map) {
        for (key, value) in source {
            if let Some(existing_value) = dest.get_mut(&key) {
                existing_value.merge(value);
            } else {
                dest.insert(key, value);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Pointer<'a>(pub &'a str);

impl<'a> Pointer<'a> {
    pub fn segments(self) -> impl Iterator<Item = &'a str> {
        self.0
            .split('.')
            .take(if self.0.is_empty() { 0 } else { usize::MAX })
    }

    pub fn split_last(self) -> Option<(Self, &'a str)> {
        if self.0.is_empty() {
            None
        } else if let Some((parent, last_segment)) = self.0.rsplit_once('.') {
            Some((Self(parent), last_segment))
        } else {
            Some((Self(""), self.0))
        }
    }

    /// Includes `Self`; doesn't include the empty pointer.
    pub fn with_ancestors(self) -> impl Iterator<Item = Self> {
        let mut current = self.0;
        iter::from_fn(move || {
            if current.is_empty() {
                None
            } else if let Some((_, tail)) = current.split_once('.') {
                current = tail;
                Some(Self(&self.0[..self.0.len() - tail.len() - 1]))
            } else {
                current = "";
                Some(self)
            }
        })
    }

    pub fn join(self, suffix: &str) -> String {
        if suffix.is_empty() {
            self.0.to_owned()
        } else if self.0.is_empty() {
            suffix.to_owned()
        } else {
            format!("{}.{suffix}", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitting_pointer() {
        let pointer = Pointer("");
        assert_eq!(pointer.split_last(), None);
        assert_eq!(pointer.segments().collect::<Vec<_>>(), [] as [&str; 0]);
        assert_eq!(pointer.with_ancestors().collect::<Vec<_>>(), []);

        let pointer = Pointer("test");
        assert_eq!(pointer.split_last(), Some((Pointer(""), "test")));
        assert_eq!(pointer.segments().collect::<Vec<_>>(), ["test"]);
        assert_eq!(
            pointer.with_ancestors().collect::<Vec<_>>(),
            [Pointer("test")]
        );

        let pointer = Pointer("test.value");
        assert_eq!(pointer.split_last(), Some((Pointer("test"), "value")));
        assert_eq!(pointer.segments().collect::<Vec<_>>(), ["test", "value"]);
        assert_eq!(
            pointer.with_ancestors().collect::<Vec<_>>(),
            [Pointer("test"), Pointer("test.value")]
        );
    }

    #[test]
    fn joining_pointers() {
        let pointer = Pointer("");
        let joined = pointer.join("test");
        assert_eq!(joined, "test");

        let pointer = Pointer("test");
        let joined = pointer.join("");
        assert_eq!(joined, "test");

        let pointer = Pointer("test");
        let joined = pointer.join("other");
        assert_eq!(joined, "test.other");
    }
}
