use std::collections::HashSet;

use assert_matches::assert_matches;

use super::*;
use crate::{
    metadata::BasicTypes,
    testonly::{AliasedConfig, NestedAliasedConfig, NestedConfig},
    value::{StrValue, Value},
    ConfigRepository, DescribeConfig, DeserializeConfig, Environment,
};

/// # Test configuration
///
/// Extended description.
#[derive(Debug, Default, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(crate = crate)]
struct TestConfig {
    /// String value.
    #[config(deprecated = "string", default = TestConfig::default_str)]
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
    assert_eq!(metadata.params.len(), 2);

    let str_metadata = &metadata.params[0];
    assert_eq!(str_metadata.name, "str");
    assert_eq!(
        str_metadata.aliases,
        [("string", AliasOptions::new().deprecated())]
    );
    assert_eq!(str_metadata.help, "String value.");
    assert_eq!(str_metadata.rust_type.name_in_code(), "String");
    assert_eq!(str_metadata.default_value_json().unwrap(), "default");

    let optional_metadata = &metadata.params[1];
    assert_eq!(optional_metadata.name, "optional");
    assert_eq!(optional_metadata.aliases, [] as [_; 0]);
    assert_eq!(optional_metadata.help, "Optional value.");
    let name_in_code = optional_metadata.rust_type.name_in_code();
    assert!(name_in_code.starts_with("Option"), "{name_in_code}");
    assert_eq!(optional_metadata.expecting, BasicTypes::INTEGER);
}

