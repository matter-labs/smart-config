use std::collections::{HashMap, HashSet};

use assert_matches::assert_matches;
use serde::Deserialize;

use super::*;
use crate::{
    metadata::DescribeConfig,
    schema::Alias,
    testonly::{NestedConfig, SimpleEnum},
};

#[derive(Debug, Deserialize)]
struct TestParam {
    int: u64,
    bool: bool,
    string: String,
    optional: Option<i64>,
    array: Vec<u32>,
    repeated: HashSet<SimpleEnum>,
}

fn wrap_into_value(env: Environment) -> WithOrigin {
    let ConfigContents::KeyValue(map) = env.into_contents() else {
        unreachable!();
    };
    let map = map.into_iter().map(|(key, value)| {
        (
            key,
            WithOrigin {
                inner: Value::String(value.inner),
                origin: value.origin,
            },
        )
    });

    WithOrigin {
        inner: Value::Object(map.collect()),
        origin: Arc::default(),
    }
}

#[test]
fn parsing() {
    let env = Environment::from_iter(
        "",
        [
            ("int", "1"),
            ("bool", "true"),
            ("string", "??"),
            ("array", "1,2,3"),
            ("renamed", "first"),
            ("repeated", "second,first"),
        ],
    );
    let env = wrap_into_value(env);

    let config = TestParam::deserialize(ValueDeserializer::new(&env, String::new())).unwrap();
    assert_eq!(config.int, 1);
    assert_eq!(config.optional, None);
    assert!(config.bool);
    assert_eq!(config.string, "??");
    assert_eq!(config.array, [1, 2, 3]);
    assert_eq!(
        config.repeated,
        HashSet::from([SimpleEnum::First, SimpleEnum::Second])
    );
}

/*
#[test]
fn parsing_enum_config() {
    let env = Environment::from_iter("", [("type", "First")]);
    let env = wrap_into_value(env);
    let config = EnumConfig::deserialize(ValueDeserializer::new(&env)).unwrap();
    assert_eq!(config, EnumConfig::First);

    let env = Environment::from_iter("", [("type", "Nested"), ("renamed", "second")]);
    let env = wrap_into_value(env);
    let config = EnumConfig::deserialize(ValueDeserializer::new(&env)).unwrap();
    assert_eq!(
        config,
        EnumConfig::Nested(NestedConfig {
            simple_enum: SimpleEnum::Second,
            other_int: 42,
            map: HashMap::new(),
        })
    );

    let env = Environment::from_iter("", [("type", "WithFields")]);
    let env = wrap_into_value(env);
    let config = EnumConfig::deserialize(ValueDeserializer::new(&env)).unwrap();
    assert_eq!(
        config,
        EnumConfig::WithFields {
            string: None,
            flag: false,
            set: HashSet::new(),
        }
    );

    let env = Environment::from_iter(
        "",
        [("type", "Fields"), ("renamed", "second"), ("string", "???")],
    );
    let env = wrap_into_value(env);
    let config = EnumConfig::deserialize(ValueDeserializer::new(&env)).unwrap();
    assert_eq!(
        config,
        EnumConfig::WithFields {
            string: Some("???".to_owned()),
            flag: false,
            set: HashSet::new(),
        }
    );
}

#[test]
fn parsing_enum_config_with_schema() {
    let schema = ConfigSchema::default().insert::<EnumConfig>("");

    let json = config!(
        "type": "Nested",
        "renamed": "second",
        "map.first": 1,
        "map.second": 2,
    );
    let repo = ConfigRepository::new(&schema).with(json);
    let config: EnumConfig = repo.parse().unwrap();
    assert_eq!(
        config,
        EnumConfig::Nested(NestedConfig {
            simple_enum: SimpleEnum::Second,
            other_int: 42,
            map: HashMap::from([("first".to_owned(), 1), ("second".to_owned(), 2)]),
        })
    );

    let json = config!(
        "type": "Fields",
        "string": "???",
        "flag": true,
        "set": [42, 23],
    );
    let repo = ConfigRepository::new(&schema).with(json);
    let config: EnumConfig = repo.parse().unwrap();
    assert_eq!(
        config,
        EnumConfig::WithFields {
            string: Some("???".to_owned()),
            flag: true,
            set: HashSet::from([42, 23]),
        }
    );

    let env = Environment::from_iter(
        "",
        [
            ("type", "Fields"),
            ("renamed", "second"),
            ("string", "???"),
            ("flag", "true"),
        ],
    );
    let repo = ConfigRepository::new(&schema).with(env);
    assert_eq!(
        repo.merged().get(Pointer("flag")).unwrap().inner,
        Value::Bool(true)
    );

    let config: EnumConfig = repo.parse().unwrap();
    assert_eq!(
        config,
        EnumConfig::WithFields {
            string: Some("???".to_owned()),
            flag: true,
            set: HashSet::new(),
        }
    );
}
*/

#[test]
fn parsing_errors() {
    let env = Environment::from_iter("", [("renamed", "first"), ("other_int", "what")]);
    let err = NestedConfig::deserialize_config(ValueDeserializer::new(
        &wrap_into_value(env),
        String::new(),
    ))
    .unwrap_err();

    assert!(err.inner.to_string().contains("u32 value 'what'"), "{err}");
    assert_matches!(
        err.origin.as_ref().unwrap().as_ref(),
        ValueOrigin::EnvVar(name) if name == "other_int"
    );
}

#[derive(Debug, DescribeConfig)]
#[config(crate = crate)]
struct ConfigWithNesting {
    value: u32,
    #[config(default)]
    not_merged: String,
    #[config(nest)]
    nested: NestedConfig,
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

    let schema = ConfigSchema::default().insert::<ConfigWithNesting>("");
    let map = ConfigRepository::new(&schema).with(env).merged;

