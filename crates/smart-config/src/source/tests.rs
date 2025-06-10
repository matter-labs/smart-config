use std::{
    any,
    collections::{HashMap, HashSet},
    time::Duration,
};

use assert_matches::assert_matches;
use secrecy::ExposeSecret;

use super::*;
use crate::{
    metadata::SizeUnit,
    testing,
    testing::MockEnvGuard,
    testonly::{
        extract_env_var_name, extract_json_name, test_config_roundtrip, test_deserialize,
        AliasedConfig, ComposedConfig, CompoundConfig, ConfigWithComplexTypes, ConfigWithFallbacks,
        ConfigWithNestedValidations, ConfigWithNesting, ConfigWithValidations, DefaultingConfig,
        EnumConfig, KvTestConfig, NestedConfig, RenamedEnumConfig, SecretConfig, SimpleEnum,
        ValueCoercingConfig,
    },
    value::StrValue,
    ByteSize, DescribeConfig, SerializerOptions,
};

#[test]
fn parsing_enum_config_with_schema() {
    let schema = ConfigSchema::new(&EnumConfig::DESCRIPTION, "");

    let json = config!(
        "type": "Nested",
        "renamed": "second",
        "map.first": 1,
        "map.second": 2,
    );
    let repo = ConfigRepository::new(&schema).with(json);
    let config: EnumConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(
        config,
        EnumConfig::Nested(NestedConfig {
            simple_enum: SimpleEnum::Second,
            other_int: 42,
            map: HashMap::from([("first".to_owned(), 1), ("second".to_owned(), 2)]),
        })
    );

    // Test coercing config variants for an enum used directly as a param
    let enum_values = [
        "FIRST",
        "First",
        "FIRST_CHOICE",
        "FirstChoice",
        "firstChoice",
        "first-choice",
    ];
    for enum_value in enum_values {
        println!("testing enum value: {enum_value}");

        let json = config!(
            "type": "Nested",
            "renamed": enum_value,
            "map.first": 1,
        );
        let mut repo = ConfigRepository::new(&schema).with(json);
        let errors = repo.single::<EnumConfig>().unwrap().parse().unwrap_err();
        let err = errors.first();
        let inner = err.inner().to_string();
        assert!(inner.contains("unknown variant"), "{inner}");
        assert_eq!(err.path(), "renamed");

        repo.deserializer_options().coerce_variant_names = true;
        let config: EnumConfig = repo.single().unwrap().parse().unwrap();
        assert_eq!(
            config,
            EnumConfig::Nested(NestedConfig {
                simple_enum: SimpleEnum::First,
                other_int: 42,
                map: HashMap::from([("first".to_owned(), 1)]),
            })
        );
    }

    let json = config!(
        "type": "Fields",
        "string": "???",
        "flag": true,
        "set": [42],
    );
    let repo = ConfigRepository::new(&schema).with(json);
    let config: EnumConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(
        config,
        EnumConfig::WithFields {
            string: Some("???".to_owned()),
            flag: true,
            set: HashSet::from([42]),
        }
    );

    let env = Environment::from_iter(
        "",
        [
            ("type", "With"),
            ("renamed", "second"),
            ("string", "???"),
            ("flag", "false"),
        ],
    );
    let repo = ConfigRepository::new(&schema).with(env);
    assert_matches!(
        &repo.merged().get(Pointer("flag")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "false"
    );

    let config: EnumConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(
        config,
        EnumConfig::WithFields {
            string: Some("???".to_owned()),
            flag: false,
            set: HashSet::from([23, 42]),
        }
    );
}

#[test]
fn serializing_enum_config() {
    let config = EnumConfig::First;
    let json = test_config_roundtrip(&config);
    assert_eq!(
        serde_json::Value::from(json),
        serde_json::json!({ "type": "first" })
    );

    let config = EnumConfig::Nested(NestedConfig {
        simple_enum: SimpleEnum::First,
        other_int: 42,
        map: HashMap::from([("call".to_owned(), 100)]),
    });
    let json = test_config_roundtrip(&config);
    assert_eq!(
        serde_json::Value::from(json),
        serde_json::json!({
            "type": "Nested",
            "renamed": "first",
            "other_int": 42,
            "map": { "call": 100 },
        })
    );

    let config = EnumConfig::WithFields {
        string: None,
        flag: true,
        set: HashSet::from([42]),
    };
    let json = test_config_roundtrip(&config);
    assert_eq!(
        serde_json::Value::from(json),
        serde_json::json!({
            "type": "WithFields",
            "string": null,
            "flag": true,
            "set": [42],
        })
    );
}

#[test]
fn parsing_defaulting_config_from_missing_value_with_schema() {
    let schema = ConfigSchema::new(&DefaultingConfig::DESCRIPTION, "test");
    let json = config!("unrelated": 123);
    let repo = ConfigRepository::new(&schema).with(json);
    let config: DefaultingConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config, DefaultingConfig::default());
}

#[test]
fn parsing_compound_config_with_schema() {
    let json = config!(
        "nested.renamed": "first",
        "renamed": "second",
        "other_int": 123,
    );

    let schema = ConfigSchema::new(&CompoundConfig::DESCRIPTION, "");
    let repo = ConfigRepository::new(&schema).with(json);
    let config: CompoundConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(
        config.nested,
        NestedConfig {
            simple_enum: SimpleEnum::First,
            other_int: 42,
            map: HashMap::new(),
        }
    );
    assert_eq!(config.nested_default, NestedConfig::default_nested());
    assert_eq!(
        config.flat,
        NestedConfig {
            simple_enum: SimpleEnum::Second,
            other_int: 123,
            map: HashMap::new(),
        }
    );
    assert_eq!(config.nested_opt, None);
}

fn test_parsing_compound_config_with_schema_error(json: Json, expected_err_path: &str) {
    let schema = ConfigSchema::new(&CompoundConfig::DESCRIPTION, "");
    let repo = ConfigRepository::new(&schema).with(json);
    let err = repo
        .single::<CompoundConfig>()
        .unwrap()
        .parse()
        .unwrap_err();
    assert_eq!(err.len(), 1, "{err:?}");
    let err = err.first();
    assert_eq!(err.path(), expected_err_path);
    let inner = err.inner().to_string();
    assert!(inner.contains("expected config object"), "{inner}");
}

#[test]
fn parsing_compound_config_with_schema_error() {
    let json = config!(
        "nested": 123,
        "renamed": "second",
    );
    test_parsing_compound_config_with_schema_error(json, "nested");

    let json = config!(
        "nested": "what",
        "renamed": "second",
    );
    test_parsing_compound_config_with_schema_error(json, "nested");

    let json = config!(
        "nested": (),
        "renamed": "second",
    );
    test_parsing_compound_config_with_schema_error(json, "nested");

    let json = config!(
        "nested": false,
        "renamed": "second",
    );
    test_parsing_compound_config_with_schema_error(json, "nested");

    let json = config!(
        "nested": [1, 2, 3],
        "renamed": "second",
    );
    test_parsing_compound_config_with_schema_error(json, "nested");

    let json = config!(
        "nested.renamed": "first",
        "nested_opt": false,
        "renamed": "second",
    );
    test_parsing_compound_config_with_schema_error(json, "nested_opt");
}

