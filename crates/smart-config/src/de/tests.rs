use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    time::Duration,
};

use assert_matches::assert_matches;
use serde::Deserialize;

use super::deserializer::ValueDeserializer;
use crate::{
    config,
    de::DeserializerOptions,
    metadata::SizeUnit,
    testonly::{
        test_deserialize, test_deserialize_missing, wrap_into_value, CompoundConfig,
        ConfigWithComplexTypes, ConfigWithNesting, DefaultingConfig, DefaultingEnumConfig,
        EnumConfig, NestedConfig, SimpleEnum, TestParam,
    },
    value::{Pointer, Value, ValueOrigin},
    ByteSize, DescribeConfig, Environment, ParseError,
};

#[test]
fn parsing_param() {
    let env = Environment::from_iter(
        "",
        [
            ("int", "1"),
            ("bool", "true"),
            ("string", "??"),
            ("array", "1,2,3"),
            ("repeated", "second,first"),
        ],
    );
    let env = wrap_into_value(env);

    let mut options = DeserializerOptions::default();
    let config = TestParam::deserialize(ValueDeserializer::new(&env, &options)).unwrap();
    assert_eq!(config.int, 1);
    assert_eq!(config.optional, None);
    assert!(config.bool);
    assert_eq!(config.string, "??");
    assert_eq!(config.array, [1, 2, 3]);
    assert_eq!(
        config.repeated,
        HashSet::from([SimpleEnum::First, SimpleEnum::Second])
    );

    let env = Environment::from_iter(
        "",
        [
            ("int", "42"),
            ("string", "!!"),
            ("repeated", "FIRST,SECOND"),
        ],
    );
    let env = wrap_into_value(env);

    let err = TestParam::deserialize(ValueDeserializer::new(&env, &options)).unwrap_err();
    let inner = err.inner.to_string();
    assert!(inner.contains("unknown variant"), "{inner}");
    assert_matches!(err.origin.as_ref(), ValueOrigin::EnvVar(name) if name == "repeated");

    options.coerce_shouting_variant_names = true;
    let config = TestParam::deserialize(ValueDeserializer::new(&env, &options)).unwrap();
    assert_eq!(config.int, 42);
    assert_eq!(config.optional, None);
    assert!(!config.bool);
    assert_eq!(config.string, "!!");
    assert_eq!(config.array, [] as [u32; 0]);
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

    let json = config!(
        "type": "Nested",
        "renamed": "first",
        "other_int": 123
    );
    let config: EnumConfig = test_deserialize(json.inner()).unwrap();
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

    let json = config!(
        "type": "WithFields",
        "renamed": "second",
        "string": "???",
        "flag": false,
        "set": [12]
    );
    let config: EnumConfig = test_deserialize(json.inner()).unwrap();
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
        "nested.other_int": 321,
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
            .filter_map(ParseError::param)
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

    assert!(err.inner().to_string().contains("invalid type"), "{err}");
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

#[test]
fn parsing_complex_types() {
    let json = config!("array": [4, 5]);
    let config: ConfigWithComplexTypes = test_deserialize(json.inner()).unwrap();
    assert_eq!(
        config,
        ConfigWithComplexTypes {
            float: 4.2,
            array: [NonZeroUsize::new(4).unwrap(), NonZeroUsize::new(5).unwrap()],
            choices: None,
            assumed: None,
            short_dur: Duration::from_millis(100),
            path: "./test".into(),
            memory_size_mb: Some(ByteSize::new(128, SizeUnit::MiB)),
        }
    );

    let json = config!(
        "array": [4, 5],
        "choices": ["first", "second"],
        "assumed": 24,
        "short_dur": 200,
        "path": "/mnt",
        "memory_size_mb": 64,
    );
    let config: ConfigWithComplexTypes = test_deserialize(json.inner()).unwrap();
    assert_eq!(
        config,
        ConfigWithComplexTypes {
            float: 4.2,
            array: [NonZeroUsize::new(4).unwrap(), NonZeroUsize::new(5).unwrap()],
            choices: Some(vec![SimpleEnum::First, SimpleEnum::Second]),
            assumed: Some(serde_json::json!(24)),
            short_dur: Duration::from_millis(200),
            path: "/mnt".into(),
            memory_size_mb: Some(ByteSize::new(64, SizeUnit::MiB)),
        }
    );

    let json = config!(
        "array": [5, 6],
        "float": -3,
        "choices": (),
        "assumed": (),
        "short_dur": 1000,
        "memory_size_mb": (),
    );
    let config: ConfigWithComplexTypes = test_deserialize(json.inner()).unwrap();
    assert_eq!(
        config,
        ConfigWithComplexTypes {
            float: -3.0,
            array: [NonZeroUsize::new(5).unwrap(), NonZeroUsize::new(6).unwrap()],
            choices: None,
            assumed: None,
            short_dur: Duration::from_secs(1),
            path: "./test".into(),
            memory_size_mb: None,
        }
    );
}

#[test]
fn parsing_complex_types_errors() {
    let json = config!("array": [1]);
    let errors = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid length"), "{inner}");
    assert_eq!(err.path(), "array");
    assert_matches!(err.origin(), ValueOrigin::Json { path, ..} if path == "array");

    let json = config!("array": [0, 1]);
    let errors = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(inner.contains("nonzero"), "{inner}");
    assert_eq!(err.path(), "array");
    assert_matches!(err.origin(), ValueOrigin::Json { path, ..} if path == "array.0");

    let json = config!("array": [2, 3], "float": "what");
    let errors = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(inner.contains("what"), "{inner}");
    assert_eq!(err.path(), "float");
    assert_matches!(err.origin(), ValueOrigin::Json { path, ..} if path == "float");

    let json = config!("array": [2, 3], "assumed": true);
    let errors = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(
        inner.contains("invalid type") && inner.contains("expected float"),
        "{inner}"
    );
    assert_eq!(err.path(), "assumed");
    assert_matches!(err.origin(), ValueOrigin::Json { path, ..} if path == "assumed");
}
