use std::collections::{HashMap, HashSet};

use assert_matches::assert_matches;

use super::*;
use crate::{
    testing,
    testonly::{
        extract_env_var_name, extract_json_name, test_deserialize, CompoundConfig,
        ConfigWithNesting, DefaultingConfig, EnumConfig, NestedConfig, SimpleEnum,
        ValueCoercingConfig,
    },
};

#[test]
fn parsing_enum_config_with_schema() {
    let mut schema = ConfigSchema::default();
    schema.insert::<EnumConfig>("").unwrap();

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
    let json = config!(
        "type": "Nested",
        "renamed": "FIRST",
        "map.first": 1,
    );
    let mut repo = ConfigRepository::new(&schema).with(json);
    let errors = repo.single::<EnumConfig>().unwrap().parse().unwrap_err();
    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(inner.contains("unknown variant"), "{inner}");
    assert_eq!(err.path(), "renamed");

    repo.deserializer_options().coerce_shouting_variant_names = true;
    let config: EnumConfig = repo.single().unwrap().parse().unwrap();
    assert_eq!(
        config,
        EnumConfig::Nested(NestedConfig {
            simple_enum: SimpleEnum::First,
            other_int: 42,
            map: HashMap::from([("first".to_owned(), 1)]),
        })
    );

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
    assert_eq!(
        repo.merged().get(Pointer("flag")).unwrap().inner,
        Value::String("false".into())
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
fn parsing_defaulting_config_from_missing_value_with_schema() {
    let mut schema = ConfigSchema::default();
    schema.insert::<DefaultingConfig>("test").unwrap();
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

    let mut schema = ConfigSchema::default();
    schema.insert::<CompoundConfig>("").unwrap();
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

    let mut schema = ConfigSchema::default();
    schema.insert::<ConfigWithNesting>("").unwrap();
    let map = ConfigRepository::new(&schema).with(env).merged;

    assert_eq!(
        map.get(Pointer("value")).unwrap().inner,
        Value::String("123".into())
    );
    assert_eq!(
        map.get(Pointer("nested.renamed")).unwrap().inner,
        Value::String("first".to_owned())
    );
    assert_eq!(
        map.get(Pointer("nested.other_int")).unwrap().inner,
        Value::String("321".into())
    );

    let config: ConfigWithNesting = test_deserialize(&map).unwrap();
    assert_eq!(config.value, 123);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 321);
}

#[test]
fn merging_config_parts() {
    let json = config!(
        "deprecated.value": 4,
        "nested.renamed": "first",
    );

    let mut schema = ConfigSchema::default();
    schema
        .insert::<ConfigWithNesting>("")
        .unwrap()
        .push_alias("deprecated")
        .unwrap();
    schema
        .single_mut::<NestedConfig>()
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
fn merging_config_parts_with_env() {
    let env = Environment::from_iter("", [("deprecated_value", "4"), ("nested_renamed", "first")]);

    let mut schema = ConfigSchema::default();
    schema
        .insert::<ConfigWithNesting>("")
        .unwrap()
        .push_alias("deprecated")
        .unwrap();
    schema
        .single_mut::<NestedConfig>()
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
        "int": 123,
        "bool": true,
        "array": [42, 23],
        "nested": {
            "int": 321,
            "string": "???",
        },
    });
    let serde_json::Value::Object(json) = json else {
        unreachable!();
    };
    let base = Json::new("base.json", json);

    let json = serde_json::json!({
        "bool": false,
        "array": [23],
        "nested": {
            "int": 123,
            "bool": true,
        },
    });
    let serde_json::Value::Object(json) = json else {
        unreachable!();
    };
    let overrides = Json::new("overrides.json", json);

    let empty_schema = ConfigSchema::default();
    let repo = ConfigRepository::new(&empty_schema)
        .with(base)
        .with(overrides);
    let Value::Object(merged) = &repo.merged().inner else {
        panic!("unexpected merged value");
    };

    assert_eq!(merged["int"].inner, Value::Number(123_u64.into()));
    assert_matches!(
        merged["int"].origin.as_ref(),
        ValueOrigin::Path { source, .. } if extract_json_name(source) == "base.json"
    );
    assert_eq!(merged["bool"].inner, Value::Bool(false));
    assert_matches!(
        merged["bool"].origin.as_ref(),
        ValueOrigin::Path { source, .. } if extract_json_name(source) == "overrides.json"
    );
    assert_matches!(
        &merged["array"].inner,
        Value::Array(items) if items.len() == 1
    );
    assert_matches!(
        merged["array"].origin.as_ref(),
        ValueOrigin::Path { source, .. } if extract_json_name(source) == "overrides.json"
    );

    assert_matches!(
        &merged["nested"].inner,
        Value::Object(items) if items.len() == 3
    );
    let nested_int = merged["nested"].get(Pointer("int")).unwrap();
    assert_eq!(nested_int.inner, Value::Number(123_u64.into()));
    assert_matches!(
        nested_int.origin.as_ref(),
        ValueOrigin::Path { source, .. } if extract_json_name(source) == "overrides.json"
    );

    let nested_str = merged["nested"].get(Pointer("string")).unwrap();
    assert_eq!(nested_str.inner, Value::String("???".into()));
    assert_matches!(
        nested_str.origin.as_ref(),
        ValueOrigin::Path { source, .. } if extract_json_name(source) == "base.json"
    );
}

