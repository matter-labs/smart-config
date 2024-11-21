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
    #[config(default = NestedConfig::default_other_int)]
    pub other_int: u32,
    #[config(default)]
    pub map: HashMap<String, u32>,
}

impl NestedConfig {
    const fn default_other_int() -> u32 {
        42
    }
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate, tag = "type")]
pub(crate) enum EnumConfig {
    First,
    Nested(NestedConfig),
    #[config(alias = "Fields")]
    WithFields {
        string: Option<String>,
        #[config(default)]
        flag: bool,
        #[config(default)]
        set: HashSet<u32>,
    },
}
