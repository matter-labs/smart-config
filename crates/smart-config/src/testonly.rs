//! Test-only functionality shared among multiple test modules.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::{
    metadata::PrimitiveType, value::WithOrigin, DescribeConfig, DeserializeConfig,
    DeserializeContext, ParseErrors,
};

#[derive(Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SimpleEnum {
    First,
    Second,
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
pub(crate) struct NestedConfig {
    #[config(rename = "renamed")]
    #[config(kind = PrimitiveType::String.as_type())]
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

pub(crate) fn test_deserialize<C: DeserializeConfig>(val: &WithOrigin) -> Result<C, ParseErrors> {
    let mut errors = ParseErrors::default();
    let ctx = DeserializeContext::new(val, "".into(), C::describe_config(), &mut errors).unwrap();
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
