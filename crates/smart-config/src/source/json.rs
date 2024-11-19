use std::sync::Arc;

use crate::value::{Map, Pointer, Value, ValueOrigin, ValueWithOrigin};

/// JSON-based configuration source.
#[derive(Debug)]
pub struct Json {
    pub(super) inner: Map,
}

impl Json {
    pub fn new(object: serde_json::Map<String, serde_json::Value>, filename: &str) -> Self {
        let filename: Arc<str> = filename.into();
        let inner =
            Self::map_value(serde_json::Value::Object(object), &filename, String::new()).inner;
        let Value::Object(inner) = inner else {
            unreachable!();
        };
        Self { inner }
    }

    fn map_value(value: serde_json::Value, filename: &Arc<str>, path: String) -> ValueWithOrigin {
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

        ValueWithOrigin {
            inner,
            origin: Arc::new(ValueOrigin::Json {
                filename: filename.clone(),
                path,
            }),
        }
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
        let json = Json::new(json, "test.json");

        assert_eq!(json.inner["bool_value"].inner, Value::Bool(true));
        assert_matches!(
            json.inner["bool_value"].origin.as_ref(),
            ValueOrigin::Json { filename, path } if filename.as_ref() == "test.json" && path == "bool_value"
        );

        let str = json.inner["nested"].get(Pointer("str")).unwrap();
        assert_eq!(str.inner, Value::String("???".into()));
        assert_matches!(
            str.origin.as_ref(),
            ValueOrigin::Json { filename, path } if filename.as_ref() == "test.json" && path == "nested.str"
        );
    }
}
