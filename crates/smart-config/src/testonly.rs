//! Test-only functionality shared among multiple test modules.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use smart_config_derive::DescribeConfig;

use crate::metadata::PrimitiveType;

#[derive(Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SimpleEnum {
    First,
    Second,
}

#[derive(Debug, PartialEq, DescribeConfig)]
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

/*
#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum EnumConfig {
    First,
    Nested(NestedConfig),
    #[serde(alias = "Fields")]
    WithFields {
        string: Option<String>,
        #[serde(default)]
        flag: bool,
        #[serde(default)]
        set: HashSet<u32>,
    },
}
*/
