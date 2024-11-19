use std::collections::HashSet;

use assert_matches::assert_matches;
use serde::Deserialize;

use super::*;
use crate::schema::Alias;

#[derive(Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SimpleEnum {
    First,
    Second,
}

#[derive(Debug, Deserialize, DescribeConfig)]
#[config(crate = crate)]
struct NestedConfig {
    #[serde(rename = "renamed")]
    simple_enum: SimpleEnum,
    #[serde(default = "NestedConfig::default_other_int")]
    other_int: u32,
}

impl NestedConfig {
    const fn default_other_int() -> u32 {
        42
    }
}

#[derive(Debug, Deserialize)]
struct TestConfig {
    int: u64,
    bool: bool,
    string: String,
    optional: Option<i64>,
    array: Vec<u32>,
    repeated: HashSet<SimpleEnum>,
    #[serde(flatten)]
    nested: NestedConfig,
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
            ("int".to_owned(), "1".to_owned()),
            ("bool".to_owned(), "true".to_owned()),
            ("string".to_owned(), "??".to_owned()),
            ("array".to_owned(), "1,2,3".to_owned()),
            ("renamed".to_owned(), "first".to_owned()),
            ("repeated".to_owned(), "second,first".to_owned()),
        ],
    );
    let env = wrap_into_value(env);

    let config = TestConfig::deserialize(ValueDeserializer::new(&env)).unwrap();
    assert_eq!(config.int, 1);
    assert_eq!(config.optional, None);
    assert!(config.bool);
    assert_eq!(config.string, "??");
    assert_eq!(config.array, [1, 2, 3]);
    assert_eq!(
        config.repeated,
        HashSet::from([SimpleEnum::First, SimpleEnum::Second])
    );
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 42);
}

#[test]
fn parsing_errors() {
    let env = Environment::from_iter(
        "",
        [
            ("renamed".to_owned(), "first".to_owned()),
            ("other_int".to_owned(), "what".to_owned()),
        ],
    );
    let err = NestedConfig::deserialize(ValueDeserializer::new(&wrap_into_value(env))).unwrap_err();

    assert!(err.inner.to_string().contains("u32 value 'what'"), "{err}");
    assert_matches!(
        err.origin.as_ref().unwrap().as_ref(),
        ValueOrigin::EnvVar(name) if name == "other_int"
    );
}

#[derive(Debug, Deserialize, DescribeConfig)]
#[config(crate = crate)]
struct ConfigWithNesting {
    value: u32,
    #[serde(default)]
    not_merged: String,
    #[config(nested)]
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
    let map = ConfigRepository::from(env).parser(&schema).unwrap().map;

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

    let config = ConfigWithNesting::deserialize(ValueDeserializer::new(&map)).unwrap();
    assert_eq!(config.value, 123);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 321);
}

#[test]
fn merging_config_parts() {
    let env = Environment::from_iter(
        "",
        [
            ("deprecated_value".to_owned(), "4".to_owned()),
            ("nested_renamed".to_owned(), "first".to_owned()),
        ],
    );

    let alias = Alias::prefix("deprecated").exclude(|name| name == "not_merged");
    let mut schema = ConfigSchema::default().insert_aliased::<ConfigWithNesting>("", [alias]);
    schema
        .single_mut::<NestedConfig>()
        .unwrap()
        .push_alias(Alias::prefix("deprecated"));

    let config: ConfigWithNesting = ConfigRepository::from(env)
        .parser(&schema)
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(config.value, 4);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 42);

    let env = Environment::from_iter(
        "",
        [
            ("value".to_owned(), "123".to_owned()),
            ("deprecated_value".to_owned(), "4".to_owned()),
            ("nested_renamed".to_owned(), "first".to_owned()),
            ("deprecated_other_int".to_owned(), "321".to_owned()),
            ("deprecated_not_merged".to_owned(), "!".to_owned()),
        ],
    );

    let config: ConfigWithNesting = ConfigRepository::from(env)
        .parser(&schema)
        .unwrap()
        .parse()
        .unwrap();
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

    let repo = ConfigRepository::default().with(base).with(overrides);

    assert_eq!(repo.object["int"].inner, Value::Number(123_u64.into()));
    assert_matches!(
        repo.object["int"].origin.as_ref(),
        ValueOrigin::Json { filename, .. } if filename.as_ref() == "base.json"
    );
    assert_eq!(repo.object["bool"].inner, Value::Bool(false));
    assert_matches!(
        repo.object["bool"].origin.as_ref(),
        ValueOrigin::Json { filename, .. } if filename.as_ref() == "overrides.json"
    );
    assert_matches!(
        &repo.object["array"].inner,
        Value::Array(items) if items.len() == 1
    );
    assert_matches!(
        repo.object["array"].origin.as_ref(),
        ValueOrigin::Json { filename, .. } if filename.as_ref() == "overrides.json"
    );

    assert_matches!(
        &repo.object["nested"].inner,
        Value::Object(items) if items.len() == 3
    );
    let nested_int = repo.object["nested"].get(Pointer("int")).unwrap();
    assert_eq!(nested_int.inner, Value::Number(123_u64.into()));
    assert_matches!(
        nested_int.origin.as_ref(),
        ValueOrigin::Json { filename, .. } if filename.as_ref() == "overrides.json"
    );

    let nested_str = repo.object["nested"].get(Pointer("string")).unwrap();
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

    let json = serde_json::json!({
        "value": 123, // should not be parsed
        "deprecated": {
            "value": 321,
        },
        "test": {
            "nested": { "renamed": "first" },
        }
    });
    let serde_json::Value::Object(json) = json else {
        unreachable!();
    };
    let base = Json::new("base.json", json);
    let repo = ConfigRepository::from(base);

    let config: ConfigWithNesting = repo.clone().parser(&schema).unwrap().parse().unwrap();
    assert_eq!(config.value, 321);
    assert_eq!(config.nested.simple_enum, SimpleEnum::First);
    assert_eq!(config.nested.other_int, 42);
}

#[test]
fn using_env_config_overrides() {
    let alias = Alias::prefix("deprecated");
    let schema = ConfigSchema::default().insert_aliased::<ConfigWithNesting>("test", [alias]);

    let json = serde_json::json!({
        "value": 123, // should not be parsed
        "test": {
            "value": 321,
            "nested": { "renamed": "first" },
        }
    });
    let serde_json::Value::Object(json) = json else {
        unreachable!();
    };
    let base = Json::new("base.json", json);
    let mut repo = ConfigRepository::from(base);

    let config: ConfigWithNesting = repo.clone().parser(&schema).unwrap().parse().unwrap();
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

    let parser = repo.clone().parser(&schema).unwrap();
    let enum_value = parser.map().get(Pointer("test.nested.renamed")).unwrap();
    assert_eq!(enum_value.inner, Value::String("second".into()));
    assert_matches!(enum_value.origin.as_ref(), ValueOrigin::EnvVar(_));

    let config: ConfigWithNesting = parser.parse().unwrap();
    assert_eq!(config.value, 321);
    assert_eq!(config.nested.simple_enum, SimpleEnum::Second);

    let env = Environment::from_iter("", [("TEST_VALUE", "555")]);
    repo = repo.with(env);

    let parser = repo.parser(&schema).unwrap();
    let int_value = parser.map().get(Pointer("test.value")).unwrap();
    assert_eq!(int_value.inner, Value::Number(555_u64.into()));

    let config: ConfigWithNesting = parser.parse().unwrap();
    assert_eq!(config.value, 555);
    assert_eq!(config.nested.simple_enum, SimpleEnum::Second);
}
