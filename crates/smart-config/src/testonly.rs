//! Test-only functionality shared among multiple test modules.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::{metadata::PrimitiveType, DescribeConfig, DeserializeConfig};

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
