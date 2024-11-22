use std::collections::{HashMap, HashSet};

use assert_matches::assert_matches;
use serde::Deserialize;

use super::serde::ValueDeserializer;
use crate::{
    config,
    testonly::{
        test_deserialize, test_deserialize_missing, wrap_into_value, CompoundConfig,
        ConfigWithNesting, DefaultingConfig, DefaultingEnumConfig, EnumConfig, NestedConfig,
        SimpleEnum, TestParam,
    },
    value::{Pointer, Value, ValueOrigin},
    DescribeConfig, Environment,
};

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

    let config = TestParam::deserialize(ValueDeserializer::new(&env)).unwrap();
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

#[test]
fn parsing_enum_config() {
    let env = Environment::from_iter("", [("type", "first")]);
    let env = wrap_into_value(env);
    let config: EnumConfig = test_deserialize(&env).unwrap();
    assert_eq!(config, EnumConfig::First);

    let env = Environment::from_iter("", [("type", "Nested"), ("renamed", "second")]);
    let env = wrap_into_value(env);
    let config: EnumConfig = test_deserialize(&env).unwrap();
    assert_eq!(
        config,
        EnumConfig::Nested(NestedConfig {
            simple_enum: SimpleEnum::Second,
            other_int: 42,
            map: HashMap::new(),
        })
    );

    let env = Environment::from_iter(
        "",
        [
            ("type", "Nested"),
            ("renamed", "first"),
            ("other_int", "123"),
        ],
    );
    let env = wrap_into_value(env);
    let config: EnumConfig = test_deserialize(&env).unwrap();
    assert_eq!(
        config,
        EnumConfig::Nested(NestedConfig {
            simple_enum: SimpleEnum::First,
            other_int: 123,
            map: HashMap::new(),
        })
    );

    let env = Environment::from_iter("", [("type", "WithFields")]);
    let env = wrap_into_value(env);
    let config: EnumConfig = test_deserialize(&env).unwrap();
    assert_eq!(
        config,
        EnumConfig::WithFields {
            string: None,
            flag: true,
            set: HashSet::from([23, 42]),
        }
    );

    let env = Environment::from_iter(
        "",
        [
            ("type", "WithFields"),
            ("renamed", "second"),
            ("string", "???"),
            ("flag", "false"),
            ("set", "12"),
        ],
    );
    let env = wrap_into_value(env);
    let config: EnumConfig = test_deserialize(&env).unwrap();
    assert_eq!(
        config,
        EnumConfig::WithFields {
            string: Some("???".to_owned()),
            flag: false,
            set: HashSet::from([12]),
        }
    );
}

#[test]
fn parsing_enum_config_missing_tag() {
    let env = Environment::from_iter("", [("renamed", "second")]);
    let env = wrap_into_value(env);
    let errors = test_deserialize::<EnumConfig>(&env).unwrap_err();
    let err = errors.first();

    let inner = err.inner().to_string();
    assert!(inner.contains("missing field"), "{inner}");
    assert_eq!(err.path(), "type");
    assert_eq!(err.config().ty, EnumConfig::describe_config().ty);
    assert_eq!(err.param().unwrap().name, "type");
}

#[test]
fn parsing_enum_config_unknown_tag() {
    let env = Environment::from_iter("", [("type", "Unknown")]);
    let env = wrap_into_value(env);
    let errors = test_deserialize::<EnumConfig>(&env).unwrap_err();
    let err = errors.first();

    let inner = err.inner().to_string();
    assert!(inner.contains("unknown variant"), "{inner}");
    assert_eq!(err.path(), "type");
    assert_eq!(err.config().ty, EnumConfig::describe_config().ty);
    assert_eq!(err.param().unwrap().name, "type");
}

#[test]
fn parsing_compound_config() {
    let json = config!(
        "nested.renamed": "first",
        "renamed": "second",
        "other_int": 123,
    );
    let config: CompoundConfig = test_deserialize(json.inner()).unwrap();
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

    let json = config!(
        "nested.renamed": "first",
        "nested.other_int": "321",
        "default.renamed": "second",
        "default.map": HashMap::from([("foo", 3)]),
        "renamed": "second",
    );
    let config: CompoundConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(
        config.nested,
        NestedConfig {
            simple_enum: SimpleEnum::First,
            other_int: 321,
            map: HashMap::new(),
        }
    );
    assert_eq!(
        config.nested_default,
        NestedConfig {
            simple_enum: SimpleEnum::Second,
            other_int: 42,
            map: HashMap::from([("foo".to_owned(), 3)]),
        }
    );
}

