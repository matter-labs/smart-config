//! Testing tools for configurations.

use std::collections::HashMap;

use crate::{
    metadata::{ConfigMetadata, RustType},
    schema::ConfigSchema,
    value::{Pointer, WithOrigin},
    ConfigRepository, ConfigSource, DeserializeConfig, ParseErrors,
};

/// Tests config deserialization from the provided `sample`.
///
/// # Errors
///
/// Propagates parsing errors, which allows testing negative cases.
#[allow(clippy::missing_panics_doc)] // can only panic if the config is recursively defined, which is impossible (?)
pub fn test<C: DeserializeConfig>(sample: impl ConfigSource) -> Result<C, ParseErrors> {
    let schema = ConfigSchema::default().insert::<C>("").unwrap();
    let repo = ConfigRepository::new(&schema).with(sample);
    repo.single::<C>().unwrap().parse()
}

/// Tests config deserialization ensuring that *all* declared config params are covered.
///
/// # Panics
///
/// Panics if the `sample` doesn't recursively cover all params in the config. The config message
/// will contain paths to the missing params.
///
/// # Errors
///
/// Propagates parsing errors, which allows testing negative cases.
pub fn test_complete<C: DeserializeConfig>(sample: impl ConfigSource) -> Result<C, ParseErrors> {
    let schema = ConfigSchema::default().insert::<C>("").unwrap();
    let repo = ConfigRepository::new(&schema).with(sample);

    let metadata = &C::DESCRIPTION;
    let mut missing_params = HashMap::new();
    let mut missing_configs = HashMap::new();
    check_params(
        Pointer(""),
        repo.merged(),
        metadata,
        &mut missing_params,
        &mut missing_configs,
    );

    assert!(
        missing_params.is_empty() && missing_configs.is_empty(),
        "The provided sample is incomplete; missing params: {missing_params:?}, missing configs: {missing_configs:?}"
    );

    repo.single::<C>().unwrap().parse()
}

fn check_params(
    current_path: Pointer<'_>,
    sample: &WithOrigin,
    metadata: &'static ConfigMetadata,
    missing_params: &mut HashMap<String, RustType>,
    missing_configs: &mut HashMap<String, RustType>,
) {
    for param in metadata.params {
        if sample.get(Pointer(param.name)).is_none() {
            missing_params.insert(current_path.join(param.name), param.rust_type);
        }
    }
    for nested in metadata.nested_configs {
        let Some(child) = sample.get(Pointer(nested.name)) else {
            missing_configs.insert(current_path.join(nested.name), nested.meta.ty);
            continue;
        };
        check_params(
            Pointer(&current_path.join(nested.name)),
            child,
            nested.meta,
            missing_params,
            missing_configs,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{
        config,
        testonly::{CompoundConfig, DefaultingConfig, SimpleEnum},
        Environment, Json,
    };

    #[test]
    fn testing_config() {
        let config = test::<DefaultingConfig>(Json::empty("test.json")).unwrap();
        assert_eq!(config, DefaultingConfig::default());

        let json = config!("float": 4.2, "url": ());
        let config = test::<DefaultingConfig>(json).unwrap();
        assert_eq!(
            config,
            DefaultingConfig {
                float: Some(4.2),
                url: None,
                ..DefaultingConfig::default()
            }
        );
    }

    #[should_panic(expected = "missing params")]
    #[test]
    fn panicking_on_incomplete_sample() {
        test_complete::<CompoundConfig>(Json::empty("test.json")).ok();
    }

    #[test]
    fn complete_testing() {
        let json = config!(
            "other_int": 123,
            "renamed": "first",
            "map": HashMap::from([("test", 3)]),
            "nested.other_int": 42,
            "nested.renamed": "second",
            "nested.map": HashMap::from([("test", 2)]),
            "default.other_int": 11,
            "default.renamed": "second",
            "default.map": HashMap::from([("test", 1)]),
        );
        let config = test_complete::<CompoundConfig>(json).unwrap();
        assert_eq!(config.flat.other_int, 123);
        assert_eq!(config.nested.other_int, 42);
        assert_eq!(config.nested_default.other_int, 11);
    }

    #[test]
    fn complete_testing_for_env_vars() {
        let env = Environment::from_dotenv(
            "test.env",
            r#"
            APP_INT=123
            APP_FLOAT=8.4
            APP_URL="https://example.com/"
            APP_SET="first,second"
            "#,
        )
        .unwrap()
        .strip_prefix("APP_");
        let config = test_complete::<DefaultingConfig>(env).unwrap();
        assert_eq!(config.int, 123);
        assert_eq!(config.float, Some(8.4));
        assert_eq!(config.url.unwrap(), "https://example.com/");
        assert_eq!(
            config.set,
            HashSet::from([SimpleEnum::First, SimpleEnum::Second])
        );
    }
}
