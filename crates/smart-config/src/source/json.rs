use std::sync::Arc;

use super::{ConfigContents, ConfigSource};
use crate::value::{Map, Pointer, Value, ValueOrigin, WithOrigin};

/// JSON-based configuration source.
#[derive(Debug)]
pub struct Json {
    filename: Arc<str>,
    inner: WithOrigin,
}

impl Json {
    pub fn empty(filename: &str) -> Self {
        Self::new(filename, serde_json::Map::default())
    }

    pub fn new(filename: &str, object: serde_json::Map<String, serde_json::Value>) -> Self {
        let filename: Arc<str> = filename.into();
        let inner = Self::map_value(serde_json::Value::Object(object), &filename, String::new());
        Self { filename, inner }
    }

    /// Merges a value at the specified path into JSON.
    ///
    /// If any ancestors in `at` are not objects, they are replaced with objects.
    ///
    /// # Panics
    ///
    /// - Panics if serializing `value` to the JSON object model fails.
    /// - Panics if `at` is empty and `value` doesn't serialize to an object (which would lead to the root object
    ///   being replaced with non-object data).
    pub fn merge(&mut self, at: &str, value: impl serde::Serialize) {
        let value = serde_json::to_value(value).expect("failed serializing inserted value");
        assert!(
            !at.is_empty() || value.is_object(),
            "Cannot overwrite root object"
        );

        let value = Self::map_value(value, &self.filename, at.to_owned());

        let merge_point = if let Some((parent, last_segment)) = Pointer(at).split_last() {
            for ancestor_path in parent.with_ancestors() {
                self.inner.ensure_object(ancestor_path, || {
                    Arc::new(ValueOrigin::Json {
                        filename: self.filename.clone(),
                        path: ancestor_path.0.to_owned(),
                    })
                });
            }

            // Safe since the object has just been inserted.
            let parent = self.inner.get_mut(parent).unwrap();
            let Value::Object(parent_object) = &mut parent.inner else {
                unreachable!();
            };
            if !parent_object.contains_key(last_segment) {
                parent_object.insert(
                    last_segment.to_owned(),
                    WithOrigin {
                        inner: Value::Null,
                        origin: Arc::default(), // Doesn't matter; will be overwritten below
                    },
                );
            }
            parent_object.get_mut(last_segment).unwrap()
        } else {
            &mut self.inner
        };

        merge_point.merge(value);
        debug_assert!(matches!(&self.inner.inner, Value::Object(_)));
    }

    fn map_value(value: serde_json::Value, filename: &Arc<str>, path: String) -> WithOrigin {
        let inner = match value {
            serde_json::Value::Bool(value) => Value::Bool(value),
            serde_json::Value::Number(value) => Value::Number(value),
            serde_json::Value::String(value) => Value::String(value),
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Array(values) => Value::Array(
                values
                    .into_iter()
                    .enumerate()
                    .map(|(i, value)| {
                        let child_path = Pointer(&path).join(&i.to_string());
                        Self::map_value(value, filename, child_path)
                    })
                    .collect(),
            ),
            serde_json::Value::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| {
                        let value = Self::map_value(value, filename, Pointer(&path).join(&key));
                        (key, value)
                    })
                    .collect(),
            ),
        };

        WithOrigin {
            inner,
            origin: Arc::new(ValueOrigin::Json {
                filename: filename.clone(),
                path,
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn inner(&self) -> &WithOrigin {
        &self.inner
    }
}

impl ConfigSource for Json {
    fn into_contents(self) -> ConfigContents {
        ConfigContents::Hierarchical(match self.inner.inner {
            Value::Object(map) => map,
            _ => Map::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn creating_json_config() {
        let json = serde_json::json!({
            "bool_value": true,
            "nested": {
                "int_value": 123,
                "str": "???",
            },
        });
        let serde_json::Value::Object(json) = json else {
            unreachable!();
        };
        let mut json = Json::new("test.json", json);

        let bool_value = json.inner.get(Pointer("bool_value")).unwrap();
        assert_eq!(bool_value.inner, Value::Bool(true));
        assert_matches!(
            bool_value.origin.as_ref(),
            ValueOrigin::Json { filename, path } if filename.as_ref() == "test.json" && path == "bool_value"
        );

        let str = json.inner.get(Pointer("nested.str")).unwrap();
        assert_eq!(str.inner, Value::String("???".into()));
        assert_matches!(
            str.origin.as_ref(),
            ValueOrigin::Json { filename, path } if filename.as_ref() == "test.json" && path == "nested.str"
        );

        json.merge("nested.str", "!!!");
        let str = json.inner.get(Pointer("nested.str")).unwrap();
        assert_eq!(str.inner, Value::String("!!!".into()));

        json.merge(
            "nested",
            serde_json::json!({
                "int_value": 5,
                "array": [1, 2],
            }),
        );
        let str = json.inner.get(Pointer("nested.str")).unwrap();
        assert_eq!(str.inner, Value::String("!!!".into()));
        let int_value = json.inner.get(Pointer("nested.int_value")).unwrap();
        assert_eq!(int_value.inner, Value::Number(5_u64.into()));
        let array = json.inner.get(Pointer("nested.array")).unwrap();
        assert_matches!(&array.inner, Value::Array(items) if items.len() == 2);
    }

    #[test]
    fn creating_config_using_macro() {
        let json = config! {
            "bool_value": true,
            "nested.str": "???",
            "nested.int_value": 123,
        };

        let bool_value = json.inner.get(Pointer("bool_value")).unwrap();
        assert_eq!(bool_value.inner, Value::Bool(true));
        assert_matches!(
            bool_value.origin.as_ref(),
            ValueOrigin::Json { filename, path } if path == "bool_value" && filename.contains("inline config")
        );

        let str = json.inner.get(Pointer("nested.str")).unwrap();
        assert_eq!(str.inner, Value::String("???".into()));
        assert_matches!(
            str.origin.as_ref(),
            ValueOrigin::Json { path, .. } if path == "nested.str"
        );
    }
}