#[test]
fn nesting_json() {
    let env = Environment::from_iter(
        "",
        [
            ("value".to_owned(), "123".to_owned()),
            ("nested_renamed".to_owned(), "first".to_owned()),
            ("nested_other_int".to_owned(), "321".to_owned()),
        ],
    );

    let schema = ConfigSchema::new(&ConfigWithNesting::DESCRIPTION, "");
    let map = ConfigRepository::new(&schema).with(env).merged;

    assert_matches!(
        &map.get(Pointer("value")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "123"
    );
    assert_matches!(
        &map.get(Pointer("nested.renamed")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "first"
    );
    assert_matches!(
        &map.get(Pointer("nested.other_int")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "321"
    );

    let config: ConfigWithNesting = test_deserialize(&map).unwrap();
    assert_eq!(config.value, 123);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 321);
}

#[test]
fn nesting_inside_child_config() {
    let json = config!(
        "value": 123,
        "nested_renamed": "first",
        "nested_other_int": 321,
    );
    let schema = ConfigSchema::new(&ConfigWithNesting::DESCRIPTION, "");
    let map = ConfigRepository::new(&schema).with(json).merged;

    assert_matches!(
        &map.get(Pointer("value")).unwrap().inner,
        Value::Number(num) if *num == 123.into()
    );
    assert_matches!(
        &map.get(Pointer("nested.renamed")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "first"
    );
    assert_matches!(
        &map.get(Pointer("nested.other_int")).unwrap().inner,
        Value::Number(num) if *num == 321.into()
    );

    let json = config!(
        "value": 123,
        "nested_renamed": "first",
        "nested_other_int": 321,
        "nested.other_int": 777, // has priority
    );
    let schema = ConfigSchema::new(&ConfigWithNesting::DESCRIPTION, "");
    let map = ConfigRepository::new(&schema).with(json).merged;

    assert_matches!(
        &map.get(Pointer("nested.renamed")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "first"
    );
    assert_matches!(
        &map.get(Pointer("nested.other_int")).unwrap().inner,
        Value::Number(num) if *num == 777.into()
    );
}

#[test]
fn merging_config_parts() {
    let json = config!(
        "deprecated.value": 4,
        "nested.renamed": "first",
    );

    let mut schema = ConfigSchema::default();
    schema
        .insert(&ConfigWithNesting::DESCRIPTION, "")
        .unwrap()
        .push_alias("deprecated")
        .unwrap();
    schema
        .single_mut(&NestedConfig::DESCRIPTION)
        .unwrap()
        .push_alias("deprecated")
        .unwrap();

    let repo = ConfigRepository::new(&schema).with(json);
    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.value, 4);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 42);

    let json = config!(
        "value": 123,
        "deprecated.value": 4,
        "nested.renamed": "first",
        "deprecated.other_int": 321,
        "deprecated.merged": "!",
    );

    let repo = ConfigRepository::new(&schema).with(json);
    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.value, 123);
    assert_eq!(config.merged, "!");
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 321);

    let json = config!(
        "deprecated.value": 4,
        "nested.renamed": "first",
        "deprecated.alias": "?",
    );
    let repo = ConfigRepository::new(&schema).with(json);
    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.merged, "?");

    let json = config!(
        "deprecated.value": 4,
        "nested.renamed": "first",
        "deprecated.merged": "!", // has priority compared to alias
        "deprecated.alias": "?",
    );
    let repo = ConfigRepository::new(&schema).with(json);
    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.merged, "!");

    let json = config!(
        "deprecated.value": 4,
        "nested.renamed": "first",
        "alias": "???", // has higher priority than any global alias
        "deprecated.merged": "!",
        "deprecated.alias": "?",
    );
    let repo = ConfigRepository::new(&schema).with(json);
    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.merged, "???");
}

#[test]
fn using_nested_config_aliases() {
    let json = config!(
        "value": 10,
        "nest.renamed": "first",
        "nest.other_int": 50,
    );
    let config: ConfigWithNesting = testing::test(json).unwrap();
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 50);

    // Mixing canonical path and aliases
    let json = config!(
        "value": 10,
        "nested.renamed": "first",
        "nest.other_int": 50,
    );
    let config: ConfigWithNesting = testing::test(json).unwrap();
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 50);
    let json = test_config_roundtrip(&config);
    assert_eq!(
        serde_json::Value::from(json),
        serde_json::json!({
            "merged": "",
            "value": 10,
            "nested": {
                "renamed": "first",
                "other_int": 50,
                "map": {},
            },
        })
    );

    let json = config!(
        "value": 10,
        "nest.renamed": "first",
        "nested.other_int": 777,
        "nest.other_int": 50, // shouldn't be used since there's a canonical param
    );
    let config: ConfigWithNesting = testing::test(json).unwrap();
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 777);
}

#[test]
fn merging_config_parts_with_env() {
    let env = Environment::from_iter("", [("deprecated_value", "4"), ("nested_renamed", "first")]);

    let mut schema = ConfigSchema::default();
    schema
        .insert(&ConfigWithNesting::DESCRIPTION, "")
        .unwrap()
        .push_alias("deprecated")
        .unwrap();
    schema
        .single_mut(&NestedConfig::DESCRIPTION)
        .unwrap()
        .push_alias("deprecated")
        .unwrap();

    let repo = ConfigRepository::new(&schema).with(env);
    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.value, 4);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 42);

    let env = Environment::from_iter(
        "",
        [
            ("value", "123"),
            ("deprecated_value", "4"),
            ("nested_renamed", "first"),
            ("deprecated_other_int", "321"),
            ("deprecated_merged", "!"),
        ],
    );

    let repo = ConfigRepository::new(&schema).with(env);
    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.value, 123);
    assert_eq!(config.merged, "!");
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 321);
}

#[test]
fn merging_configs() {
    let json = serde_json::json!({
        "value": 123,
        "merged": "!!",
        "nested": {
            "other_int": 321,
            "renamed": "first",
        },
    });
    let serde_json::Value::Object(json) = json else {
        unreachable!();
    };
    let base = Json::new("base.json", json);

    let json = serde_json::json!({
        "merged": "??",
        "nested": {
            "enum": "second",
            "map": HashMap::from([("first", 5)]),
        },
    });
    let serde_json::Value::Object(json) = json else {
        unreachable!();
    };
    let overrides = Json::new("overrides.json", json);

    let schema = ConfigSchema::new(&ConfigWithNesting::DESCRIPTION, "");
    let repo = ConfigRepository::new(&schema).with(base).with(overrides);
    let Value::Object(merged) = &repo.merged().inner else {
        panic!("unexpected merged value");
    };

    assert_matches!(&merged["value"].inner, Value::Number(num) if *num == 123_u64.into());
    assert_matches!(
        merged["value"].origin.as_ref(),
        ValueOrigin::Path { source, .. } if extract_json_name(source) == "base.json"
    );
    assert_matches!(&merged["merged"].inner, Value::String(StrValue::Plain(s)) if s == "??");
    assert_matches!(
        merged["merged"].origin.as_ref(),
        ValueOrigin::Path { source, .. } if extract_json_name(source) == "overrides.json"
    );

    assert_matches!(
        &merged["nested"].inner,
        Value::Object(items) if items.len() == 3
    );
    let nested_int = merged["nested"].get(Pointer("other_int")).unwrap();
    assert_matches!(&nested_int.inner, Value::Number(num) if *num == 321_u64.into());
    assert_matches!(
        nested_int.origin.as_ref(),
        ValueOrigin::Path { source, .. } if extract_json_name(source) == "base.json"
    );

    let nested_str = merged["nested"].get(Pointer("renamed")).unwrap();
    assert_matches!(&nested_str.inner, Value::String(StrValue::Plain(s)) if s == "second");
    assert_matches!(
        nested_str.origin.as_ref(),
        ValueOrigin::Path { source, .. } if extract_json_name(source) == "overrides.json"
    );

    let sources = repo.sources();
    assert_eq!(sources.len(), 2);
    assert_eq!(extract_json_name(&sources[0].origin), "base.json");
    assert_eq!(extract_json_name(&sources[1].origin), "overrides.json");
    assert_eq!(sources[0].param_count, 4);
    assert_eq!(sources[1].param_count, 3);
}

