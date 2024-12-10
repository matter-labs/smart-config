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
        extract_env_var_name, extract_json_name, test_deserialize, test_deserialize_missing,
        wrap_into_value, ComposedConfig, CompoundConfig, ConfigWithComplexTypes, ConfigWithNesting,
        DefaultingConfig, DefaultingEnumConfig, EnumConfig, MapOrString, NestedConfig,
        RenamedEnumConfig, SimpleEnum, TestParam,
    },
    value::{Pointer, Value, ValueOrigin},
    ByteSize, DescribeConfig, Environment, ParseError,
};

#[test]
fn parsing_param() {
    let json = config!(
        // Strings should be coerced to numbers / bools inside the deserializer
        "int": "1",
        "bool": "true",
        "string": "??",
        "array": [1, 2, 3],
        "repeated": ["second", "first"],
    );

    let mut options = DeserializerOptions::default();
    let deserializer = ValueDeserializer::new(json.inner(), &options);
    let config = TestParam::deserialize(deserializer).unwrap();
    assert_eq!(config.int, 1);
    assert_eq!(config.optional, None);
    assert!(config.bool);
    assert_eq!(config.string, "??");
    assert_eq!(config.array, [1, 2, 3]);
    assert_eq!(
        config.repeated,
        HashSet::from([SimpleEnum::First, SimpleEnum::Second])
    );

    let json = config!(
        "int": 42,
        "string": "!!",
        "repeated": ["FIRST", "SECOND"],
    );
    let deserializer = ValueDeserializer::new(json.inner(), &options);
    let err = TestParam::deserialize(deserializer).unwrap_err();
    let inner = err.inner.to_string();
    assert!(inner.contains("unknown variant"), "{inner}");
    let ValueOrigin::Path { path, source } = err.origin.as_ref() else {
        panic!("unexpected origin: {:?}", err.origin);
    };
    assert_eq!(path, "repeated.0");
    extract_json_name(source);

    options.coerce_shouting_variant_names = true;
    let deserializer = ValueDeserializer::new(json.inner(), &options);
    let config = TestParam::deserialize(deserializer).unwrap();
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
    assert_eq!(err.config().ty, EnumConfig::DESCRIPTION.ty);
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
    assert_eq!(err.config().ty, EnumConfig::DESCRIPTION.ty);
    assert_eq!(err.param().unwrap().name, "type");
}