#[test]
fn parsing_compound_config_missing_nested_value() {
    let json = config!(
        "nested.value": 1,
        "renamed": "second",
    );
    let errors = test_deserialize::<CompoundConfig>(json.inner()).unwrap_err();

    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(inner.contains("missing field"), "{inner}");
    assert_eq!(err.path(), "nested.renamed");
    assert_eq!(err.config().ty, NestedConfig::describe_config().ty);
    assert_eq!(err.param().unwrap().name, "renamed");
}

#[test]
fn parsing_compound_config_with_multiple_errors() {
    let json = config!("other_int": "what?");
    let errors = test_deserialize::<CompoundConfig>(json.inner()).unwrap_err();
    assert_eq!(errors.len(), 3, "{errors:#?}");
    assert_eq!(
        errors
            .iter()
            .filter_map(|err| err.param())
            .filter(|param| param.name == "renamed")
            .count(),
        2,
        "{errors:#?}"
    );
    assert!(
        errors
            .iter()
            .any(|err| err.inner().to_string().contains("what?") && err.path() == "other_int"),
        "{errors:#?}"
    );
}

#[test]
fn parsing_defaulting_config_from_missing_value() {
    let config: DefaultingConfig = test_deserialize_missing().unwrap();
    assert_eq!(config, DefaultingConfig::default());
}

#[test]
fn parsing_defaulting_config_with_null_override() {
    let json = config!("url": ());
    assert_eq!(json.inner().get(Pointer("url")).unwrap().inner, Value::Null);
    let config: DefaultingConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(
        config,
        DefaultingConfig {
            url: None,
            ..DefaultingConfig::default()
        }
    );
}

#[test]
fn parsing_defaulting_enum_config_from_missing_value() {
    let config: DefaultingEnumConfig = test_deserialize_missing().unwrap();
    assert_eq!(config, DefaultingEnumConfig::default());
}

#[test]
fn parsing_defaulting_enum_config_from_missing_tag() {
    let json = config!("int": 42);
    let config: DefaultingEnumConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(config, DefaultingEnumConfig::Second { int: 42 });

    let json = config!("kind": "First", "int": 42);
    let config: DefaultingEnumConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(config, DefaultingEnumConfig::First);

    let json = config!("kind": "Third", "int": 42);
    let errors = test_deserialize::<DefaultingEnumConfig>(json.inner()).unwrap_err();
    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(inner.contains("unknown variant"), "{inner}");
}

#[test]
fn type_mismatch_parsing_error() {
    let env = Environment::from_iter("", [("renamed", "first"), ("other_int", "what")]);
    let env = wrap_into_value(env);
    let errors = test_deserialize::<NestedConfig>(&env).unwrap_err();
    let err = errors.first();

    assert!(
        err.inner().to_string().contains("u32 value 'what'"),
        "{err}"
    );
    assert_matches!(
        err.origin(),
        ValueOrigin::EnvVar(name) if name == "other_int"
    );
    assert_eq!(err.config().ty, NestedConfig::describe_config().ty);
    assert_eq!(err.param().unwrap().name, "other_int");
}

#[test]
fn missing_parameter_parsing_error() {
    let env = Environment::from_iter("", [("other_int", "12")]);
    let env = wrap_into_value(env);
    let errors = test_deserialize::<NestedConfig>(&env).unwrap_err();

    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(inner.contains("missing field"), "{inner}");
    assert_eq!(err.config().ty, NestedConfig::describe_config().ty);
    assert_eq!(err.param().unwrap().name, "renamed");
}

#[test]
fn missing_nested_config_parsing_error() {
    let json = config!("value": 123);
    let errors = test_deserialize::<ConfigWithNesting>(json.inner()).unwrap_err();
    let err = errors.first();

    let inner = err.inner().to_string();
    assert!(inner.contains("missing field"), "{inner}");
    assert_eq!(err.path(), "nested.renamed");
    assert_eq!(err.config().ty, NestedConfig::describe_config().ty);
    assert_eq!(err.param().unwrap().name, "renamed");
}
