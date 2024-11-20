//! Test-only functionality shared among multiple test modules.

use serde::Deserialize;
use smart_config_derive::DescribeConfig;

use crate::metadata::TypeKind;

#[derive(Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SimpleEnum {
    First,
    Second,
}

#[derive(Debug, PartialEq, Deserialize, DescribeConfig)]
#[config(crate = crate)]
pub(crate) struct NestedConfig {
    #[serde(rename = "renamed")]
    #[config(kind = TypeKind::String)]
    pub simple_enum: SimpleEnum,
    #[serde(default = "NestedConfig::default_other_int")]
    pub other_int: u32,
}

impl NestedConfig {
    const fn default_other_int() -> u32 {
        42
    }
}

#[derive(Debug, PartialEq, Deserialize, DescribeConfig)]
#[config(crate = crate)]
#[serde(tag = "type")]
pub(crate) enum EnumConfig {
    First,
    Nested(NestedConfig),
    #[serde(alias = "Fields")]
    WithFields {
        string: Option<String>,
        #[serde(default)]
        flag: bool,
    },
}
