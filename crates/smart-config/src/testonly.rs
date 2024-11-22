//! Test-only functionality shared among multiple test modules.

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use serde::Deserialize;

use crate::{
    de::{self, DeserializeContext},
    metadata::{BasicType, SchemaType, TimeUnit},
    source::ConfigContents,
    value::{Value, WithOrigin},
    ConfigSource, DescribeConfig, DeserializeConfig, Environment, ParseErrors,
};

#[derive(Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SimpleEnum {
    First,
    Second,
}

impl de::WellKnown for SimpleEnum {
    const TYPE: SchemaType = SchemaType::new(BasicType::String);
}

// FIXME: test embedding into config
#[derive(Debug, Deserialize)]
pub(crate) struct TestParam {
    pub int: u64,
    pub bool: bool,
    pub string: String,
    pub optional: Option<i64>,
    pub array: Vec<u32>,
    pub repeated: HashSet<SimpleEnum>,
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct NestedConfig {
    #[config(rename = "renamed")]
    pub simple_enum: SimpleEnum,
    #[config(default_t = 42)]
    pub other_int: u32,
    #[config(default)]
    pub map: HashMap<String, u32>,
}

impl NestedConfig {
    pub fn default_nested() -> Self {
        Self {
            simple_enum: SimpleEnum::First,
            other_int: 23,
            map: HashMap::new(),
        }
    }
}

#[derive(Debug, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct ConfigWithNesting {
    pub value: u32,
    #[config(default)]
    pub not_merged: String,
    #[config(nest)]
    pub nested: NestedConfig,
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate, tag = "type")]
pub(crate) enum EnumConfig {
    #[config(rename = "first")]
    First,
    Nested(NestedConfig),
    #[config(alias = "Fields", alias = "With")]
    WithFields {
        #[config(default)]
        string: Option<String>,
        #[config(default_t = true)]
        flag: bool,
        #[config(default_t = HashSet::from([23, 42]))]
        set: HashSet<u32>,
    },
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct CompoundConfig {
    #[config(nest)]
    pub nested: NestedConfig,
    #[config(rename = "default", nest, default = NestedConfig::default_nested)]
    pub nested_default: NestedConfig,
    #[config(flatten)]
    pub flat: NestedConfig,
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate, derive(Default))]
pub(crate) struct DefaultingConfig {
    #[config(default_t = 12)]
    pub int: u32,
    #[config(default_t = Some("https://example.com/".into()))]
    pub url: Option<String>,
    #[config(default)]
    pub set: HashSet<SimpleEnum>,
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate, tag = "kind", derive(Default))]
pub(crate) enum DefaultingEnumConfig {
    First,
    #[config(default)]
    Second {
        #[config(default_t = 123)]
        int: u32,
    },
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct ConfigWithComplexTypes {
    #[config(default_t = 4.2)]
    pub float: f32,
    pub array: [NonZeroUsize; 2],
    #[config(with = de::Assume(BasicType::Float))]
    pub assumed: Option<serde_json::Value>,
    #[config(default_t = Duration::from_millis(100), with = TimeUnit::Millis)]
    pub short_dur: Duration,
    #[config(default_t = "./test".into())]
    pub path: PathBuf,
}

pub(crate) fn wrap_into_value(env: Environment) -> WithOrigin {
    let ConfigContents::KeyValue(map) = env.into_contents() else {
        unreachable!();
    };
    let map = map.into_iter().map(|(key, value)| {
        (
            key,
            WithOrigin {
                inner: Value::String(value.inner),
                origin: value.origin,
            },
        )
    });

    WithOrigin {
        inner: Value::Object(map.collect()),
        origin: Arc::default(),
    }
}

pub(crate) fn test_deserialize<C: DeserializeConfig>(val: &WithOrigin) -> Result<C, ParseErrors> {
    let mut errors = ParseErrors::default();
    let ctx = DeserializeContext::new(val, "".into(), C::describe_config(), &mut errors);
    match C::deserialize_config(ctx) {
        Some(config) => Ok(config),
        None => Err(errors),
    }
}

pub(crate) fn test_deserialize_missing<C: DeserializeConfig>() -> Result<C, ParseErrors> {
    let mut errors = ParseErrors::default();
    let val = WithOrigin::new(Value::Null, Arc::default());
    let ctx = DeserializeContext::new(&val, "test".into(), C::describe_config(), &mut errors);
    match C::deserialize_config(ctx) {
        Some(config) => Ok(config),
        None => Err(errors),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_derived_as_expected() {
        let config = DefaultingConfig::default();
        assert_eq!(config.int, 12);
        assert_eq!(config.url.unwrap(), "https://example.com/");
        assert!(config.set.is_empty());

        let config = DefaultingEnumConfig::default();
        assert_eq!(config, DefaultingEnumConfig::Second { int: 123 });
    }
}