#[test]
fn using_aliases_with_object_config() {
    let mut schema = ConfigSchema::default();
    schema
        .insert(&ConfigWithNesting::DESCRIPTION, "test")
        .unwrap()
        .push_alias("deprecated")
        .unwrap();

    let json = config!(
        "value": 123, // Should not be used.
        "deprecated.value": 321,
        "test.nested.renamed": "first",
    );
    let repo = ConfigRepository::new(&schema).with(json);

    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.value, 321);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 42);
}

#[test]
fn using_env_config_overrides() {
    let mut schema = ConfigSchema::default();
    schema
        .insert(&ConfigWithNesting::DESCRIPTION, "test")
        .unwrap()
        .push_alias("deprecated")
        .unwrap();

    let base = config!(
        "value": 123, // Should not be used.
        "deprecated.value": 321,
        "test.nested.renamed": "first",
    );
    let mut repo = ConfigRepository::new(&schema).with(base);

    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.value, 321);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 42);

    let env = Environment::from_iter(
        "",
        [
            ("DEPRECATED_VALUE", "777"),
            ("TEST_NESTED_RENAMED", "second"),
        ],
    );
    repo = repo.with(env);

    let enum_value = repo.merged().get(Pointer("test.nested.renamed")).unwrap();
    assert_matches!(&enum_value.inner, Value::String(StrValue::Plain(s)) if s == "second");
    extract_env_var_name(&enum_value.origin);

    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.value, 777);
    assert_eq!(config.nested.simple_enum, SimpleEnum::Second);

    let env = Environment::from_iter("", [("TEST_VALUE", "555")]);
    repo = repo.with(env);

    let int_value = repo.merged().get(Pointer("test.value")).unwrap();
    assert_matches!(&int_value.inner, Value::String(StrValue::Plain(s)) if s == "555");

    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.value, 555);
    assert_eq!(config.nested.simple_enum, SimpleEnum::Second);
}

#[test]
fn parsing_complex_param() {
    let json = config!(
        "param": serde_json::json!({
            "int": 4,
            "string": "??",
            "repeated": ["first"],
        }),
        "set": [1, 2, 3],
    );
    let config: ValueCoercingConfig = testing::test(json).unwrap();
    assert_eq!(config.param.int, 4);
    assert_eq!(config.param.string, "??");
    assert_eq!(config.param.repeated, HashSet::from([SimpleEnum::First]));
    assert_eq!(config.set, HashSet::from([1, 2, 3]));

    let json = SerializerOptions::default().serialize(&config);
    assert!(json.contains_key("repeated"), "{json:?}");
    assert_eq!(
        json["param"],
        serde_json::json!({
            "array": [],
            "bool": false,
            "int": 4,
            "optional": null,
            "repeated": ["first"],
            "string": "??",
        })
    );

    let config_copy: ValueCoercingConfig =
        testing::test(Json::new("test.json", json.clone())).unwrap();
    assert_eq!(config_copy, config);

    let json_diff = SerializerOptions::diff_with_default().serialize(&config);
    assert!(!json_diff.contains_key("repeated"), "{json_diff:?}");
    assert_eq!(json_diff["param"], json["param"]);

    let mut env = Environment::from_iter(
        "",
        [
            (
                "PARAM__JSON",
                r#"{ "int": 3, "string": "!!", "repeated": ["second"] }"#,
            ),
            ("SET__JSON", "[2, 3]"),
        ],
    );
    env.coerce_json().unwrap();
    let config: ValueCoercingConfig = testing::test(env).unwrap();
    assert_eq!(config.param.int, 3);
    assert_eq!(config.param.string, "!!");
    assert_eq!(config.param.repeated, HashSet::from([SimpleEnum::Second]));
    assert_eq!(config.set, HashSet::from([2, 3]));

    test_config_roundtrip(&config);
}

#[test]
fn parsing_complex_param_errors() {
    let mut env = Environment::from_iter("", [("PARAM__JSON", r#"{ "int": "???" }"#)]);
    env.coerce_json().unwrap();
    let err = testing::test::<ValueCoercingConfig>(env).unwrap_err();
    assert_eq!(err.len(), 1);
    let err = err.first();
    assert_eq!(err.path(), "param");
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid digit"), "{inner}");
    assert_eq!(
        err.origin().to_string(),
        "env variable 'PARAM__JSON' -> parsed JSON string -> path 'int'"
    );

    let mut env = Environment::from_iter(
        "APP_",
        [
            ("APP_PARAM__JSON", r#"{ "int": 42, "string": "!" }"#),
            ("APP_SET__JSON", "[1, false]"),
        ],
    );
    env.coerce_json().unwrap();
    let err = testing::test::<ValueCoercingConfig>(env).unwrap_err();
    assert_eq!(err.len(), 1);
    let err = err.first();
    assert_eq!(err.path(), "set.1");
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid type"), "{inner}");
    assert_eq!(
        err.origin().to_string(),
        "env variable 'APP_SET__JSON' -> parsed JSON string -> path '1'"
    );
}

#[test]
fn merging_params_is_atomic() {
    let base = config!(
        "param": serde_json::json!({
            "int": 4,
            "string": "??",
            "bool": true,
        }),
    );
    let overrides = config!(
        "param": serde_json::json!({
            "int": 3,
            "string": "!!",
        }),
    );
    let schema = ConfigSchema::new(&ValueCoercingConfig::DESCRIPTION, "");
    let repo = ConfigRepository::new(&schema).with(base).with(overrides);
    let param_value = &repo.merged().get(Pointer("param")).unwrap().inner;
    assert_matches!(
        param_value,
        Value::Object(map) if map.len() == 2 && !map.contains_key("bool")
    );

    let config: ValueCoercingConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.param.int, 3);
    assert_eq!(config.param.string, "!!");
    assert!(!config.param.bool);
}

#[test]
fn merging_params_is_still_atomic_with_prefixes() {
    let base = config!(
        "test.config.param": serde_json::json!({
            "int": 4,
            "string": "??",
            "bool": true,
        }),
        "test.unused": 123,
    );
    let overrides = config!(
        "test.config.param": serde_json::json!({
            "int": 3,
            "string": "!!",
        }),
        "test.config.unused": true,
    );
    let schema = ConfigSchema::new(&ValueCoercingConfig::DESCRIPTION, "test.config");
    let repo = ConfigRepository::new(&schema).with(base).with(overrides);
    let param_value = &repo
        .merged()
        .get(Pointer("test.config.param"))
        .unwrap()
        .inner;
    assert_matches!(
        param_value,
        Value::Object(map) if map.len() == 2 && !map.contains_key("bool")
    );

    let config: ValueCoercingConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.param.int, 3);
    assert_eq!(config.param.string, "!!");
    assert!(!config.param.bool);
}

#[test]
fn nesting_key_value_map_to_multiple_locations() {
    let schema = ConfigSchema::new(&KvTestConfig::DESCRIPTION, "");
    let mut repo = ConfigRepository::new(&schema);
    let config: KvTestConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.nested_int, -3);
    assert_eq!(config.nested.int, 12);

    let env = Environment::from_iter("", [("NESTED_INT", "123")]);
    repo = repo.with(env);
    let config: KvTestConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.nested_int, 123);
    assert_eq!(config.nested.int, 123);
}

