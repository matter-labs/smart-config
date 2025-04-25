//! Test-only functionality shared among multiple test modules.

use std::{
    any::Any,
    collections::{HashMap, HashSet},
    fmt,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    num::NonZeroUsize,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use anyhow::Context as _;
use assert_matches::assert_matches;
use secrecy::{ExposeSecret, SecretString};
use serde::{de::Error as DeError, Deserialize, Serialize};

use crate::{
    de::{self, DeserializeContext, DeserializeParam, DeserializerOptions, Serde, WellKnown},
    fallback,
    fallback::FallbackSource,
    metadata::{BasicTypes, ConfigMetadata, ParamMetadata, SizeUnit, TimeUnit},
    testing,
    value::{FileFormat, Value, ValueOrigin, WithOrigin},
    visit::ConfigVisitor,
    ByteSize, ConfigSource, DescribeConfig, DeserializeConfig, Environment, ErrorWithOrigin, Json,
    ParseErrors,
};

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SimpleEnum {
    #[serde(alias = "first_choice")]
    First,
    Second,
}

impl WellKnown for SimpleEnum {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct ValueCoercingConfig {
    pub param: TestParam,
    #[config(default)]
    pub set: HashSet<u64>,
    #[config(default)]
    pub repeated: Vec<TestParam>,
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct NestedConfig {
    #[config(rename = "renamed", alias = "enum")]
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
    #[config(nest, alias = "nest")]
    pub nested: NestedConfig,
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate, tag = "type")]
pub(crate) enum EnumConfig {
    /// Empty variant.
    #[config(rename = "first")]
    First,
    /// Variant wrapping a flattened config.
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
    #[config(nest)]
    pub nested_opt: Option<NestedConfig>,
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
#[config(crate = crate)]
pub(crate) struct KvTestConfig {
    #[config(default_t = -3)]
    pub nested_int: i32,
    #[config(nest)]
    pub nested: DefaultingConfig,
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

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug)]
struct StringLen;

impl DeserializeParam<usize> for StringLen {
    const EXPECTING: BasicTypes = BasicTypes::STRING;

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<usize, ErrorWithOrigin> {
        let de = ctx.current_value_deserializer(param.name)?;
        let len = String::deserialize(de)?.len();
        if len > 5 {
            return Err(DeError::custom("string is too long"));
        }
        Ok(len)
    }

    fn serialize_param(&self, &param: &usize) -> serde_json::Value {
        "_".repeat(param).into()
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
    #[config(default_t = Duration::from_secs(5), alias = "long_timeout")]
    pub long_dur: Duration,
    #[config(default_t = "./test".into())]
    pub path: PathBuf,
    #[config(with = de::Optional(SizeUnit::MiB))]
    #[config(default_t = Some(ByteSize::new(128, SizeUnit::MiB)))]
    pub memory_size_mb: Option<ByteSize>,
    pub disk_size: Option<ByteSize>,
    #[config(default, with = de::Delimited(":"))]
    pub paths: Vec<PathBuf>,
    #[config(default, with = de::OrString(Serde![object]))]
    pub map_or_string: MapOrString,
    #[config(default_t = Ipv4Addr::LOCALHOST.into())]
    pub ip_addr: IpAddr,
    #[config(default_t = ([192, 168, 0, 1], 3000).into())]
    pub socket_addr: SocketAddr,
    #[config(default, with = StringLen)]
    pub with_custom_deserializer: usize,
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct ComposedConfig {
    #[config(default)]
    pub arrays: HashSet<[u64; 2]>,
    #[config(default)]
    pub durations: Vec<Duration>,
    #[config(default, with = de::Delimited(","))]
    pub delimited_durations: Vec<Duration>,
    #[config(default)]
    pub map_of_sizes: HashMap<String, ByteSize>,
    #[config(default)]
    pub map_of_ints: HashMap<u64, Duration>,
    #[config(default, with = de::Entries::WELL_KNOWN.named("val", "timeout"))]
    pub entry_map: HashMap<u64, Duration>,
    #[config(default, with = de::Entries::WELL_KNOWN.named("method", "priority"))]
    pub entry_slice: Box<[(String, i32)]>,
}

#[derive(Debug, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct SecretConfig {
    pub key: SecretString,
    pub opt: Option<SecretString>,
    #[config(secret)]
    pub path: Option<PathBuf>,
    /// We need to override the default deserializer to be able to read from string.
    #[config(default, secret, with = de::OrString(()))]
    pub int: u64,
    #[config(default_t = vec![1], secret, with = de::Delimited(","))]
    pub seq: Vec<u64>,
}

#[derive(DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct NestedAliasedConfig {
    #[config(default, alias = "string")]
    pub str: String,
}

#[derive(DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct AliasedConfig {
    pub int: u32,
    #[config(nest, alias = "nest")]
    pub nested: NestedAliasedConfig,
    #[config(flatten)]
    pub flat: NestedAliasedConfig,
}

const STR_SOURCE: &'static dyn FallbackSource =
    &fallback::Manual::new("filtered 'SMART_CONFIG_STR' env var", || {
        fallback::Env("SMART_CONFIG_STR")
            .provide_value()
            .filter(|val| val.inner.as_plain_str() != Some("unset"))
    });

#[derive(DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct ConfigWithFallbacks {
    #[config(default_t = 42, fallback = &fallback::Env("SMART_CONFIG_INT"))]
    pub int: u32,
    #[config(fallback = STR_SOURCE)]
    pub str: Option<SecretString>,
}

