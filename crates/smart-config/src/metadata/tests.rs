use std::collections::HashSet;

use super::*;
use crate::testonly::EnumConfig;

#[test]
fn describing_enum_config() {
    let metadata: &ConfigMetadata = EnumConfig::describe_config();
    assert_eq!(metadata.nested_configs.len(), 1);
    assert_eq!(metadata.nested_configs[0].name, "");

    let nested_meta = metadata.nested_configs[0].meta;
    let nested_param_names: HashSet<_> =
        nested_meta.params.iter().map(|param| param.name).collect();
    assert_eq!(nested_param_names, HashSet::from(["renamed", "other_int"]));

    let param_names: HashSet<_> = metadata.params.iter().map(|param| param.name).collect();
    assert_eq!(param_names, HashSet::from(["string", "flag", "type"]));

    let tag_param = metadata
        .params
        .iter()
        .find(|param| param.name == "type")
        .unwrap();
    assert_eq!(tag_param.base_type_kind, TypeKind::String);
}