#[test]
fn nesting_for_object_param() {
    let schema = ConfigSchema::new(&ValueCoercingConfig::DESCRIPTION, "test");
    let env = Environment::from_iter("", [("TEST_PARAM_INT", "123"), ("TEST_PARAM_STRING", "??")]);
    let repo = ConfigRepository::new(&schema).with(env);

    let object = repo.merged().get(Pointer("test.param")).unwrap();
    assert_matches!(
        object.origin.as_ref(),
        ValueOrigin::Synthetic { transform, .. } if transform.contains("object param")
    );
    assert_matches!(
        &object.inner,
        Value::Object(obj) if obj.len() == 2 && obj.contains_key("int") && obj.contains_key("string")
    );

    let config: ValueCoercingConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.param.int, 123);
    assert_eq!(config.param.string, "??");
}

#[test]
fn nesting_for_object_param_with_structured_source() {
    let schema = ConfigSchema::new(&ValueCoercingConfig::DESCRIPTION, "test");
    let json = config!(
        "test.param_int": 123,
        "test.param.string": "??",
    );
    let repo = ConfigRepository::new(&schema).with(json);

    let object = repo.merged().get(Pointer("test.param")).unwrap();
    assert_matches!(
        &object.inner,
        Value::Object(obj) if obj.len() == 2 && obj.contains_key("int") && obj.contains_key("string")
    );

    let config: ValueCoercingConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.param.int, 123);
    assert_eq!(config.param.string, "??");
}

#[test]
fn nesting_for_array_param() {
    let schema = ConfigSchema::new(&ValueCoercingConfig::DESCRIPTION, "test");
    let mut env = Environment::from_iter(
        "",
        [
            ("TEST_PARAM_INT", "123"),
            ("TEST_PARAM_STRING", "??"),
            ("TEST_SET_0", "123"),
            ("TEST_SET_1", "321"),
            ("TEST_SET_2", "777"),
            ("TEST_REPEATED_0__JSON", r#"{ "int": 123, "string": "!" }"#),
            (
                "TEST_REPEATED_1__JSON",
                r#"{ "int": 321, "string": "?", "array": [1, 2] }"#,
            ),
        ],
    );
    env.coerce_json().unwrap();
    let repo = ConfigRepository::new(&schema).with(env);

    assert_matches!(
        &repo.merged().get(Pointer("test.set")).unwrap().inner,
        Value::Array(_)
    );
    assert_matches!(
        &repo.merged().get(Pointer("test.repeated")).unwrap().inner,
        Value::Array(_)
    );

    let config: ValueCoercingConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.set, HashSet::from([123, 321, 777]));
    assert_eq!(config.repeated.len(), 2);
    assert_eq!(config.repeated[0].int, 123);
    assert_eq!(config.repeated[0].string, "!");
    assert_eq!(config.repeated[1].int, 321);
    assert_eq!(config.repeated[1].string, "?");
    assert_eq!(config.repeated[1].array, [1, 2]);
}

#[test]
fn nesting_not_applied_if_original_param_is_defined() {
    let schema = ConfigSchema::new(&ValueCoercingConfig::DESCRIPTION, "test");
    let env = Environment::from_iter(
        "",
        [
            ("TEST_PARAM", r#"{ "int": 42 }"#),
            ("TEST_PARAM_INT", "123"),
        ],
    );
    let repo = ConfigRepository::new(&schema).with(env);
    let val = &repo.merged().get(Pointer("test.param")).unwrap().inner;
    assert_matches!(val, Value::String(StrValue::Plain(s)) if s == r#"{ "int": 42 }"#);

    let env = Environment::from_iter(
        "",
        [
            ("TEST_SET", "[]"),
            ("TEST_SET_0", "123"),
            ("TEST_SET_1", "321"),
        ],
    );
    let repo = ConfigRepository::new(&schema).with(env);

    assert_matches!(
        &repo.merged().get(Pointer("test.set")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "[]"
    );
}

#[test]
fn nesting_not_applied_for_non_sequential_array_indices() {
    let schema = ConfigSchema::new(&ValueCoercingConfig::DESCRIPTION, "test");
    let env = Environment::from_iter("", [("TEST_SET_1", "123"), ("TEST_SET_2", "321")]);
    let repo = ConfigRepository::new(&schema).with(env);
    let set = repo.merged().get(Pointer("test.set"));
    assert!(set.is_none(), "{set:?}");

    let env = Environment::from_iter("", [("TEST_SET_0", "123"), ("TEST_SET_2", "321")]);
    let repo = ConfigRepository::new(&schema).with(env);
    let set = repo.merged().get(Pointer("test.set"));
    assert!(set.is_none(), "{set:?}");
}

#[test]
fn nesting_does_not_override_existing_values() {
    let schema = ConfigSchema::new(&ValueCoercingConfig::DESCRIPTION, "test");
    let json = config!(
        "test.param_int": 123,
        "test.param_string": "!!",
        "test.param.string": "??",
    );
    let repo = ConfigRepository::new(&schema).with(json);

    let object = repo.merged().get(Pointer("test.param")).unwrap();
    assert_matches!(
        &object.inner,
        Value::Object(obj) if obj.len() == 2 && obj.contains_key("int") && obj.contains_key("string")
    );

    let config: ValueCoercingConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.param.int, 123);
    assert_eq!(config.param.string, "??");
}

#[test]
fn nesting_with_duration_param() {
    let json = config!("array": [4, 5], "long_dur_sec": 30);
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(30));
    test_config_roundtrip(&config);

    let json = config!("array": [4, 5], "long_dur_hours": "4");
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(3_600 * 4));
    test_config_roundtrip(&config);

    let json = config!("array": [4, 5], "long_dur_in_hours": "4");
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(3_600 * 4));
    test_config_roundtrip(&config);

    let json = config!("array": [4, 5], "long_dur": "3min");
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(60 * 3));
    test_config_roundtrip(&config);

    let json = config!("array": [4, 5], "long_dur": HashMap::from([("days", 1)]));
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(86_400));
    test_config_roundtrip(&config);

    let json = config!("array": [4, 5], "long_dur": HashMap::from([("in_days", 1)]));
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(86_400));
    test_config_roundtrip(&config);
}

#[test]
fn nesting_with_aliased_duration_param() {
    // Sanity check: the alias should be recognized.
    let json = config!("array": [4, 5], "long_timeout": "30s");
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(30));
    test_config_roundtrip(&config);

    let json = config!("array": [4, 5], "long_timeout_sec": 30);
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(30));
    test_config_roundtrip(&config);

    // Test global aliases as well.
    let mut schema = ConfigSchema::default();
    schema
        .insert(&ConfigWithComplexTypes::DESCRIPTION, "test")
        .unwrap()
        .push_alias("long.alias")
        .unwrap();
    let json = config!("array": [4, 5], "long_timeout_sec": 30);
    let mut repo = ConfigRepository::new(&schema).with(Prefixed::new(json, "long.alias"));
    let config: ConfigWithComplexTypes = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(30));

    let env = Environment::from_iter("", [("LONG_ALIAS_LONG_DUR_MIN", "1")]);
    repo = repo.with(env);
    let config: ConfigWithComplexTypes = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(60));
}

