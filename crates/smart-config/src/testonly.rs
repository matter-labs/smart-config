//! Test-only functionality shared among multiple test modules.

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use anyhow::Context as _;
use assert_matches::assert_matches;
use serde::Deserialize;

use crate::{
    de::{self, DeserializeContext, DeserializerOptions, Serde, WellKnown},
    metadata::{SizeUnit, TimeUnit},
    source::ConfigContents,
    value::{FileFormat, Value, ValueOrigin, WithOrigin},
    ByteSize, ConfigSource, DescribeConfig, DeserializeConfig, Environment, ParseErrors,
};

#[derive(Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SimpleEnum {
    First,
    Second,
}

impl WellKnown for SimpleEnum {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

#[derive(Debug, Deserialize)]
pub(crate) struct TestParam {
    pub int: u64,
    #[serde(default)]
    pub bool: bool,
    pub string: String,
    pub optional: Option<i64>,
    #[serde(default)]
    pub array: Vec<u32>,
    #[serde(default)]
    pub repeated: HashSet<SimpleEnum>,
}

impl WellKnown for TestParam {
    type Deserializer = Serde![object];
    const DE: Self::Deserializer = Serde![object];
}

#[derive(Debug, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct ValueCoercingConfig {
    pub param: TestParam,
    #[config(default)]
    pub set: HashSet<u64>,
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
    pub(crate) fn default_nested() -> Self {
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
    #[config(default, alias = "alias")]
    pub merged: String,
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
#[config(crate = crate, tag = "version", rename_all = "snake_case")]
pub(crate) enum RenamedEnumConfig {
    V0,
    #[config(alias = "previous")]
    V1 {
        int: u64,
    },
    #[config(default)]
    V2 {
        str: String,
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
    pub float: Option<f64>,
    #[config(default_t = Some("https://example.com/".into()))]
    pub url: Option<String>,
    #[config(default, with = de::Delimited(","))]
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

#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(transparent)]
pub(crate) struct MapOrString(pub HashMap<String, u64>);

impl WellKnown for MapOrString {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

impl FromStr for MapOrString {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let entries = s.split(',').map(|entry| {
            let (key, value) = entry.split_once('=').context("incorrect entry")?;
            let value: u64 = value.parse().context("invalid value")?;
            anyhow::Ok((key.to_owned(), value))
        });
        entries.collect::<anyhow::Result<_>>().map(Self)
    }
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct ConfigWithComplexTypes {
    #[config(default_t = 4.2)]
    pub float: f32,
    #[config(with = de::Delimited(","))]
    pub array: [NonZeroUsize; 2],
    pub choices: Option<Vec<SimpleEnum>>,
    #[config(with = de::Optional(Serde![float]))]
    pub assumed: Option<serde_json::Value>,
    #[config(default_t = Duration::from_millis(100), with = TimeUnit::Millis)]
    pub short_dur: Duration,
    #[config(default_t = "./test".into())]
    pub path: PathBuf,
    #[config(with = de::Optional(SizeUnit::MiB))]
    #[config(default_t = Some(ByteSize::new(128, SizeUnit::MiB)))]
    pub memory_size_mb: Option<ByteSize>,
    #[config(default, with = de::Delimited(":"))]
    pub paths: Vec<PathBuf>,
    #[config(default, with = de::OrString(Serde![object]))]
    pub map_or_string: MapOrString,
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
    let de_options = DeserializerOptions::default();
    let ctx = DeserializeContext::new(
        &de_options,
        val,
        String::new(),
        &C::DESCRIPTION,
        &mut errors,
    );
    match C::deserialize_config(ctx) {
        Some(config) => Ok(config),
        None => Err(errors),
    }
}

pub(crate) fn test_deserialize_missing<C: DeserializeConfig>() -> Result<C, ParseErrors> {
    let mut errors = ParseErrors::default();
    let de_options = DeserializerOptions::default();
    let val = WithOrigin::new(Value::Null, Arc::default());
    let ctx = DeserializeContext::new(
        &de_options,
        &val,
        "test".into(),
        &C::DESCRIPTION,
        &mut errors,
    );
    match C::deserialize_config(ctx) {
        Some(config) => Ok(config),
        None => Err(errors),
    }
}

pub(crate) fn extract_json_name(source: &ValueOrigin) -> &str {
    if let ValueOrigin::File {
        name,
        format: FileFormat::Json,
    } = source
    {
        name
    } else {
        panic!("unexpected source, expected JSON file: {source:?}");
    }
}

pub(crate) fn extract_env_var_name(source: &ValueOrigin) -> &str {
    let ValueOrigin::Path { path, source } = source else {
        panic!("unexpected source: {source:?}");
    };
    assert_matches!(source.as_ref(), ValueOrigin::EnvVars);
    path
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