    assert_eq!(
        map.get(Pointer("value")).unwrap().inner,
        Value::Number(123_u64.into())
    );
    assert_eq!(
        map.get(Pointer("nested.renamed")).unwrap().inner,
        Value::String("first".to_owned())
    );
    assert_eq!(
        map.get(Pointer("nested.other_int")).unwrap().inner,
        Value::Number(321_u64.into())
    );

    let config =
        ConfigWithNesting::deserialize_config(ValueDeserializer::new(&map, String::new())).unwrap();
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

    let alias = Alias::prefix("deprecated").exclude(|name| name == "not_merged");
    let mut schema = ConfigSchema::default().insert_aliased::<ConfigWithNesting>("", [alias]);
    schema
        .single_mut::<NestedConfig>()
        .unwrap()
        .push_alias(Alias::prefix("deprecated"));

    let config: ConfigWithNesting = ConfigRepository::new(&schema).with(json).parse().unwrap();
    assert_eq!(config.value, 4);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 42);

    let json = config!(
        "value": 123,
        "deprecated.value": 4,
        "nested.renamed": "first",
        "deprecated.other_int": 321,
        "deprecated.not_merged": "!",
    );

    let config: ConfigWithNesting = ConfigRepository::new(&schema).with(json).parse().unwrap();
    assert_eq!(config.value, 123);
    assert_eq!(config.not_merged, "");
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 321);
}

#[test]
fn merging_config_parts_with_env() {
    let env = Environment::from_iter("", [("deprecated_value", "4"), ("nested_renamed", "first")]);

    let alias = Alias::prefix("deprecated").exclude(|name| name == "not_merged");
    let mut schema = ConfigSchema::default().insert_aliased::<ConfigWithNesting>("", [alias]);
    schema
        .single_mut::<NestedConfig>()
        .unwrap()
        .push_alias(Alias::prefix("deprecated"));

    let config: ConfigWithNesting = ConfigRepository::new(&schema).with(env).parse().unwrap();
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
            ("deprecated_not_merged", "!"),
        ],
    );

    let config: ConfigWithNesting = ConfigRepository::new(&schema).with(env).parse().unwrap();
    assert_eq!(config.value, 123);
    assert_eq!(config.not_merged, "");
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
        ValueOrigin::Json { filename, .. } if filename.as_ref() == "base.json"
    );
    assert_eq!(merged["bool"].inner, Value::Bool(false));
    assert_matches!(
        merged["bool"].origin.as_ref(),
        ValueOrigin::Json { filename, .. } if filename.as_ref() == "overrides.json"
    );
    assert_matches!(
        &merged["array"].inner,
        Value::Array(items) if items.len() == 1
    );
    assert_matches!(
        merged["array"].origin.as_ref(),
        ValueOrigin::Json { filename, .. } if filename.as_ref() == "overrides.json"
    );

    assert_matches!(
        &merged["nested"].inner,
        Value::Object(items) if items.len() == 3
    );
    let nested_int = merged["nested"].get(Pointer("int")).unwrap();
    assert_eq!(nested_int.inner, Value::Number(123_u64.into()));
    assert_matches!(
        nested_int.origin.as_ref(),
        ValueOrigin::Json { filename, .. } if filename.as_ref() == "overrides.json"
    );

    let nested_str = merged["nested"].get(Pointer("string")).unwrap();
    assert_eq!(nested_str.inner, Value::String("???".into()));
    assert_matches!(
        nested_str.origin.as_ref(),
        ValueOrigin::Json { filename, .. } if filename.as_ref() == "base.json"
    );
}

#[test]
fn using_aliases_with_object_config() {
    let alias = Alias::prefix("deprecated");
    let schema = ConfigSchema::default().insert_aliased::<ConfigWithNesting>("test", [alias]);

    let json = config!(
        "value": 123, // Should not be used.
        "deprecated.value": 321,
        "test.nested.renamed": "first",
    );
    let repo = ConfigRepository::new(&schema).with(json);

    let config: ConfigWithNesting = repo.parse().unwrap();
    assert_eq!(config.value, 321);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 42);
}

#[test]
fn using_env_config_overrides() {
    let alias = Alias::prefix("deprecated");
    let schema = ConfigSchema::default().insert_aliased::<ConfigWithNesting>("test", [alias]);

    let base = config!(
        "value": 123, // Should not be used.
        "deprecated.value": 321,
        "test.nested.renamed": "first",
    );
    let mut repo = ConfigRepository::new(&schema).with(base);

    let config: ConfigWithNesting = repo.parse().unwrap();
    assert_eq!(config.value, 321);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 42);

    let env = Environment::from_iter(
        "",
        [
            ("DEPRECATED_VALUE", "777"), // should not be used (aliases have lower priorities)
            ("TEST_NESTED_RENAMED", "second"),
        ],
    );
    repo = repo.with(env);

    let enum_value = repo.merged().get(Pointer("test.nested.renamed")).unwrap();
    assert_eq!(enum_value.inner, Value::String("second".into()));
    assert_matches!(enum_value.origin.as_ref(), ValueOrigin::EnvVar(_));

    let config: ConfigWithNesting = repo.parse().unwrap();
    assert_eq!(config.value, 321);
    assert_eq!(config.nested.simple_enum, SimpleEnum::Second);

    let env = Environment::from_iter("", [("TEST_VALUE", "555")]);
    repo = repo.with(env);

    let int_value = repo.merged().get(Pointer("test.value")).unwrap();
    assert_eq!(int_value.inner, Value::Number(555_u64.into()));

    let config: ConfigWithNesting = repo.parse().unwrap();
    assert_eq!(config.value, 555);
    assert_eq!(config.nested.simple_enum, SimpleEnum::Second);
}