#[test]
fn nesting_with_byte_size_param() {
    let json = config!("array": [4, 5], "disk_size_mb": 64);
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.disk_size.unwrap(), 64 * SizeUnit::MiB);
    test_config_roundtrip(&config);

    let json = config!("array": [4, 5], "disk_size_in_mb": 64);
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.disk_size.unwrap(), 64 * SizeUnit::MiB);
    test_config_roundtrip(&config);

    let json = config!("array": [4, 5], "disk_size": "2 GiB");
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.disk_size.unwrap(), 2 * SizeUnit::GiB);
    test_config_roundtrip(&config);

    let json = config!("array": [4, 5], "disk_size": HashMap::from([("kib", 512)]));
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.disk_size.unwrap(), 512 * SizeUnit::KiB);
    test_config_roundtrip(&config);

    let json = config!("array": [4, 5], "disk_size": HashMap::from([("in_kib", 512)]));
    let config: ConfigWithComplexTypes = testing::test(json).unwrap();
    assert_eq!(config.disk_size.unwrap(), 512 * SizeUnit::KiB);
    test_config_roundtrip(&config);
}

#[test]
fn nesting_with_duration_param_errors() {
    fn assert_error(err: &ParseErrors) -> &ParseError {
        assert_eq!(err.len(), 1);
        let err = err.first();
        assert_eq!(err.path(), "long_dur");
        assert_eq!(err.param().unwrap().name, "long_dur");
        err
    }

    let env = Environment::from_iter("", [("ARRAY", "4,5"), ("LONG_DUR_SEC", "what")]);
    let err = testing::test::<ConfigWithComplexTypes>(env).unwrap_err();
    let err = assert_error(&err);
    assert_matches!(err.origin(), ValueOrigin::Path { path, .. } if path == "LONG_DUR_SEC");
    let inner = err.inner().to_string();
    assert!(inner.contains("what"), "{inner}");

    let env = Environment::from_iter("", [("ARRAY", "4,5"), ("LONG_DUR_WHAT", "123")]);
    let err = testing::test::<ConfigWithComplexTypes>(env).unwrap_err();
    let err = assert_error(&err);
    let inner = err.inner().to_string();
    assert!(inner.contains("unknown variant"), "{inner}");

    let env = Environment::from_iter("", [("ARRAY", "4,5"), ("LONG_DUR", "123 years")]);
    let err = testing::test::<ConfigWithComplexTypes>(env).unwrap_err();
    let err = assert_error(&err);
    assert_matches!(err.origin(), ValueOrigin::Path { path, .. } if path == "LONG_DUR");
    let inner = err.inner().to_string();
    assert!(inner.contains("unknown variant"), "{inner}");

    let env = Environment::from_iter(
        "",
        [
            ("ARRAY", "4,5"),
            ("LONG_DUR_SECS", "12"), // ambiguous qualifier
            ("LONG_DUR_MIN", "1"),
        ],
    );
    let err = testing::test::<ConfigWithComplexTypes>(env).unwrap_err();
    let err = assert_error(&err);
    assert_matches!(err.origin(), ValueOrigin::Synthetic { .. });
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid type"), "{inner}");
}

#[test]
fn merging_duration_params_is_atomic() {
    let schema = ConfigSchema::new(&ConfigWithComplexTypes::DESCRIPTION, "test");

    // Base case: the duration is defined only in overrides
    let base = config!("test.array": [4, 5]);
    let overrides = config!("test.long_dur": "3 secs");
    let repo = ConfigRepository::new(&schema).with(base).with(overrides);
    assert_matches!(
        &repo.merged().get(Pointer("test.long_dur")).unwrap().inner,
        Value::String(_)
    );

    let config: ConfigWithComplexTypes = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(3));

    // Structured override
    let base = config!("test.array": [4, 5], "test.long_dur": "3 secs");
    let overrides = config!("test.long_dur": HashMap::from([("ms", 500)]));
    let repo = ConfigRepository::new(&schema).with(base).with(overrides);
    assert_matches!(
        &repo.merged().get(Pointer("test.long_dur")).unwrap().inner,
        Value::Object(_)
    );

    let config: ConfigWithComplexTypes = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.long_dur, Duration::from_millis(500));

    // Prefixed override
    let base = config!("test.array": [4, 5], "test.long_dur": "3 secs");
    let overrides = Environment::from_iter("", [("TEST_LONG_DUR_IN_MIN", "1")]);
    let repo = ConfigRepository::new(&schema).with(base).with(overrides);
    assert_matches!(
        &repo.merged().get(Pointer("test.long_dur")).unwrap().inner,
        Value::Object(_)
    );

    let config: ConfigWithComplexTypes = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(60));

    // Prefixed base and override
    let base = config!("test.array": [4, 5], "test.long_dur_in_secs": "3");
    let mut repo = ConfigRepository::new(&schema).with(base);
    let config: ConfigWithComplexTypes = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(3));

    let overrides = Environment::from_iter("", [("TEST_LONG_DUR_MIN", "2")]);
    repo = repo.with(overrides);
    assert_matches!(
        &repo.merged().get(Pointer("test.long_dur")).unwrap().inner,
        Value::Object(_)
    );

    let config: ConfigWithComplexTypes = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.long_dur, Duration::from_secs(120));
}

