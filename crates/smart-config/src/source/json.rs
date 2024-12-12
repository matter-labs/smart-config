use std::sync::Arc;

use super::{ConfigContents, ConfigSource};
use crate::value::{FileFormat, Map, Pointer, StrValue, Value, ValueOrigin, WithOrigin};

/// JSON-based configuration source.
#[derive(Debug)]
pub struct Json {
    origin: Arc<ValueOrigin>,
    inner: WithOrigin,
}

impl Json {
    /// Creates an empty JSON source with the specified name.
    pub fn empty(filename: &str) -> Self {
        Self::new(filename, serde_json::Map::default())
    }

    /// Creates a source with the specified name and contents.
    pub fn new(filename: &str, object: serde_json::Map<String, serde_json::Value>) -> Self {
        let origin = Arc::new(ValueOrigin::File {
            name: filename.to_owned(),
            format: FileFormat::Json,
        });
        let inner = Self::map_value(serde_json::Value::Object(object), &origin, String::new());
        Self { origin, inner }
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

        let value = Self::map_value(value, &self.origin, at.to_owned());

        let merge_point = if let Some((parent, last_segment)) = Pointer(at).split_last() {
            self.inner.ensure_object(parent, |path| {
                Arc::new(ValueOrigin::Path {
                    source: self.origin.clone(),
                    path: path.0.to_owned(),
                })
            });

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

        merge_point.deep_merge(value);
        debug_assert!(matches!(&self.inner.inner, Value::Object(_)));
    }

    pub(crate) fn map_value(
        value: serde_json::Value,
        file_origin: &Arc<ValueOrigin>,
        path: String,
    ) -> WithOrigin {
        let inner = match value {
            serde_json::Value::Bool(value) => Value::Bool(value),
            serde_json::Value::Number(value) => Value::Number(value),
            serde_json::Value::String(value) => Value::String(StrValue::Plain(value)),
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Array(values) => Value::Array(
                values
                    .into_iter()
                    .enumerate()
                    .map(|(i, value)| {
                        let child_path = Pointer(&path).join(&i.to_string());
                        Self::map_value(value, file_origin, child_path)
                    })
                    .collect(),
            ),
            serde_json::Value::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| {
                        let value = Self::map_value(value, file_origin, Pointer(&path).join(&key));
                        (key, value)
                    })
                    .collect(),
            ),
        };

        WithOrigin {
            inner,
            origin: Arc::new(ValueOrigin::Path {
                source: file_origin.clone(),
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
    fn origin(&self) -> Arc<ValueOrigin> {
        self.origin.clone()
    }

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
    use crate::testonly::extract_json_name;

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
        assert_matches!(bool_value.inner, Value::Bool(true));
        assert_matches!(
            bool_value.origin.as_ref(),
            ValueOrigin::Path { path, source } if path == "bool_value" && extract_json_name(source) == "test.json"
        );

        let str = json.inner.get(Pointer("nested.str")).unwrap();
        assert_matches!(&str.inner, Value::String(StrValue::Plain(s)) if s == "???");
        assert_matches!(
            str.origin.as_ref(),
            ValueOrigin::Path { path, source } if path == "nested.str" && extract_json_name(source) == "test.json"
        );

        json.merge("nested.str", "!!!");
        let str = json.inner.get(Pointer("nested.str")).unwrap();
        assert_matches!(&str.inner, Value::String(StrValue::Plain(s)) if s == "!!!");

        json.merge(
            "nested",
            serde_json::json!({
                "int_value": 5,
                "array": [1, 2],
            }),
        );
        let str = json.inner.get(Pointer("nested.str")).unwrap();
        assert_matches!(&str.inner, Value::String(StrValue::Plain(s)) if s == "!!!");
        let int_value = json.inner.get(Pointer("nested.int_value")).unwrap();
        assert_matches!(&int_value.inner, Value::Number(num) if *num == 5_u64.into());
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
        assert_matches!(bool_value.inner, Value::Bool(true));
        assert_matches!(
            bool_value.origin.as_ref(),
            ValueOrigin::Path { path, source }
                if path == "bool_value" && extract_json_name(source).contains("inline config")
        );

        let str = json.inner.get(Pointer("nested.str")).unwrap();
        assert_matches!(&str.inner, Value::String(StrValue::Plain(s)) if s == "???");
        assert_matches!(
            str.origin.as_ref(),
            ValueOrigin::Path { path, .. } if path == "nested.str"
        );
    }
}