#[test]
fn parsing_enum_config_with_renamed_variants() {
    let json = config!("version": "v0");
    let config: RenamedEnumConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(config, RenamedEnumConfig::V0);

    let json = config!("version": "v1", "int": 2);
    let config: RenamedEnumConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(config, RenamedEnumConfig::V1 { int: 2 });

    let json = config!("version": "previous", "int": 3);
    let config: RenamedEnumConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(config, RenamedEnumConfig::V1 { int: 3 });

    let json = config!("version": "v2", "str": "!!");
    let config: RenamedEnumConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(config, RenamedEnumConfig::V2 { str: "!!".into() });

    let json = config!("str": "??");
    let config: RenamedEnumConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(config, RenamedEnumConfig::V2 { str: "??".into() });
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
    assert_eq!(config.nested_opt, None);

    let json = config!(
        "nested.renamed": "first",
        "nested.other_int": 321,
        "default.renamed": "second",
        "default.map": HashMap::from([("foo", 3)]),
        "renamed": "second",
        "nested_opt.renamed": "second",
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
    assert_eq!(
        config.nested_opt.unwrap(),
        NestedConfig {
            simple_enum: SimpleEnum::Second,
            other_int: 42,
            map: HashMap::new(),
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
    assert_eq!(err.config().ty, NestedConfig::DESCRIPTION.ty);
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
    assert_matches!(json.inner().get(Pointer("url")).unwrap().inner, Value::Null);
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
    assert_eq!(extract_env_var_name(err.origin()), "other_int");
    assert_eq!(err.config().ty, NestedConfig::DESCRIPTION.ty);
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
    assert_eq!(err.config().ty, NestedConfig::DESCRIPTION.ty);
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
    assert_eq!(err.config().ty, NestedConfig::DESCRIPTION.ty);
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
            long_dur: Duration::from_secs(5),
            path: "./test".into(),
            memory_size_mb: Some(ByteSize::new(128, SizeUnit::MiB)),
            disk_size: None,
            paths: vec![],
            map_or_string: MapOrString::default(),
        }
    );

    let json = config!(
        "array": "5,4",
        "choices": ["first", "second"],
        "assumed": 24,
        "short_dur": 200,
        "long_dur": "5 min",
        "path": "/mnt",
        "paths": "/usr/bin:/usr/local/bin",
        "memory_size_mb": 64,
        "disk_size": "4 GB",
        "map_or_string": "test=1,other=2",
    );
    let config: ConfigWithComplexTypes = test_deserialize(json.inner()).unwrap();
    assert_eq!(
        config,
        ConfigWithComplexTypes {
            float: 4.2,
            array: [NonZeroUsize::new(5).unwrap(), NonZeroUsize::new(4).unwrap()],
            choices: Some(vec![SimpleEnum::First, SimpleEnum::Second]),
            assumed: Some(serde_json::json!(24)),
            short_dur: Duration::from_millis(200),
            long_dur: Duration::from_secs(5 * 60),
            path: "/mnt".into(),
            memory_size_mb: Some(ByteSize::new(64, SizeUnit::MiB)),
            disk_size: Some(ByteSize::new(4, SizeUnit::GiB)),
            paths: vec!["/usr/bin".into(), "/usr/local/bin".into()],
            map_or_string: MapOrString(HashMap::from([("test".into(), 1), ("other".into(), 2),])),
        }
    );

    let json = config!(
        "array": [5, 6],
        "float": -3,
        "choices": (),
        "assumed": (),
        "short_dur": 1000,
        "long_dur": serde_json::json!({ "millis": 500 }),
        "memory_size_mb": (),
        "disk_size": serde_json::json!({ "mb": 256 }),
        "paths": ["/usr/bin", "/mnt"],
        "map_or_string": serde_json::json!({
            "test": 42,
            "other": 23,
        }),
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
            long_dur: Duration::from_millis(500),
            path: "./test".into(),
            memory_size_mb: None,
            disk_size: Some(ByteSize::new(256, SizeUnit::MiB)),
            paths: vec!["/usr/bin".into(), "/mnt".into()],
            map_or_string: MapOrString(HashMap::from([("test".into(), 42), ("other".into(), 23),])),
        }
    );
}

#[test]
fn parsing_composed_params() {
    let json = config!("durations": ["1sec", "5 min"]);
    let config: ComposedConfig = test_deserialize(json.inner()).unwrap();
    let expected_array = [Duration::from_secs(1), Duration::from_secs(300)];
    assert_eq!(config.durations, expected_array);

    let json = config!(
        "durations": serde_json::json!(["1 sec", { "min": 5 }])
    );
    let config: ComposedConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(config.durations, expected_array);

    let json = config!("delimited_durations": ["1sec", "5 min"]);
    let config: ComposedConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(config.delimited_durations, expected_array);

    let json = config!("delimited_durations": "1 sec,5 min");
    let config: ComposedConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(config.delimited_durations, expected_array);

    let json = config!("map_of_sizes": HashMap::from([("small", "1 MiB"), ("large", "3 MiB")]));
    let config: ComposedConfig = test_deserialize(json.inner()).unwrap();
    let expected_map = HashMap::from([
        ("small".to_owned(), ByteSize(1 << 20)),
        ("large".to_owned(), ByteSize(3 << 20)),
    ]);
    assert_eq!(config.map_of_sizes, expected_map);

    let json = config!("map_of_ints": HashMap::from([(5, "30 sec")]));
    let config: ComposedConfig = test_deserialize(json.inner()).unwrap();
    assert_eq!(config.map_of_ints[&5], Duration::from_secs(30));
}

