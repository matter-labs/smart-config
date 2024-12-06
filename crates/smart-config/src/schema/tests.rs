use std::collections::HashSet;

use assert_matches::assert_matches;

use super::*;
use crate::{
    metadata::BasicTypes, value::Value, ConfigRepository, DescribeConfig, DeserializeConfig,
    Environment,
};

/// # Test configuration
///
/// Extended description.
#[derive(Debug, Default, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
struct TestConfig {
    /// String value.
    #[config(alias = "string", default = TestConfig::default_str)]
    str: String,
    /// Optional value.
    #[config(rename = "optional")]
    optional_int: Option<u32>,
}

impl TestConfig {
    fn default_str() -> String {
        "default".to_owned()
    }
}

#[derive(Debug, Default, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
struct NestingConfig {
    #[config(default)]
    bool_value: bool,
    /// Hierarchical nested config.
    #[config(default, nest)]
    hierarchical: TestConfig,
    #[config(default, flatten)]
    flattened: TestConfig,
}

#[test]
fn getting_config_metadata() {
    let metadata = &TestConfig::DESCRIPTION;
    assert_eq!(metadata.ty.name_in_code(), "TestConfig");
    assert_eq!(metadata.help, "# Test configuration\nExtended description.");
    assert_eq!(metadata.help_header(), Some("Test configuration"));
    assert_eq!(metadata.params.len(), 2);

    let str_metadata = &metadata.params[0];
    assert_eq!(str_metadata.name, "str");
    assert_eq!(str_metadata.aliases, ["string"]);
    assert_eq!(str_metadata.help, "String value.");
    assert_eq!(str_metadata.rust_type.name_in_code(), "String");
    assert_eq!(
        format!("{:?}", str_metadata.default_value().unwrap()),
        "\"default\""
    );

    let optional_metadata = &metadata.params[1];
    assert_eq!(optional_metadata.name, "optional");
    assert_eq!(optional_metadata.aliases, [] as [&str; 0]);
    assert_eq!(optional_metadata.help, "Optional value.");
    assert_eq!(optional_metadata.rust_type.name_in_code(), "Option"); // FIXME: does `Option<u32>` get printed only for nightly Rust?
    assert_eq!(optional_metadata.expecting, BasicTypes::INTEGER);
}

#[test]
fn using_alias() {
    let mut schema = ConfigSchema::default();
    schema
        .insert::<TestConfig>("test")
        .unwrap()
        .push_alias("")
        .unwrap();

    let config_prefixes: Vec<_> = schema.locate(&TestConfig::DESCRIPTION).collect();
    assert_eq!(config_prefixes, ["test"]);
    let config_ref = schema.single(&TestConfig::DESCRIPTION).unwrap();
    assert_eq!(config_ref.prefix(), "test");
    assert_eq!(config_ref.aliases().count(), 1);

    let env = Environment::from_iter("APP_", [("APP_TEST_STR", "test"), ("APP_OPTIONAL", "123")]);

    let parser = ConfigRepository::new(&schema).with(env);
    assert_eq!(
        parser.merged().get(Pointer("test.str")).unwrap().inner,
        Value::String("test".into())
    );
    assert_eq!(
        parser.merged().get(Pointer("test.optional")).unwrap().inner,
        Value::String("123".into())
    );

    let config: TestConfig = parser.single().unwrap().parse().unwrap();
    assert_eq!(config.str, "test");
    assert_eq!(config.optional_int, Some(123));
}

#[test]
fn using_multiple_aliases() {
    let mut schema = ConfigSchema::default();
    schema
        .insert::<TestConfig>("test")
        .unwrap()
        .push_alias("")
        .unwrap()
        .push_alias("deprecated")
        .unwrap();

    let config_prefixes: Vec<_> = schema.locate(&TestConfig::DESCRIPTION).collect();
    assert_eq!(config_prefixes, ["test"]);
    let config_ref = schema.single(&TestConfig::DESCRIPTION).unwrap();
    assert_eq!(config_ref.prefix(), "test");
    assert_eq!(config_ref.aliases().count(), 2);

    let env = Environment::from_iter(
        "APP_",
        [
            ("APP_TEST_STR", "?"),
            ("APP_OPTIONAL", "123"),
            ("APP_DEPRECATED_STR", "!"), // should not be used (original var is defined)
            ("APP_DEPRECATED_OPTIONAL", "321"), // should not be used (alias has lower priority)
        ],
    );
    let config: TestConfig = ConfigRepository::new(&schema)
        .with(env)
        .single()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(config.str, "?");
    assert_eq!(config.optional_int, Some(123));
}