#[test]
fn nesting_with_composed_deserializers() {
    let mut env = Environment::from_iter(
        "",
        [
            ("arrays:json", "[[1, 2], [3, 4], [5, 6]]"),
            ("durations:json", r#"["1 sec", "3 min"]"#),
            ("delimited_durations", "3ms,5sec,2hr"),
            ("map_of_sizes_small", "3 KiB"),
            ("map_of_sizes_large", "5 MiB"),
        ],
    );
    env.coerce_json().unwrap();

    let config: ComposedConfig = testing::test(env).unwrap();
    assert_eq!(config.arrays, HashSet::from([[1, 2], [3, 4], [5, 6]]));
    assert_eq!(
        config.durations,
        [Duration::from_secs(1), Duration::from_secs(3 * 60)]
    );
    assert_eq!(
        config.delimited_durations,
        [
            Duration::from_millis(3),
            Duration::from_secs(5),
            Duration::from_secs(2 * 3_600)
        ]
    );
    assert_eq!(
        config.map_of_sizes,
        HashMap::from([
            ("small".to_owned(), ByteSize(3 << 10)),
            ("large".to_owned(), ByteSize(5 << 20)),
        ])
    );
    test_config_roundtrip(&config);
}

#[test]
fn nesting_with_composed_deserializers_errors() {
    let mut env = Environment::from_iter("", [("arrays:json", "[[1, 2], [3, 4], [-5, 6]]")]);
    env.coerce_json().unwrap();
    let err = testing::test::<ComposedConfig>(env).unwrap_err();
    assert_eq!(err.len(), 1);
    let err = err.first();
    assert_eq!(err.path(), "arrays.2.0");
    let origin = err.origin().to_string();
    assert!(
        origin.ends_with("-> parsed JSON string -> path '2.0'"),
        "{origin}"
    );
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid value"), "{inner}");

    let json = config!("durations": [1]);
    let err = testing::test::<ComposedConfig>(json).unwrap_err();
    assert_eq!(err.len(), 1);
    let err = err.first();
    assert_eq!(err.path(), "durations.0");
    let origin = err.origin().to_string();
    assert!(origin.ends_with("-> path 'durations.0'"), "{origin}");
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid type"), "{inner}");

    let json = config!("map_of_sizes_small": "20 gajillion bytes");
    let err = testing::test::<ComposedConfig>(json).unwrap_err();
    assert_eq!(err.len(), 1);
    let err = err.first();
    assert_eq!(err.path(), "map_of_sizes.small");
    let origin = err.origin().to_string();
    assert!(origin.ends_with("-> path 'map_of_sizes_small'"), "{origin}");
    let inner = err.inner().to_string();
    assert!(
        inner.contains("unknown variant") && inner.contains("gajillion"),
        "{inner}"
    );

    let mut env = Environment::from_iter("", [("map_of_sizes:json", r#"{ "small": 3 }"#)]);
    env.coerce_json().unwrap();
    let err = testing::test::<ComposedConfig>(env).unwrap_err();
    assert_eq!(err.len(), 1);
    let err = err.first();
    assert_eq!(err.path(), "map_of_sizes.small");
    let origin = err.origin().to_string();
    assert!(
        origin.ends_with("-> parsed JSON string -> path 'small'"),
        "{origin}"
    );
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid type"), "{inner}");
}

#[test]
fn reading_secrets() {
    let schema = ConfigSchema::new(&SecretConfig::DESCRIPTION, "");
    let env = Environment::from_iter("APP_", [("APP_KEY", "super_secret")]);
    let mut repo = ConfigRepository::new(&schema).with(env);

    assert_matches!(
        &repo.merged().get(Pointer("key")).unwrap().inner,
        Value::String(StrValue::Secret(_))
    );
    let config: SecretConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.key.expose_secret(), "super_secret");
    assert!(config.opt.is_none());

    let overrides = config!(
        "key": "override_secret",
        "opt": "opt_secret",
        "path": "/super/secret/path",
        "int": "123",
        "seq": "1,2,3",
    );
    repo = repo.with(overrides);

    assert_matches!(
        &repo.merged().get(Pointer("key")).unwrap().inner,
        Value::String(StrValue::Secret(_))
    );
    assert_matches!(
        &repo.merged().get(Pointer("opt")).unwrap().inner,
        Value::String(StrValue::Secret(_))
    );
    assert_matches!(
        &repo.merged().get(Pointer("path")).unwrap().inner,
        Value::String(StrValue::Secret(_))
    );
    assert_matches!(
        &repo.merged().get(Pointer("int")).unwrap().inner,
        Value::String(StrValue::Secret(_))
    );
    assert_matches!(
        &repo.merged().get(Pointer("seq")).unwrap().inner,
        Value::String(StrValue::Secret(_))
    );
    let config: SecretConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.key.expose_secret(), "override_secret");
    assert_eq!(config.opt.unwrap().expose_secret(), "opt_secret");
    assert_eq!(config.path.unwrap().as_os_str(), "/super/secret/path");
    assert_eq!(config.int, 123);
    assert_eq!(config.seq, [1, 2, 3]);

    let debug_str = format!("{:?}", repo.merged());
    assert!(!debug_str.contains("override_secret"), "{debug_str}");
    assert!(!debug_str.contains("opt_secret"), "{debug_str}");
}

#[test]
fn aliasing_for_flattened_config() {
    let mut schema = ConfigSchema::default();
    schema
        .insert(&AliasedConfig::DESCRIPTION, "test")
        .unwrap()
        .push_alias("alias")
        .unwrap();

    let json = config!("alias.int": 123, "alias.str": "!!");
    let mut repo = ConfigRepository::new(&schema).with(json);
    let config: AliasedConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.int, 123);
    assert_eq!(config.flat.str, "!!");

    let mixed_json = config!("test.int": 321, "alias.str": "??");
    repo = repo.with(mixed_json);
    let config: AliasedConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.int, 321);
    assert_eq!(config.flat.str, "??");

    let env = Environment::from_iter("", [("ALIAS_INT", "777"), ("ALIAS_STR", "!")]);
    repo = repo.with(env);
    let config: AliasedConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.int, 777);
    assert_eq!(config.flat.str, "!");
}

#[test]
fn aliasing_for_nested_config() {
    let mut schema = ConfigSchema::default();
    schema
        .insert(&AliasedConfig::DESCRIPTION, "test")
        .unwrap()
        .push_alias("alias")
        .unwrap();

    let json = config!("int": 123, "nested.str": "!!");
    let mut repo = ConfigRepository::new(&schema).with(Prefixed::new(json, "alias"));
    let config: AliasedConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.int, 123);
    assert_eq!(config.nested.str, "!!");

    let mixed_json = config!("test.int": 321, "alias.nest.str": "??");
    repo = repo.with(mixed_json);
    let config: AliasedConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.int, 321);
    assert_eq!(config.nested.str, "??");

    let env = Environment::from_iter("", [("ALIAS_INT", "777"), ("ALIAS_NEST_STRING", "!")]);
    repo = repo.with(env);
    let config: AliasedConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.int, 777);
    assert_eq!(config.nested.str, "!");
}

#[test]
fn reading_fallbacks() {
    let schema = ConfigSchema::new(&ConfigWithFallbacks::DESCRIPTION, "test");
    let repo = ConfigRepository::new(&schema);
    assert!(repo.sources().is_empty());
    let config: ConfigWithFallbacks = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.int, 42);
    assert!(config.str.is_none());

    let guard = MockEnvGuard::default();
    guard.set_env("SMART_CONFIG_INT".into(), "23".into());
    guard.set_env("SMART_CONFIG_STR".into(), "correct horse".into());
    let repo = ConfigRepository::new(&schema);
    assert_eq!(repo.sources().len(), 1);
    assert_matches!(repo.sources()[0].origin.as_ref(), ValueOrigin::Fallbacks);
    assert_eq!(repo.sources()[0].param_count, 2);
    drop(guard);

    assert_matches!(
        &repo.merged().get(Pointer("test.int")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "23"
    );
    assert_matches!(
        &repo.merged().get(Pointer("test.str")).unwrap().inner,
        Value::String(StrValue::Secret(_))
    );

    let config: ConfigWithFallbacks = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.int, 23);
    assert_eq!(config.str.unwrap().expose_secret(), "correct horse");

    // Mock env vars are read in `test::*` methods as well
    let mut tester = testing::Tester::default();
    tester
        .set_env("SMART_CONFIG_INT", "23")
        .set_env("SMART_CONFIG_STR", "unset");
    let config: ConfigWithFallbacks = tester.test(config!()).unwrap();
    assert_eq!(config.int, 23);
    assert!(config.str.is_none());

    let config: ConfigWithFallbacks = tester.test(config!("int": 555)).unwrap();
    assert_eq!(config.int, 555);
    assert!(config.str.is_none());
}

#[test]
fn reading_env_vars_using_env_source() {
    let config: NestedConfig = testing::Tester::default()
        .set_env("APP_RENAMED", "FIRST")
        .set_env("APP_OTHER_INT", "23")
        .coerce_variant_names()
        .test(Environment::prefixed("APP_"))
        .unwrap();
    assert_eq!(config.simple_enum, SimpleEnum::First);
    assert_eq!(config.other_int, 23);
}

