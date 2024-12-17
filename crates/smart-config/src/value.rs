//! Enriched JSON object model that allows to associate values with origins.

use std::{collections::HashMap, fmt, iter, mem, sync::Arc};

use secrecy::ExposeSecret;
pub use secrecy::SecretString;

use crate::metadata::BasicTypes;

/// Supported file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FileFormat {
    /// JSON file.
    Json,
    /// YAML file.
    Yaml,
    /// `.env` file.
    Dotenv,
}

impl fmt::Display for FileFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Json => "JSON",
            Self::Yaml => "YAML",
            Self::Dotenv => ".env",
        })
    }
}

/// Origin of a [`Value`] in configuration input.
#[derive(Debug, Default)]
#[non_exhaustive]
pub enum ValueOrigin {
    /// Unknown / default origin.
    #[default]
    Unknown,
    /// Environment variables.
    EnvVars,
    /// Alternative values for config params.
    Alternatives,
    /// File source.
    File {
        /// Filename; may not correspond to a real filesystem path.
        name: String,
        /// File format.
        format: FileFormat,
    },
    /// Path from a structured source.
    Path {
        /// Source of structured data, e.g. a JSON file.
        source: Arc<Self>,
        /// Dot-separated path in the source, like `api.http.port`.
        path: String,
    },
    /// Synthetic value.
    Synthetic {
        /// Original value source.
        source: Arc<Self>,
        /// Human-readable description of the transform.
        transform: String,
    },
}

impl fmt::Display for ValueOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => formatter.write_str("unknown"),
            Self::EnvVars => formatter.write_str("env variables"),
            Self::Alternatives => formatter.write_str("alternatives"),
            Self::File { name, format } => {
                write!(formatter, "{format} file '{name}'")
            }
            Self::Path { source, path } => {
                if matches!(source.as_ref(), ValueOrigin::EnvVars) {
                    write!(formatter, "env variable '{path}'")
                } else {
                    write!(formatter, "{source} -> path '{path}'")
                }
            }
            Self::Synthetic { source, transform } => {
                write!(formatter, "{source} -> {transform}")
            }
        }
    }
}

/// String value: either a plaintext one, or a secret.
#[derive(Clone)]
pub enum StrValue {
    /// Plain string value.
    Plain(String),
    /// Secret string value.
    Secret(SecretString),
}

impl StrValue {
    /// Exposes a secret string if appropriate.
    pub fn expose(&self) -> &str {
        match self {
            Self::Plain(s) => s,
            Self::Secret(s) => s.expose_secret(),
        }
    }

    pub(crate) fn is_secret(&self) -> bool {
        matches!(self, Self::Secret(_))
    }

    pub(crate) fn make_secret(&mut self) {
        match self {
            Self::Plain(s) => {
                *self = Self::Secret(mem::take(s).into());
            }
            Self::Secret(_) => { /* value is already secret; do nothing */ }
        }
    }
}

impl fmt::Debug for StrValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plain(s) => fmt::Debug::fmt(s, formatter),
            Self::Secret(_) => formatter.write_str("[REDACTED]"),
        }
    }
}

impl fmt::Display for StrValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Plain(s) => s,
            Self::Secret(_) => "[REDACTED]",
        })
    }
}

/// JSON value with additional origin information.
#[derive(Debug, Clone, Default)]
pub enum Value {
    /// `null`.
    #[default]
    Null,
    /// Boolean value.
    Bool(bool),
    /// Numeric value.
    Number(serde_json::Number),
    /// String value.
    String(StrValue),
    /// Array of values.
    Array(Vec<WithOrigin>),
    /// Object / map of values.
    Object(Map),
}

// TODO: add more conversions

impl Value {
    pub(crate) fn is_supported_by(&self, types: BasicTypes) -> bool {
        match self {
            Self::Null => true,
            Self::Bool(_) => types.contains(BasicTypes::BOOL),
            Self::Number(number) if number.is_u64() || number.is_i64() => {
                types.contains(BasicTypes::INTEGER)
            }
            Self::Number(_) => types.contains(BasicTypes::FLOAT),
            Self::String(_) => {
                // Relax type consistency check in order to be able to deserialize numbers / bools
                // (which is supported on the `ValueDeserializer` level).
                types.contains(BasicTypes::STRING)
                    || types.contains(BasicTypes::INTEGER)
                    || types.contains(BasicTypes::BOOL)
            }
            Self::Array(_) => types.contains(BasicTypes::ARRAY),
            Self::Object(_) => types.contains(BasicTypes::OBJECT),
        }
    }

    /// Attempts to convert this value to an object.
    pub fn as_object(&self) -> Option<&Map> {
        match self {
            Self::Object(map) => Some(map),
            _ => None,
        }
    }
}