#[test]
fn error_parsing_array_from_string() {
    let json = config!("array": "4,what");
    let err = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    assert_eq!(err.len(), 1);
    let err = err.first();
    assert_eq!(err.path(), "array.1");
    let inner = err.inner().to_string();
    assert!(inner.contains("what"), "{inner}");

    let ValueOrigin::Path { source, path } = err.origin() else {
        panic!("unexpected origin: {:?}", err.origin());
    };
    assert_eq!(path, "1");
    let ValueOrigin::Synthetic { source, transform } = source.as_ref() else {
        panic!("unexpected source origin: {source:?}");
    };
    assert_eq!(transform, "\",\"-delimited string");
    assert_matches!(
        source.as_ref(),
        ValueOrigin::Path { source, path }
            if path == "array" && matches!(source.as_ref(), ValueOrigin::File { .. })
    );
}

#[test]
fn error_parsing_string_or_complex_value() {
    let json = config!("array": [4, 5], "map_or_string": "??");
    let err = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    assert_eq!(err.len(), 1);
    let err = err.first();
    assert_eq!(err.path(), "map_or_string");
    let inner = err.inner().to_string();
    assert!(inner.contains("incorrect entry"), "{inner}");

    let json = config!("array": [4, 5], "map_or_string": "test=what");
    let err = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    assert_eq!(err.len(), 1);
    let err = err.first();
    assert_eq!(err.path(), "map_or_string");
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid value"), "{inner}");
}

#[test]
fn parsing_complex_types_errors() {
    let json = config!("array": [1]);
    let errors = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid length"), "{inner}");
    assert_eq!(err.path(), "array");
    assert_matches!(err.origin(), ValueOrigin::Path { path, .. } if path == "array");

    let json = config!("array": [0, 1]);
    let errors = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(inner.contains("nonzero"), "{inner}");
    assert_eq!(err.path(), "array.0");
    assert_matches!(err.origin(), ValueOrigin::Path { path, .. } if path == "array.0");

    let json = config!("array": [2, 3], "float": "what");
    let errors = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(inner.contains("what"), "{inner}");
    assert_eq!(err.path(), "float");
    assert_matches!(err.origin(), ValueOrigin::Path { path, .. } if path == "float");

    let json = config!("array": [2, 3], "assumed": true);
    let errors = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    let err = errors.first();
    let inner = err.inner().to_string();
    assert!(
        inner.contains("invalid type") && inner.contains("expected integer | float"),
        "{inner}"
    );
    assert_eq!(err.path(), "assumed");
    assert_matches!(err.origin(), ValueOrigin::Path { path, .. } if path == "assumed");
}

#[test]
fn multiple_errors_for_composed_deserializers() {
    let json = config!("array": "-4,what");
    let errors = test_deserialize::<ConfigWithComplexTypes>(json.inner()).unwrap_err();
    assert_eq!(errors.len(), 2);
    let err = errors.iter().find(|err| err.path() == "array.0").unwrap();
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid value"), "{inner}");
    let err = errors.iter().find(|err| err.path() == "array.1").unwrap();
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid type"), "{inner}");

    let json = config!("durations": r#"[5, "30us"]"#);
    let errors = test_deserialize::<ComposedConfig>(json.inner()).unwrap_err();
    assert_eq!(errors.len(), 2);
    let err = errors
        .iter()
        .find(|err| err.path() == "durations.0")
        .unwrap();
    let inner = err.inner().to_string();
    assert!(inner.contains("invalid type"), "{inner}");
    let err = errors
        .iter()
        .find(|err| err.path() == "durations.1")
        .unwrap();
    let inner = err.inner().to_string();
    assert!(
        inner.contains("invalid value") && inner.contains("duration unit"),
        "{inner}"
    );

    let json = config!("map_of_ints": r#"{ "what": 120 }"#);
    let errors = test_deserialize::<ComposedConfig>(json.inner()).unwrap_err();
    assert_eq!(errors.len(), 2);
    assert!(errors.iter().any(|err| {
        err.path() == "map_of_ints.what"
            && err
                .inner()
                .to_string()
                .starts_with("cannot deserialize key")
    }));
    assert!(errors.iter().any(|err| {
        err.path() == "map_of_ints.what" && err.inner().to_string().starts_with("invalid type")
    }));
}
