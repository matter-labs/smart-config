use std::collections::HashSet;

use assert_matches::assert_matches;

use super::*;
use crate::{
    config,
    de::{DeserializeContext, DeserializerOptions},
    testonly::{ComposedConfig, ConfigWithComplexTypes, DefaultingEnumConfig, EnumConfig},
    DescribeConfig, ParseErrors,
};

#[test]
fn describing_enum_config() {
    let metadata = &EnumConfig::DESCRIPTION;
    assert_eq!(metadata.nested_configs.len(), 1);
    assert_eq!(metadata.nested_configs[0].name, "");

    let nested_meta = metadata.nested_configs[0].meta;
    let nested_param_names: HashSet<_> =
        nested_meta.params.iter().map(|param| param.name).collect();
    assert_eq!(
        nested_param_names,
        HashSet::from(["renamed", "other_int", "map"])
    );
    assert_eq!(
        metadata.nested_configs[0].tag_variant.unwrap().name,
        "Nested"
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
    let set_param_default = set_param.default_value_json().unwrap();
    assert!(
        set_param_default == serde_json::json!([42, 23])
            || set_param_default == serde_json::json!([23, 42]),
        "{set_param_default:?}"
    );
    assert_eq!(set_param.tag_variant.unwrap().name, "WithFields");

    let tag = metadata.tag.unwrap();
    assert_eq!(tag.param.expecting, BasicTypes::STRING);
    assert_eq!(tag.variants.len(), 3);
    assert_eq!(tag.variants[0].name, "first");
    assert_eq!(tag.variants[0].rust_name, "First");
    assert_eq!(tag.variants[0].help, "Empty variant.");
    assert_eq!(tag.variants[1].name, "Nested");
    assert_eq!(tag.variants[1].help, "Variant wrapping a flattened config.");
    assert_eq!(tag.variants[2].name, "WithFields");
    assert_eq!(tag.variants[2].aliases, ["Fields", "With"]);
}

#[test]
fn deserializing_config_using_deserializer() {
    let deserializer = EnumConfig::DESCRIPTION.deserializer;

    let mut errors = ParseErrors::default();
    let json = config!("type": "first");
    let config = deserializer(DeserializeContext::new(
        &DeserializerOptions::default(),
        json.inner(),
        String::new(),
        &EnumConfig::DESCRIPTION,
        &mut errors,
    ))
    .unwrap();

    let config: EnumConfig = *config.downcast().unwrap();
    assert_eq!(config, EnumConfig::First);
}

#[test]
fn describing_defaulting_enum_config() {
    let metadata = &DefaultingEnumConfig::DESCRIPTION;
    let tag = metadata.tag.unwrap();
    assert_eq!(tag.param.default_value_json().unwrap(), "Second");
    assert_eq!(tag.default_variant.unwrap().name, "Second");
}

#[test]
fn describing_complex_types() {
    let metadata = &ConfigWithComplexTypes::DESCRIPTION;
    let array_param = metadata
        .params
        .iter()
        .find(|param| param.name == "array")
        .unwrap();
    assert_eq!(
        array_param.expecting,
        BasicTypes::ARRAY.or(BasicTypes::STRING)
    );
    let description = array_param.type_description();
    assert_eq!(
        description.details().unwrap(),
        "2-element array; using \",\" delimiter"
    );
    assert!(!description.contains_secrets());
    let (expected_item, _) = description.items().unwrap();
    assert_eq!(expected_item, BasicTypes::INTEGER);

    let assumed_param = metadata
        .params
        .iter()
        .find(|param| param.name == "assumed")
        .unwrap();
    assert_eq!(assumed_param.expecting, BasicTypes::FLOAT);

    let path_param = metadata
        .params
        .iter()
        .find(|param| param.name == "path")
        .unwrap();
    assert_eq!(path_param.expecting, BasicTypes::STRING);
    let description = path_param.type_description();
    assert_eq!(description.details().unwrap(), "filesystem path");

    let dur_param = metadata
        .params
        .iter()
        .find(|param| param.name == "short_dur")
        .unwrap();
    assert_eq!(dur_param.expecting, BasicTypes::INTEGER);
    let description = dur_param.type_description();
    assert_eq!(description.details().unwrap(), "time duration");
    assert_eq!(description.unit(), Some(TimeUnit::Millis.into()));

    let custom_de_param = metadata
        .params
        .iter()
        .find(|param| param.name == "with_custom_deserializer")
        .unwrap();
    assert_eq!(custom_de_param.expecting, BasicTypes::STRING);
}

#[test]
fn suffixes_for_composed_params() {
    let metadata = &ConfigWithComplexTypes::DESCRIPTION;
    let size_param = metadata
        .params
        .iter()
        .find(|param| param.name == "disk_size")
        .unwrap();
    let ty = size_param.type_description();
    assert_matches!(ty.suffixes, Some(TypeSuffixes::SizeUnits));
    let size_param = metadata
        .params
        .iter()
        .find(|param| param.name == "memory_size_mb")
        .unwrap();
    let ty = size_param.type_description();
    assert_matches!(ty.suffixes, None);

    let metadata = &ComposedConfig::DESCRIPTION;
    let dur_param = metadata
        .params
        .iter()
        .find(|param| param.name == "durations")
        .unwrap();
    let ty = dur_param.type_description();
    assert_matches!(ty.suffixes, None);
}