/// JSON object.
pub type Map<V = Value> = HashMap<String, WithOrigin<V>>;

/// JSON value together with its origin.
#[derive(Debug, Clone, Default)]
pub struct WithOrigin<T = Value> {
    /// Inner value.
    pub inner: T,
    /// Origin of the value.
    pub origin: Arc<ValueOrigin>,
}

impl<T> WithOrigin<T> {
    pub(crate) fn new(inner: T, origin: Arc<ValueOrigin>) -> Self {
        Self { inner, origin }
    }

    pub(crate) fn set_origin_if_unset(mut self, origin: &Arc<ValueOrigin>) -> Self {
        if matches!(self.origin.as_ref(), ValueOrigin::Unknown) {
            self.origin = origin.clone();
        }
        self
    }
}

impl WithOrigin {
    pub(crate) fn get(&self, pointer: Pointer<'_>) -> Option<&Self> {
        pointer
            .segments()
            .try_fold(self, |ptr, segment| match &ptr.inner {
                Value::Object(map) => map.get(segment),
                Value::Array(array) => array.get(segment.parse::<usize>().ok()?),
                _ => None,
            })
    }

    /// Returns value at the specified pointer.
    pub fn pointer(&self, pointer: &str) -> Option<&Self> {
        self.get(Pointer(pointer))
    }

    pub(crate) fn get_mut(&mut self, pointer: Pointer) -> Option<&mut Self> {
        pointer
            .segments()
            .try_fold(self, |ptr, segment| match &mut ptr.inner {
                Value::Object(map) => map.get_mut(segment),
                Value::Array(array) => array.get_mut(segment.parse::<usize>().ok()?),
                _ => None,
            })
    }

    /// Ensures that there is an object (possibly empty) at the specified location.
    pub(crate) fn ensure_object(
        &mut self,
        at: Pointer<'_>,
        mut create_origin: impl FnMut(Pointer<'_>) -> Arc<ValueOrigin>,
    ) -> &mut Map {
        for ancestor_path in at.with_ancestors() {
            self.ensure_object_step(ancestor_path, &mut create_origin);
        }

        let Value::Object(map) = &mut self.get_mut(at).unwrap().inner else {
            unreachable!(); // Ensured by calls above
        };
        map
    }

    fn ensure_object_step(
        &mut self,
        at: Pointer<'_>,
        mut create_origin: impl FnMut(Pointer<'_>) -> Arc<ValueOrigin>,
    ) {
        let Some((parent, last_segment)) = at.split_last() else {
            // Nothing to do.
            return;
        };

        // `unwrap()` is safe since `ensure_object()` is always called for the parent
        let parent = &mut self.get_mut(parent).unwrap().inner;
        if !matches!(parent, Value::Object(_)) {
            *parent = Value::Object(Map::new());
        }
        let Value::Object(parent_object) = parent else {
            unreachable!();
        };

        if !parent_object.contains_key(last_segment) {
            parent_object.insert(
                last_segment.to_owned(),
                WithOrigin {
                    inner: Value::Object(Map::new()),
                    origin: create_origin(at),
                },
            );
        }
    }

    /// Deep-merges self and `other`, with `other` having higher priority. Only objects are meaningfully merged;
    /// all other values are replaced.
    pub(crate) fn deep_merge(&mut self, overrides: Self) {
        match (&mut self.inner, overrides.inner) {
            (Value::Object(this), Value::Object(other)) => {
                Self::deep_merge_into_map(this, other);
            }
            (this, value) => {
                *this = value;
                self.origin = overrides.origin;
            }
        }
    }

    fn deep_merge_into_map(dest: &mut Map, source: Map) {
        for (key, value) in source {
            if let Some(existing_value) = dest.get_mut(&key) {
                existing_value.deep_merge(value);
            } else {
                dest.insert(key, value);
            }
        }
    }
}

// TODO: make public for increased type safety?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Pointer<'a>(pub &'a str);

impl fmt::Display for Pointer<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl<'a> Pointer<'a> {
    pub(crate) fn segments(self) -> impl Iterator<Item = &'a str> {
        self.0
            .split('.')
            .take(if self.0.is_empty() { 0 } else { usize::MAX })
    }

    pub(crate) fn split_last(self) -> Option<(Self, &'a str)> {
        if self.0.is_empty() {
            None
        } else if let Some((parent, last_segment)) = self.0.rsplit_once('.') {
            Some((Self(parent), last_segment))
        } else {
            Some((Self(""), self.0))
        }
    }

    /// Includes `Self`; doesn't include the empty pointer.
    pub(crate) fn with_ancestors(self) -> impl Iterator<Item = Self> {
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

    pub(crate) fn join(self, suffix: &str) -> String {
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