#[derive(Debug, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
#[config(validate("`len` must match `secret` length", Self::validate_len))]
pub(crate) struct ConfigWithValidations {
    pub len: usize,
    pub secret: SecretString,
}

impl ConfigWithValidations {
    fn validate_len(&self) -> Result<(), ErrorWithOrigin> {
        if self.len != self.secret.expose_secret().len() {
            return Err(DeError::custom("`len` doesn't correspond to `secret`"));
        }
        Ok(())
    }
}

#[derive(Debug, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct ConfigWithNestedValidations {
    #[config(nest)]
    pub nested: ConfigWithValidations,
}

pub(crate) fn wrap_into_value(env: Environment) -> WithOrigin {
    let map = env.into_contents().inner;
    let map = map.into_iter().map(|(key, value)| {
        (
            key,
            WithOrigin {
                inner: value.inner.into(),
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
    C::deserialize_config(ctx).map_err(|_| errors)
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
    C::deserialize_config(ctx).map_err(|_| errors)
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

// FIXME: Doesn't work for nested / flattened configs!
pub(crate) fn serialize_to_json<C: DeserializeConfig>(
    config: &C,
) -> serde_json::Map<String, serde_json::Value> {
    struct SerializingVisitor {
        metadata: &'static ConfigMetadata,
        json: serde_json::Map<String, serde_json::Value>,
    }

    impl ConfigVisitor for SerializingVisitor {
        fn visit_tag(&mut self, variant_index: usize) {
            let tag = self.metadata.tag.unwrap();
            let tag_name = tag.param.name;
            let tag_value = tag.variants[variant_index].name;
            self.json.insert(tag_name.to_owned(), tag_value.into());
        }

        fn visit_param(&mut self, param_index: usize, value: &dyn Any) {
            let param = &self.metadata.params[param_index];
            let value = param.deserializer.serialize_param(value);
            self.json.insert(param.name.to_owned(), value);
        }
    }

    let mut visitor = SerializingVisitor {
        metadata: &C::DESCRIPTION,
        json: serde_json::Map::default(),
    };
    config.visit_config(&mut visitor);
    visitor.json
}

pub(crate) fn test_config_roundtrip<C>(config: &C) -> serde_json::Map<String, serde_json::Value>
where
    C: DeserializeConfig + PartialEq + fmt::Debug,
{
    let json = serialize_to_json(config);
    let config_copy: C = testing::test(Json::new("test.json", json.clone())).unwrap();
    assert_eq!(config_copy, *config);
    json
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