#[test]
fn using_alias() {
    let mut schema = ConfigSchema::default();
    schema
        .insert(&TestConfig::DESCRIPTION, "test")
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
    assert_matches!(
        &parser.merged().get(Pointer("test.str")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "test"
    );
    assert_matches!(
        &parser.merged().get(Pointer("test.optional")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "123"
    );

    let config: TestConfig = parser.single().unwrap().parse().unwrap();
    assert_eq!(config.str, "test");
    assert_eq!(config.optional_int, Some(123));
}

#[test]
fn using_multiple_aliases() {
    let mut schema = ConfigSchema::default();
    schema
        .insert(&TestConfig::DESCRIPTION, "test")
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
    assert!(config_ref.is_top_level());

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
    let schema = ConfigSchema::new(&NestingConfig::DESCRIPTION, "");
    let config_prefixes: Vec<_> = schema.locate(&NestingConfig::DESCRIPTION).collect();
    assert_eq!(config_prefixes, [""]);
    let config_prefixes: HashSet<_> = schema.locate(&TestConfig::DESCRIPTION).collect();
    assert_eq!(config_prefixes, HashSet::from(["", "hierarchical"]));

    let refs: Vec<_> = schema
        .iter()
        .map(|config_ref| {
            (
                config_ref.prefix,
                config_ref.is_top_level(),
                config_ref.metadata().ty.name_in_code(),
            )
        })
        .collect();
    assert_eq!(
        refs,
        [
            ("", true, "NestingConfig"),
            ("", false, "TestConfig"),
            ("hierarchical", false, "TestConfig")
        ]
    );

    let parent_link = schema
        .single(&NestingConfig::DESCRIPTION)
        .unwrap()
        .parent_link();
    assert!(parent_link.is_none());
    let (parent_ref, this_ref) = schema
        .get(&TestConfig::DESCRIPTION, "")
        .unwrap()
        .parent_link()
        .unwrap();
    assert_eq!(parent_ref.metadata().ty.name_in_code(), "NestingConfig");
    assert_eq!(this_ref.rust_field_name, "flattened");
    let (parent_ref, this_ref) = schema
        .get(&TestConfig::DESCRIPTION, "hierarchical")
        .unwrap()
        .parent_link()
        .unwrap();
    assert_eq!(parent_ref.metadata().ty.name_in_code(), "NestingConfig");
    assert_eq!(this_ref.rust_field_name, "hierarchical");

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
    assert_matches!(
        &repo.merged().get(Pointer("bool_value")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "true"
    );
    assert_matches!(
        &repo.merged()
            .get(Pointer("hierarchical.str"))
            .unwrap()
            .inner,
        Value::String(StrValue::Plain(s)) if s == "???"
    );
    assert_matches!(
        &repo.merged().get(Pointer("optional")).unwrap().inner,
        Value::String(StrValue::Plain(s)) if s == "777"
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

#[derive(Debug, DescribeConfig)]
#[config(crate = crate)]
struct BogusNestedConfigWithAlias {
    #[allow(dead_code)]
    #[config(nest, alias = "str")]
    nested: TestConfig,
}

#[test]
fn mountpoint_errors() {
    let mut schema = ConfigSchema::default();
    schema.insert(&NestingConfig::DESCRIPTION, "test").unwrap();
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
        .insert(&BogusParamConfig::DESCRIPTION, "test")
        .unwrap_err()
        .to_string();
    assert!(err.contains("[Rust field: `hierarchical`]"), "{err}");
    assert!(err.contains("config(s) are already mounted"), "{err}");

    let err = schema
        .insert(&BogusNestedConfig::DESCRIPTION, "test")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot mount config"), "{err}");
    assert!(err.contains("at `test.str`"), "{err}");
    assert!(err.contains("parameter(s) are already mounted"), "{err}");

    let err = schema
        .insert(&BogusNestedConfig::DESCRIPTION, "test.bool_value")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot mount config"), "{err}");
    assert!(err.contains("at `test.bool_value`"), "{err}");
    assert!(err.contains("parameter(s) are already mounted"), "{err}");

    let err = schema
        .insert(&BogusParamTypeConfig::DESCRIPTION, "test")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot insert param"), "{err}");
    assert!(err.contains("at `test.bool_value`"), "{err}");
    assert!(err.contains("expects integer"), "{err}");
}

#[test]
fn aliasing_mountpoint_errors() {
    let mut schema = ConfigSchema::default();
    schema.insert(&NestingConfig::DESCRIPTION, "test").unwrap();

    let err = schema
        .insert(&BogusParamConfig::DESCRIPTION, "bogus")
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
        .insert(&BogusParamTypeConfig::DESCRIPTION, "bogus")
        .unwrap()
        .push_alias("test")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot insert param"), "{err}");
    assert!(err.contains("at `test.bool_value`"), "{err}");
    assert!(err.contains("expects integer"), "{err}");
}

#[test]
fn aliasing_mountpoint_errors_via_nested_configs() {
    let mut schema = ConfigSchema::default();
    schema.insert(&NestingConfig::DESCRIPTION, "test").unwrap();

    let err = schema
        .insert(&BogusNestedConfigWithAlias::DESCRIPTION, "test")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot mount config"), "{err}");
    assert!(err.contains("at `test.str`"), "{err}");
    assert!(err.contains("parameter(s) are already mounted"), "{err}");

    // Mount a config at the location of a param of the nested config.
    let mut schema = ConfigSchema::default();
    schema
        .insert(&TestConfig::DESCRIPTION, "str.optional")
        .unwrap();

    let err = schema
        .insert(&BogusNestedConfigWithAlias::DESCRIPTION, "")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot insert param"), "{err}");
    assert!(err.contains("at `str.optional`"), "{err}");
    assert!(err.contains(" config(s) are already mounted"), "{err}");
}

#[test]
fn aliasing_info_for_nested_configs() {
    let mut schema = ConfigSchema::default();
    schema
        .insert(&AliasedConfig::DESCRIPTION, "test")
        .unwrap()
        .push_deprecated_alias("alias")
        .unwrap();
    let aliases: Vec<_> = schema
        .single(&AliasedConfig::DESCRIPTION)
        .unwrap()
        .aliases()
        .collect();
    assert_eq!(aliases, [("alias", AliasOptions::new().deprecated())]);
    let aliases: Vec<_> = schema
        .get(&NestedAliasedConfig::DESCRIPTION, "test")
        .unwrap()
        .aliases()
        .collect();
    assert_eq!(aliases, [("alias", AliasOptions::new().deprecated())]);
    let aliases: Vec<_> = schema
        .get(&NestedAliasedConfig::DESCRIPTION, "test.nested")
        .unwrap()
        .aliases()
        .collect();
    assert_eq!(
        aliases,
        [
            ("test.nest", AliasOptions::new()),
            ("alias.nested", AliasOptions::new().deprecated()),
            ("alias.nest", AliasOptions::new().deprecated())
        ]
    );

    let param_paths = [
        "test_nested_str",
        "test_nested_string",
        "alias_nested_str",
        "alias_nest_string",
    ];
    for path in param_paths {
        println!("Testing path: {path}");
        let mut data: Vec<_> = schema.params_with_kv_path(path).collect();
        assert_eq!(data.len(), 1);
        let (_, expecting) = data.pop().unwrap();
        assert_eq!(expecting, BasicTypes::STRING);
    }
}

#[test]
fn aliasing_does_not_change_config_depth() {
    let mut schema = ConfigSchema::default();
    schema.insert(&AliasedConfig::DESCRIPTION, "test").unwrap();

    let expected_index_by_depth = BTreeSet::from([
        (0, any::TypeId::of::<AliasedConfig>()),
        (1, any::TypeId::of::<NestedAliasedConfig>()),
    ]);
    assert_eq!(schema.configs["test"].by_depth, expected_index_by_depth);
    assert_eq!(
        schema.configs["test.nested"].by_depth,
        BTreeSet::from([(1, any::TypeId::of::<NestedAliasedConfig>())])
    );

    schema
        .get_mut(&NestedAliasedConfig::DESCRIPTION, "test")
        .unwrap()
        .push_alias("alias")
        .unwrap();
    assert!(!schema
        .get(&NestedAliasedConfig::DESCRIPTION, "test")
        .unwrap()
        .is_top_level());

    assert_eq!(schema.configs["test"].by_depth, expected_index_by_depth);
    assert_eq!(
        schema.configs["test.nested"].by_depth,
        BTreeSet::from([(1, any::TypeId::of::<NestedAliasedConfig>())])
    );

    // Insert a top-level config at the location of a nested config.
    schema
        .insert(&TestConfig::DESCRIPTION, "test.nested")
        .unwrap();

    assert_eq!(schema.configs["test"].by_depth, expected_index_by_depth);
    let expected_nested_index_by_depth = BTreeSet::from([
        (0, any::TypeId::of::<TestConfig>()),
        (1, any::TypeId::of::<NestedAliasedConfig>()),
    ]);
    assert_eq!(
        schema.configs["test.nested"].by_depth,
        expected_nested_index_by_depth
    );

    // Insert another instance of a nested config (should be no-op).
    schema
        .insert(&NestedAliasedConfig::DESCRIPTION, "test.nested")
        .unwrap();
    assert_eq!(
        schema.configs["test.nested"].by_depth,
        expected_nested_index_by_depth
    );
}

#[test]
fn config_cannot_be_nested_to_path_alias() {
    let mut schema = ConfigSchema::default();
    schema.insert(&NestedConfig::DESCRIPTION, "test").unwrap();

    let err = schema
        .insert(&NestedConfig::DESCRIPTION, "test.experimental.enum")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot mount config"), "{err}");
    assert!(err.contains("at `test.experimental.enum`"), "{err}");
    assert!(err.contains("parameter(s) are already mounted"), "{err}");

    let err = schema
        .insert(&NestedConfig::DESCRIPTION, "top.enum")
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cannot mount config"), "{err}");
    assert!(err.contains("at `top.enum`"), "{err}");
    assert!(err.contains("parameter(s) are already mounted"), "{err}");
}
