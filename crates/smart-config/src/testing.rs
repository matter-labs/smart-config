//! Testing tools for configurations.

use std::collections::HashMap;

use crate::{
    metadata::{ConfigMetadata, RustType},
    schema::ConfigSchema,
    value::{Pointer, WithOrigin},
    ConfigRepository, ConfigSource, DeserializeConfig, ParseErrors,
};

/// Tests config deserialization from the provided `sample`. Takes into account param aliases,
/// performs `sample` preprocessing etc.
///
/// # Errors
///
/// Propagates parsing errors, which allows testing negative cases.
///
/// # Examples
///
/// ## Basic usage
///
/// ```
/// # use smart_config::{DescribeConfig, DeserializeConfig};
/// use smart_config::{metadata::SizeUnit, testing, ByteSize};
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(default_t = true)]
///     flag: bool,
///     #[config(with = SizeUnit::MiB)]
///     size_mb: ByteSize,
/// }
///
/// let sample = smart_config::config!("size_mb": 2);
/// let config: TestConfig = testing::test(sample)?;
/// assert!(config.flag);
/// assert_eq!(config.size_mb, ByteSize(2 << 20));
/// # anyhow::Ok(())
/// ```
///
/// ## Testing errors
///
/// ```
/// # use smart_config::{testing, DescribeConfig, DeserializeConfig};
/// #[derive(Debug, DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(default_t = true, alias = "flag")]
///     boolean: bool,
/// }
///
/// let sample = smart_config::config!("flag": "no");
/// let err = testing::test::<TestConfig>(sample).unwrap_err();
/// let err = err.first();
/// assert_eq!(err.path(), "boolean");
/// assert!(err
///     .inner()
///     .to_string()
///     .contains("provided string was not `true` or `false`"));
/// ```
#[allow(clippy::missing_panics_doc)] // can only panic if the config is recursively defined, which is impossible
pub fn test<C: DeserializeConfig>(sample: impl ConfigSource) -> Result<C, ParseErrors> {
    let mut schema = ConfigSchema::default();
    schema.insert::<C>("").unwrap();
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
///
/// # Examples
///
/// ## Basic usage
///
/// ```
/// # use smart_config::{DescribeConfig, DeserializeConfig};
/// use smart_config::{metadata::SizeUnit, testing, ByteSize};
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(default_t = true, alias = "flag")]
///     boolean: bool,
///     #[config(with = SizeUnit::MiB)]
///     size_mb: ByteSize,
/// }
///
/// let sample = smart_config::config!("flag": "false", "size_mb": 64);
/// let config: TestConfig = testing::test_complete(sample)?;
/// assert!(!config.boolean);
/// assert_eq!(config.size_mb, ByteSize(64 << 20));
/// # anyhow::Ok(())
/// ```
///
/// ## Panics on incomplete sample
///
/// ```should_panic
/// # use smart_config::{DescribeConfig, DeserializeConfig};
/// # use smart_config::{metadata::SizeUnit, testing, ByteSize};
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(default_t = true, alias = "flag")]
///     boolean: bool,
///     #[config(with = SizeUnit::MiB)]
///     size_mb: ByteSize,
/// }
///
/// let incomplete_sample = smart_config::config!("flag": "false");
/// // Will panic with a message detailing missing params (`size_mb` in this case)
/// testing::test_complete::<TestConfig>(incomplete_sample)?;
/// # anyhow::Ok(())
/// ```
pub fn test_complete<C: DeserializeConfig>(sample: impl ConfigSource) -> Result<C, ParseErrors> {
    let mut schema = ConfigSchema::default();
    schema.insert::<C>("").unwrap();
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
            "nested_opt.other_int": 777,
            "nested_opt.renamed": "first",
            "nested_opt.map": HashMap::<&str, u32>::new(),
            "default.other_int": 11,
            "default.renamed": "second",
            "default.map": HashMap::from([("test", 1)]),
        );
        let config = test_complete::<CompoundConfig>(json).unwrap();
        assert_eq!(config.flat.other_int, 123);
        assert_eq!(config.nested.other_int, 42);
        assert_eq!(config.nested_default.other_int, 11);
        let opt = config.nested_opt.unwrap();
        assert_eq!(opt.other_int, 777);
        assert_eq!(opt.simple_enum, SimpleEnum::First);
        assert_eq!(opt.map, HashMap::new());
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
