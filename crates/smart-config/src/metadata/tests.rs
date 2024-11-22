use std::collections::HashSet;

use super::*;
use crate::{
    testonly::{DefaultingEnumConfig, EnumConfig},
    DescribeConfig,
};

#[test]
fn describing_enum_config() {
    let metadata: &ConfigMetadata = EnumConfig::describe_config();
    assert_eq!(metadata.nested_configs.len(), 1);
    assert_eq!(metadata.nested_configs[0].name, "");

    let nested_meta = metadata.nested_configs[0].meta;
    let nested_param_names: HashSet<_> =
        nested_meta.params.iter().map(|param| param.name).collect();
    assert_eq!(
        nested_param_names,
        HashSet::from(["renamed", "other_int", "map"])
    );

    let param_names: HashSet<_> = metadata.params.iter().map(|param| param.name).collect();
    assert_eq!(
        param_names,
        HashSet::from(["string", "flag", "set", "type"])
    );

    let set_param = metadata
        .params
        .iter()
        .find(|param| param.name == "set")
        .unwrap();
    let set_param_default = format!("{:?}", set_param.default_value().unwrap());
    assert!(set_param_default == "{42, 23}" || set_param_default == "{23, 42}");

    let tag_param = metadata
        .params
        .iter()
        .find(|param| param.name == "type")
        .unwrap();
    assert_eq!(
        tag_param.deserializer.expecting().base,
        Some(BasicType::String)
    );
}

#[test]
fn describing_defaulting_enum_config() {
    let metadata: &ConfigMetadata = DefaultingEnumConfig::describe_config();
    let tag_param = metadata
        .params
        .iter()
        .find(|param| param.name == "kind")
        .unwrap();
    let default = format!("{:?}", tag_param.default_value().unwrap());
    assert_eq!(default, "\"Second\"");
}