#[test]
fn config_validations() {
    let json = config!("len": 4, "secret": "test");
    let config: ConfigWithValidations = testing::test(json).unwrap();
    assert_eq!(config.len, 4);
    assert_eq!(config.secret.expose_secret(), "test");

    let json = config!("len": 3, "secret": "test");
    let err = testing::test::<ConfigWithValidations>(json).unwrap_err();
    assert_eq!(err.len(), 1, "{err:?}");
    let err = err.first();
    assert_eq!(err.path(), "");
    assert_eq!(
        err.config().ty.id(),
        any::TypeId::of::<ConfigWithValidations>()
    );
    assert!(err.param().is_none());
    let inner = err.inner().to_string();
    assert!(
        inner.contains("`len` doesn't correspond to `secret`"),
        "{inner}"
    );
    assert_eq!(err.validation(), Some("`len` must match `secret` length"));

    let json = config!("len": 2_000, "secret": "!".repeat(2_000));
    let err = testing::test::<ConfigWithValidations>(json).unwrap_err();
    assert_eq!(err.len(), 1, "{err:?}");
    let err = err.first();
    assert_eq!(err.path(), "len");
    assert_eq!(err.param().unwrap().name, "len");
    assert_eq!(err.validation(), Some("must be in range ..1000"));
    let inner = err.inner().to_string();
    assert!(inner.contains("expected value in range ..1000"), "{inner}");

    let json = config!("len": 4, "secret": "test", "numbers": [] as [u32; 0]);
    let err = testing::test::<ConfigWithValidations>(json).unwrap_err();
    assert_eq!(err.len(), 1, "{err:?}");
    let err = err.first();
    assert_eq!(err.path(), "numbers");
    assert_eq!(err.param().unwrap().name, "numbers");
    let inner = err.inner().to_string();
    assert!(inner.contains("value is empty"), "{inner}");
}

#[test]
fn multiple_validation_failures() {
    let json = config!("len": 1_666, "secret": "!");
    let err = testing::test::<ConfigWithValidations>(json).unwrap_err();
    assert_eq!(err.len(), 2, "{err:?}");

    let validations: HashSet<_> = err
        .iter()
        .map(|err| err.validation.as_deref().unwrap())
        .collect();
    assert_eq!(
        validations,
        HashSet::from(["must not be cursed", "must be in range ..1000"])
    );
}

#[test]
fn config_nested_validations() {
    let json = config!("nested.len": 4, "nested.secret": "test");
    let config: ConfigWithNestedValidations = testing::test(json).unwrap();
    assert_eq!(config.nested.len, 4);
    assert_eq!(config.nested.secret.expose_secret(), "test");

    let json = config!("nested.len": 3, "nested.secret": "test");
    let err = testing::test::<ConfigWithNestedValidations>(json).unwrap_err();
    assert_eq!(err.len(), 1, "{err:?}");
    let err = err.first();
    assert_eq!(err.path(), "nested");
    assert_eq!(
        err.config().ty.id(),
        any::TypeId::of::<ConfigWithValidations>()
    );
    assert!(err.param().is_none());
    let inner = err.inner().to_string();
    assert!(
        inner.contains("`len` doesn't correspond to `secret`"),
        "{inner}"
    );
}

#[test]
fn config_canonicalization() {
    let schema = ConfigSchema::new(&NestedConfig::DESCRIPTION, "");
    let mut repo = ConfigRepository::new(&schema);
    // There is a missing required param in the config, but it shouldn't be a fatal error.
    let json = repo.canonicalize(&SerializerOptions::default()).unwrap();
    assert!(json.is_empty(), "{json:?}");

    repo = repo.with(config!("renamed": "first"));
    let json = repo.canonicalize(&SerializerOptions::default()).unwrap();
    assert_eq!(
        serde_json::Value::from(json),
        serde_json::json!({
            "renamed": "first",
            "other_int": 42,
            "map": {},
        })
    );

    repo = repo.with(config!("renamed": "???"));
    let err = repo
        .canonicalize(&SerializerOptions::default())
        .unwrap_err();
    assert_eq!(err.len(), 1);
    assert_eq!(err.first().param().unwrap().name, "renamed");
}

#[test]
fn config_canonicalization_with_nesting() {
    let schema = ConfigSchema::new(&ConfigWithNesting::DESCRIPTION, "");
    let mut repo = ConfigRepository::new(&schema);
    let json = repo.canonicalize(&SerializerOptions::default()).unwrap();
    assert!(json.is_empty(), "{json:?}");

    // The required nested config param is still missing.
    repo = repo.with(config!("value": 42));
    let json = repo.canonicalize(&SerializerOptions::default()).unwrap();
    assert!(json.is_empty(), "{json:?}");

    repo = repo.with(config!("nest.renamed": "first_choice"));
    let json = repo
        .canonicalize(&SerializerOptions::diff_with_default())
        .unwrap();
    assert_eq!(
        serde_json::Value::from(json),
        serde_json::json!({
            "value": 42,
            "nested": {
                "renamed": "first",
            },
        })
    );
}

#[test]
fn coercing_serde_enum() {
    let json = config!("fields.string": "!", "fields.flag": false, "fields.set": [123]);
    let config: EnumConfig = testing::Tester::default()
        .coerce_serde_enums()
        .test(json)
        .unwrap();
    assert_eq!(
        config,
        EnumConfig::WithFields {
            string: Some("!".to_owned()),
            flag: false,
            set: HashSet::from([123]),
        }
    );

    let json = config!(
        "nested.renamed": "first_choice",
        "nested.map": serde_json::json!({ "call": 100 }),
    );
    let config: EnumConfig = testing::Tester::default()
        .coerce_serde_enums()
        .test(json)
        .unwrap();
    assert_eq!(
        config,
        EnumConfig::Nested(NestedConfig {
            simple_enum: SimpleEnum::First,
            other_int: 42,
            map: HashMap::from([("call".to_owned(), 100)]),
        })
    );
}

#[test]
fn coercing_serde_enum_with_aliased_field() {
    let json = config!("fields.str": "?", "fields.flag": false);
    let config: EnumConfig = testing::Tester::default()
        .coerce_serde_enums()
        .test(json)
        .unwrap();
    assert_matches!(
        config,
        EnumConfig::WithFields { string: Some(s), flag: false, .. } if s == "?"
    );
}

#[test]
fn origins_for_coerced_serde_enum() {
    let schema = ConfigSchema::new(&EnumConfig::DESCRIPTION, "");
    let mut repo = ConfigRepository::new(&schema);
    repo.deserializer_options().coerce_serde_enums = true;
    let json = config!("fields.string": "!", "fields.flag": false, "fields.set": [123]);
    repo = repo.with(json);

    let tag = repo.merged().get(Pointer("type")).unwrap();
    assert_matches!(&tag.inner, Value::String(StrValue::Plain(s)) if s == "WithFields");
    let tag_origin = tag.origin.to_string();
    assert!(
        tag_origin.ends_with("-> path 'fields' -> coercing serde enum"),
        "{tag_origin}"
    );

    let flag = repo.merged().get(Pointer("flag")).unwrap();
    assert_matches!(flag.inner, Value::Bool(false));
    let flag_origin = flag.origin.to_string();
    assert!(
        flag_origin.ends_with("-> path 'fields.flag'"),
        "{flag_origin}"
    );

    // The original field should be garbage-collected.
    assert!(repo.merged().get(Pointer("fields")).is_none());
}