#[test]
fn using_aliases_with_object_config() {
    let mut schema = ConfigSchema::default();
    schema
        .insert::<ConfigWithNesting>("test")
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
        .insert::<ConfigWithNesting>("test")
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
    assert_eq!(enum_value.inner, Value::String("second".into()));
    extract_env_var_name(&enum_value.origin);

    let config: ConfigWithNesting = repo.single().unwrap().parse().unwrap();
    assert_eq!(config.value, 777);
    assert_eq!(config.nested.simple_enum, SimpleEnum::Second);

    let env = Environment::from_iter("", [("TEST_VALUE", "555")]);
    repo = repo.with(env);

    let int_value = repo.merged().get(Pointer("test.value")).unwrap();
    assert_eq!(int_value.inner, Value::String("555".into()));

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

    let env = Environment::from_iter(
        "",
        [
            (
                "PARAM",
                r#"{ "int": 3, "string": "!!", "repeated": ["second"] }"#,
            ),
            ("SET", "[2, 3]"),
        ],
    );
    let config: ValueCoercingConfig = testing::test(env).unwrap();
    assert_eq!(config.param.int, 3);
    assert_eq!(config.param.string, "!!");
    assert_eq!(config.param.repeated, HashSet::from([SimpleEnum::Second]));
    assert_eq!(config.set, HashSet::from([2, 3]));
}

#[test]
fn parsing_complex_param_errors() {
    let env = Environment::from_iter("", [("PARAM", r#"{ "int": "???" }"#)]);
    let err = testing::test::<ValueCoercingConfig>(env).unwrap_err();
    assert_eq!(err.len(), 1);
    let err = err.first();
    assert_eq!(err.path(), "param");
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid digit"), "{inner}");
    assert_eq!(
        err.origin().to_string(),
        "env variable 'PARAM' -> parsed JSON string -> path 'int'"
    );

    let env = Environment::from_iter(
        "APP_",
        [
            ("APP_PARAM", r#"{ "int": 42, "string": "!" }"#),
            ("APP_SET", "[1, false]"),
        ],
    );
    let err = testing::test::<ValueCoercingConfig>(env).unwrap_err();
    assert_eq!(err.len(), 1);
    let err = err.first();
    assert_eq!(err.path(), "set");
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid type"), "{inner}");
    assert_eq!(
        err.origin().to_string(),
        "env variable 'APP_SET' -> parsed JSON string -> path '1'"
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
    let mut schema = ConfigSchema::default();
    schema.insert::<ValueCoercingConfig>("").unwrap();
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
    let mut schema = ConfigSchema::default();
    schema.insert::<ValueCoercingConfig>("test.config").unwrap();
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
