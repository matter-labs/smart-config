use std::sync::Arc;

use anyhow::Context;

use super::{ConfigContents, ConfigSource};
use crate::value::{Map, Pointer, Value, ValueOrigin, WithOrigin};

/// YAML-based configuration source.
#[derive(Debug)]
pub struct Yaml {
    inner: Map,
}

impl Yaml {
    /// Creates a source with the specified name and contents.
    pub fn new(filename: &str, object: serde_yaml::Mapping) -> anyhow::Result<Self> {
        let filename: Arc<str> = filename.into();
        let inner =
            Self::map_value(serde_yaml::Value::Mapping(object), &filename, String::new())?.inner;
        let Value::Object(inner) = inner else {
            unreachable!();
        };
        Ok(Self { inner })
    }

    fn map_key(key: serde_yaml::Value, parent_path: &str) -> anyhow::Result<String> {
        Ok(match key {
            serde_yaml::Value::String(value) => value,
            serde_yaml::Value::Number(value) => value.to_string(),
            serde_yaml::Value::Bool(value) => value.to_string(),
            serde_yaml::Value::Null => "null".into(),
            _ => anyhow::bail!("unsupported key type at {parent_path:?}: {key:?}; only primitive value types are supported as keys"),
        })
    }

    fn map_number(number: serde_yaml::Number, path: &str) -> anyhow::Result<serde_json::Number> {
        Ok(if let Some(number) = number.as_u64() {
            number.into()
        } else if let Some(number) = number.as_i64() {
            number.into()
        } else if let Some(number) = number.as_f64() {
            serde_json::Number::from_f64(number)
                .with_context(|| format!("unsupported number at {path:?}: {number:?}"))?
        } else {
            anyhow::bail!("unsupported number at {path:?}: {number:?}")
        })
    }

    fn map_value(
        value: serde_yaml::Value,
        filename: &Arc<str>,
        path: String,
    ) -> anyhow::Result<WithOrigin> {
        let inner = match value {
            serde_yaml::Value::Null => Value::Null,
            serde_yaml::Value::Bool(value) => Value::Bool(value),
            serde_yaml::Value::Number(value) => Value::Number(Self::map_number(value, &path)?),
            serde_yaml::Value::String(value) => Value::String(value),
            serde_yaml::Value::Sequence(items) => Value::Array(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, value)| {
                        let child_path = Pointer(&path).join(&i.to_string());
                        Self::map_value(value, filename, child_path)
                    })
                    .collect::<anyhow::Result<_>>()?,
            ),
            serde_yaml::Value::Mapping(items) => Value::Object(
                items
                    .into_iter()
                    .map(|(key, value)| {
                        let key = Self::map_key(key, &path)?;
                        let child_path = Pointer(&path).join(&key);
                        anyhow::Ok((key, Self::map_value(value, filename, child_path)?))
                    })
                    .collect::<anyhow::Result<_>>()?,
            ),
            serde_yaml::Value::Tagged(tagged) => {
                return Self::map_value(tagged.value, filename, path);
            }
        };

        Ok(WithOrigin {
            inner,
            origin: Arc::new(ValueOrigin::Yaml {
                filename: filename.clone(),
                path,
            }),
        })
    }
}

impl ConfigSource for Yaml {
    fn into_contents(self) -> ConfigContents {
        ConfigContents::Hierarchical(self.inner)
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    const YAML_CONFIG: &str = r#"
bool: true
nested:
    int: 123
    string: "what?"
array:
    - test: 23
    "#;

    #[test]
    fn creating_yaml_config() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(YAML_CONFIG).unwrap();
        let serde_yaml::Value::Mapping(yaml) = yaml else {
            unreachable!();
        };
        let yaml = Yaml::new("test.yml", yaml).unwrap();

        assert_eq!(yaml.inner["bool"].inner, Value::Bool(true));
        assert_matches!(
            yaml.inner["bool"].origin.as_ref(),
            ValueOrigin::Yaml { filename, path } if filename.as_ref() == "test.yml" && path == "bool"
        );

        let str = yaml.inner["nested"].get(Pointer("string")).unwrap();
        assert_eq!(str.inner, Value::String("what?".into()));
        assert_matches!(
            str.origin.as_ref(),
            ValueOrigin::Yaml { filename, path } if filename.as_ref() == "test.yml" && path == "nested.string"
        );

        let inner_int = yaml.inner["array"].get(Pointer("0.test")).unwrap();
        assert_eq!(inner_int.inner, Value::Number(23_u64.into()));
    }

    #[test]
    fn unsupported_key() {
        let yaml = r#"
array:
    - [12, 34]: bogus
        "#;
        let yaml: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let serde_yaml::Value::Mapping(yaml) = yaml else {
            unreachable!();
        };

        let err = Yaml::new("test.yml", yaml).unwrap_err().to_string();
        assert!(err.contains("unsupported key type"), "{err}");
        assert!(err.contains("array.0"), "{err}");
    }
}