#[test]
fn using_nesting() {
    let mut schema = ConfigSchema::default();
    schema.insert::<NestingConfig>("").unwrap();

    let config_prefixes: Vec<_> = schema.locate(&NestingConfig::DESCRIPTION).collect();
    assert_eq!(config_prefixes, [""]);
    let config_prefixes: HashSet<_> = schema.locate(&TestConfig::DESCRIPTION).collect();
    assert_eq!(config_prefixes, HashSet::from(["", "hierarchical"]));

    let err = schema
        .single(&TestConfig::DESCRIPTION)
        .unwrap_err()
        .to_string();
    assert!(err.contains("at least 2 locations"), "{err}");

    let env = Environment::from_iter(
        "",
        [
            ("bool_value", "true"),
            ("hierarchical_string", "???"),
            ("str", "!!!"),
            ("optional", "777"),
        ],
    );
    let repo = ConfigRepository::new(&schema).with(env);
    assert_eq!(
        repo.merged().get(Pointer("bool_value")).unwrap().inner,
        Value::String("true".into())
    );
    assert_eq!(
        repo.merged()
            .get(Pointer("hierarchical.str"))
            .unwrap()
            .inner,
        Value::String("???".into())
    );
    assert_eq!(
        repo.merged().get(Pointer("optional")).unwrap().inner,
        Value::String("777".into())
    );

    let config: NestingConfig = repo.single().unwrap().parse().unwrap();
    assert!(config.bool_value);
    assert_eq!(config.hierarchical.str, "???");
    assert_eq!(config.hierarchical.optional_int, None);
    assert_eq!(config.flattened.str, "!!!");
    assert_eq!(config.flattened.optional_int, Some(777));
}

#[derive(Debug, DescribeConfig)]
#[config(crate = crate)]
struct BogusParamConfig {
    #[allow(dead_code)]
    hierarchical: u64,
}

#[derive(Debug, DescribeConfig)]
#[config(crate = crate)]
struct BogusParamTypeConfig {
    #[allow(dead_code)]
    bool_value: u64,
}

#[derive(Debug, DescribeConfig)]
#[config(crate = crate)]
struct BogusNestedConfig {
    #[allow(dead_code)]
    #[config(nest)]
    str: TestConfig,
}

#[test]
fn mountpoint_errors() {
    let mut schema = ConfigSchema::default();
    schema.insert::<NestingConfig>("test").unwrap();
    assert_matches!(
        schema.mounting_points["test.hierarchical"],
        MountingPoint::Config
    );
    assert_matches!(
        schema.mounting_points["test.bool_value"],
        MountingPoint::Param {
            expecting: BasicTypes::BOOL,
            is_canonical: true,
        }
    );
    assert_matches!(
        schema.mounting_points["test.str"],
        MountingPoint::Param {
            expecting: BasicTypes::STRING,
            is_canonical: true,
        }
    );
    assert_matches!(
        schema.mounting_points["test.string"],
        MountingPoint::Param {
            expecting: BasicTypes::STRING,
            is_canonical: false,
        }
    );
    assert_matches!(
        schema.mounting_points["test.hierarchical.str"],
        MountingPoint::Param {
            expecting: BasicTypes::STRING,
            is_canonical: true,
        }
    );

    let err = schema
        .insert::<BogusParamConfig>("test")
        .unwrap_err()
        .to_string();
    assert!(err.contains("[Rust field: `hierarchical`]"), "{err}");
    assert!(err.contains("config(s) are already mounted"), "{err}");

    let err = schema
        .insert::<BogusNestedConfig>("test")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot mount config"), "{err}");
    assert!(err.contains("at `test.str`"), "{err}");
    assert!(err.contains("parameter(s) are already mounted"), "{err}");

    let err = schema
        .insert::<BogusNestedConfig>("test.bool_value")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot mount config"), "{err}");
    assert!(err.contains("at `test.bool_value`"), "{err}");
    assert!(err.contains("parameter(s) are already mounted"), "{err}");

    let err = schema
        .insert::<BogusParamTypeConfig>("test")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot insert param"), "{err}");
    assert!(err.contains("at `test.bool_value`"), "{err}");
    assert!(err.contains("expects integer"), "{err}");
}

#[test]
fn aliasing_mountpoint_errors() {
    let mut schema = ConfigSchema::default();
    schema.insert::<NestingConfig>("test").unwrap();

    let err = schema
        .insert::<BogusParamConfig>("bogus")
        .unwrap()
        .push_alias("test")
        .unwrap_err()
        .to_string();
    assert!(err.contains("[Rust field: `hierarchical`]"), "{err}");
    assert!(err.contains("config(s) are already mounted"), "{err}");

    assert_matches!(
        schema.mounting_points["bogus.hierarchical"],
        MountingPoint::Param {
            expecting: BasicTypes::INTEGER,
            is_canonical: true,
        }
    );
    assert_matches!(
        schema.mounting_points["test.hierarchical"],
        MountingPoint::Config
    );

    let err = schema
        .insert::<BogusParamTypeConfig>("bogus")
        .unwrap()
        .push_alias("test")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot insert param"), "{err}");
    assert!(err.contains("at `test.bool_value`"), "{err}");
    assert!(err.contains("expects integer"), "{err}");
}