#[test]
fn coercing_serde_enum_negative_cases() {
    // Tag field present; the variant field shouldn't be used.
    let json = config!("type": "Fields", "fields.string": "!");
    let config: EnumConfig = testing::Tester::default()
        .coerce_serde_enums()
        .test(json)
        .unwrap();
    assert_matches!(
        config,
        EnumConfig::WithFields {
            string: None,
            flag: true,
            ..
        }
    );

    // Multiple variant fields present
    let json = config!("fields.string": "!", "with_fields.flag": false);
    let err = testing::Tester::<EnumConfig>::default()
        .coerce_serde_enums()
        .test(json)
        .unwrap_err();
    assert_eq!(err.len(), 1);
    assert_eq!(err.first().path(), "type");

    // Variant field is not an object
    let json = config!("fields": "!");
    let err = testing::Tester::<EnumConfig>::default()
        .coerce_serde_enums()
        .test(json)
        .unwrap_err();
    assert_eq!(err.len(), 1);
    assert_eq!(err.first().path(), "type");
}

#[test]
fn coercing_nested_enum_config() {
    let json = config!("next.nested.renamed": "second");
    let config: RenamedEnumConfig = testing::Tester::default()
        .coerce_serde_enums()
        .test(json)
        .unwrap();
    assert_eq!(
        config,
        RenamedEnumConfig::V3(EnumConfig::Nested(NestedConfig {
            simple_enum: SimpleEnum::Second,
            other_int: 42,
            map: HashMap::new(),
        }))
    );
}

#[test]
fn coercing_aliased_enum_config() {
    #[derive(Debug, DescribeConfig, DeserializeConfig)]
    #[config(crate = crate)]
    struct ConfigWithNestedEnum {
        #[config(nest, alias = "value")]
        val: EnumConfig,
    }

    // Base case: no aliasing.
    let json = config!("val.with_fields.str": "what");
    let config: ConfigWithNestedEnum = testing::Tester::default()
        .coerce_serde_enums()
        .test(json)
        .unwrap();
    assert_matches!(
        &config.val,
        EnumConfig::WithFields { string: Some(s), .. } if s == "what"
    );

    let json = config!("value.with_fields.string": "what");
    let config: ConfigWithNestedEnum = testing::Tester::default()
        .coerce_serde_enums()
        .test(json)
        .unwrap();
    assert_matches!(
        &config.val,
        EnumConfig::WithFields { string: Some(s), .. } if s == "what"
    );

    // Some more aliases for variant and the enclosed field.
    let json = config!("value.fields.str": "what");
    let config: ConfigWithNestedEnum = testing::Tester::default()
        .coerce_serde_enums()
        .test(json)
        .unwrap();
    assert_matches!(
        &config.val,
        EnumConfig::WithFields { string: Some(s), .. } if s == "what"
    );
}

#[test]
fn working_with_embedded_params() {
    #[derive(Debug, DescribeConfig, DeserializeConfig)]
    #[config(crate = crate)]
    pub(crate) struct ConfigWithEmbeddedParams {
        size: ByteSize,
        size_overrides: ByteSize,
    }

    let json = config!("size_mb": 10, "size_overrides": "20 MB");
    let config: ConfigWithEmbeddedParams = testing::test_complete(json).unwrap();
    assert_eq!(config.size, 10 * SizeUnit::MiB);
    assert_eq!(config.size_overrides, 20 * SizeUnit::MiB);

    let json = config!("size.mb": 10, "size_overrides": "20 MB");
    let config: ConfigWithEmbeddedParams = testing::test_complete(json).unwrap();
    assert_eq!(config.size, 10 * SizeUnit::MiB);
    assert_eq!(config.size_overrides, 20 * SizeUnit::MiB);

    let json = config!("size_mb": 10, "size_overrides_mb": 20);
    let config: ConfigWithEmbeddedParams = testing::test_complete(json).unwrap();
    assert_eq!(config.size, 10 * SizeUnit::MiB);
    assert_eq!(config.size_overrides, 20 * SizeUnit::MiB);
}

#[test]
fn parsing_nulls_from_env() {
    // It's always possible to use JSON coercion.
    let mut env = Environment::from_iter("", [("URL__JSON", "null"), ("FLOAT__JSON", "null")]);
    env.coerce_json().unwrap();
    let config: DefaultingConfig = testing::test(env).unwrap();
    assert_eq!(
        config,
        DefaultingConfig {
            url: None,
            float: None,
            ..DefaultingConfig::default()
        }
    );

    let env = Environment::from_iter("", [("URL", ""), ("FLOAT", "null")]);
    let config: DefaultingConfig = testing::test(env).unwrap();
    assert_eq!(
        config,
        DefaultingConfig {
            url: None,
            float: None,
            ..DefaultingConfig::default()
        }
    );
}

#[test]
fn parsing_with_custom_filter() {
    #[derive(Debug, DescribeConfig, DeserializeConfig)]
    #[config(crate = crate)]
    struct FilteringConfig {
        #[config(deserialize_if(
            |s: &String| !s.is_empty() && s != "unset",
            "not empty or 'unset'"
        ))]
        url: Option<String>,
    }

    let mut env = Environment::from_iter("", [("URL__JSON", "null")]);
    env.coerce_json().unwrap();
    let config: FilteringConfig = testing::test(env).unwrap();
    assert_eq!(config.url, None);

    let mut env = Environment::from_iter("", [("URL", "")]);
    env.coerce_json().unwrap();
    let config: FilteringConfig = testing::test(env).unwrap();
    assert_eq!(config.url, None);

    let mut env = Environment::from_iter("", [("URL", "unset")]);
    env.coerce_json().unwrap();
    let config: FilteringConfig = testing::test(env).unwrap();
    assert_eq!(config.url, None);

    let mut env = Environment::from_iter("", [("URL", "null")]);
    env.coerce_json().unwrap();
    let config: FilteringConfig = testing::test(env).unwrap();
    assert_eq!(config.url.unwrap(), "null");
}

#[test]
fn deserializing_optional_config() {
    let json = config!(
        "nested.renamed": "first",
        "renamed": "second",
        "nested_opt.other_int": 23,
    );
    let config: CompoundConfig = testing::test(json.clone()).unwrap();
    // There's no required `nested_opt.renamed` param, but otherwise, the config is valid.
    assert!(config.nested_opt.is_none(), "{config:?}");

    // Same with a repo.
    let schema = ConfigSchema::new(&CompoundConfig::DESCRIPTION, "");
    let repo = ConfigRepository::new(&schema).with(json);
    let config: Option<NestedConfig> = repo.get("nested_opt").unwrap().parse_opt().unwrap();
    assert!(config.is_none());

    let json = config!(
        "nested.renamed": "first",
        "renamed": "second",
        "nested_opt.other_int": "??", // << invalid type
    );
    let err = testing::test::<CompoundConfig>(json.clone()).unwrap_err();
    // Both missing param and the invalid type errors should be present
    assert_eq!(err.len(), 2);

    // Same with a repo.
    let repo = repo.with(json);
    let err = repo
        .get::<NestedConfig>("nested_opt")
        .unwrap()
        .parse_opt()
        .unwrap_err();
    assert_eq!(err.len(), 2);
}
