//! Testing tools for configurations.

use std::{any, cell::RefCell, collections::HashMap, marker::PhantomData, mem};

use crate::{
    ConfigRepository, ConfigSource, DeserializeConfig, ParseErrors,
    de::DeserializerOptions,
    metadata::{ConfigMetadata, NestedConfigMetadata, ParamMetadata, RustType},
    schema::ConfigSchema,
    value::{Pointer, WithOrigin},
    visit::{ConfigVisitor, VisitConfig},
};

// We don't actually use `std::env::set_var()` because it is unsafe (and will be marked as such in future Rust editions).
// On non-Windows OSes, env access is not synchronized across threads.
thread_local! {
    pub(crate) static MOCK_ENV_VARS: RefCell<HashMap<String, String>> = RefCell::default();
}

#[derive(Debug)]
pub(crate) struct MockEnvGuard {
    _not_send: PhantomData<*mut ()>,
}

impl Default for MockEnvGuard {
    fn default() -> Self {
        MOCK_ENV_VARS.with_borrow(|vars| {
            assert!(
                vars.is_empty(),
                "Cannot define mock env vars while another `Tester` is active"
            );
        });

        Self {
            _not_send: PhantomData,
        }
    }
}

impl MockEnvGuard {
    #[allow(clippy::unused_self)] // used for better type safety
    pub(crate) fn set_env(&self, name: String, value: String) {
        MOCK_ENV_VARS.with_borrow_mut(|vars| vars.insert(name, value));
    }
}

impl Drop for MockEnvGuard {
    fn drop(&mut self) {
        MOCK_ENV_VARS.take(); // Remove all mocked env vars
    }
}

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
pub fn test<C: DeserializeConfig>(sample: impl ConfigSource) -> Result<C, ParseErrors> {
    Tester::default().test(sample)
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
/// let incomplete_sample = smart_config::config!("size_mb": 64);
/// // Will panic with a message detailing missing params (`flag` in this case)
/// testing::test_complete::<TestConfig>(incomplete_sample)?;
/// # anyhow::Ok(())
/// ```
#[track_caller] // necessary for assertion panics to be located in the test code, rather than in this crate
pub fn test_complete<C: DeserializeConfig>(sample: impl ConfigSource) -> Result<C, ParseErrors> {
    Tester::default().test_complete(sample)
}

/// Tests config deserialization ensuring that *only* required config params are covered.
/// This is useful to detect new required params (i.e., backward-incompatible changes).
///
/// # Panics
///
/// Panics if the `sample` contains non-required params in the config. The config message
/// will contain paths to the redundant params.
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
/// let sample = smart_config::config!("size_mb": 64);
/// let config: TestConfig = testing::test_minimal(sample)?;
/// assert!(config.boolean);
/// assert_eq!(config.size_mb, ByteSize(64 << 20));
/// # anyhow::Ok(())
/// ```
///
/// ## Panics on redundant sample
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
/// let redundant_sample = smart_config::config!("flag": "false", "size_mb": 64);
/// // Will panic with a message detailing redundant params (`boolean` in this case)
/// testing::test_minimal::<TestConfig>(redundant_sample)?;
/// # anyhow::Ok(())
/// ```
#[track_caller]
pub fn test_minimal<C: DeserializeConfig>(sample: impl ConfigSource) -> Result<C, ParseErrors> {
    Tester::default().test_minimal(sample)
}

#[derive(Debug)]
enum CompletenessCheckerMode {
    Complete,
    Minimal,
}

#[derive(Debug)]
#[must_use = "must be put back"]
struct Checkpoint {
    prev_config: &'static ConfigMetadata,
    prev_path: Option<String>,
}

#[derive(Debug)]
struct CompletenessChecker<'a> {
    mode: CompletenessCheckerMode,
    current_path: String,
    sample: &'a WithOrigin,
    config: &'static ConfigMetadata,
    found_params: HashMap<String, RustType>,
}

impl<'a> CompletenessChecker<'a> {
    fn new(
        mode: CompletenessCheckerMode,
        sample: &'a WithOrigin,
        config: &'static ConfigMetadata,
        config_prefix: &str,
    ) -> Self {
        Self {
            mode,
            current_path: config_prefix.to_owned(),
            sample,
            config,
            found_params: HashMap::new(),
        }
    }

    fn check_param(&mut self, param: &ParamMetadata) {
        let param_path = Pointer(&self.current_path).join(param.name);
        let should_add_param = match self.mode {
            CompletenessCheckerMode::Complete => self.sample.get(Pointer(&param_path)).is_none(),
            CompletenessCheckerMode::Minimal => {
                param.default_value.is_some() && self.sample.get(Pointer(&param_path)).is_some()
            }
        };

        if should_add_param {
            self.found_params.insert(param_path, param.rust_type);
        }
    }

    fn insert_param(&mut self, param: &ParamMetadata) {
        let param_path = Pointer(&self.current_path).join(param.name);
        self.found_params.insert(param_path, param.rust_type);
    }

    fn push_config(&mut self, config_meta: &NestedConfigMetadata) -> Checkpoint {
        let prev_config = mem::replace(&mut self.config, config_meta.meta);
        let prev_path = if config_meta.name.is_empty() {
            None
        } else {
            let nested_path = Pointer(&self.current_path).join(config_meta.name);
            Some(mem::replace(&mut self.current_path, nested_path))
        };
        Checkpoint {
            prev_config,
            prev_path,
        }
    }

    fn pop_config(&mut self, checkpoint: Checkpoint) {
        self.config = checkpoint.prev_config;
        if let Some(path) = checkpoint.prev_path {
            self.current_path = path;
        }
    }

    fn collect_all_params(&mut self) {
        if let Some(tag) = &self.config.tag {
            // Only report the tag param as missing, since all other params can be legitimately absent depending on its value.
            self.insert_param(tag.param);
            return;
        }

        for param in self.config.params {
            self.insert_param(param);
        }
        for config_meta in self.config.nested_configs {
            let checkpoint = self.push_config(config_meta);
            self.collect_all_params();
            self.pop_config(checkpoint);
        }
    }
}

impl ConfigVisitor for CompletenessChecker<'_> {
    fn visit_tag(&mut self, _variant_index: usize) {
        let param = self.config.tag.unwrap().param;
        self.check_param(param);
    }

    fn visit_param(&mut self, param_index: usize, _value: &dyn any::Any) {
        let param = &self.config.params[param_index];
        self.check_param(param);
    }

    fn visit_nested_config(&mut self, config_index: usize, config: &dyn VisitConfig) {
        let config_meta = &self.config.nested_configs[config_index];
        let checkpoint = self.push_config(config_meta);
        config.visit_config(self);
        self.pop_config(checkpoint);
    }

    fn visit_nested_opt_config(&mut self, config_index: usize, config: Option<&dyn VisitConfig>) {
        if let Some(config) = config {
            self.visit_nested_config(config_index, config);
        } else if matches!(self.mode, CompletenessCheckerMode::Complete) {
            let config_meta = &self.config.nested_configs[config_index];
            let checkpoint = self.push_config(config_meta);
            self.collect_all_params();
            self.pop_config(checkpoint);
        }
    }
}

#[derive(Debug)]
struct TesterData {
    de_options: DeserializerOptions,
    schema: ConfigSchema,
    env_guard: MockEnvGuard,
}

#[derive(Debug)]
enum TesterDataGoat<'a> {
    Owned(TesterData),
    Borrowed(&'a mut TesterData),
}

impl AsRef<TesterData> for TesterDataGoat<'_> {
    fn as_ref(&self) -> &TesterData {
        match self {
            Self::Owned(data) => data,
            Self::Borrowed(data) => data,
        }
    }
}

impl AsMut<TesterData> for TesterDataGoat<'_> {
    fn as_mut(&mut self) -> &mut TesterData {
        match self {
            Self::Owned(data) => data,
            Self::Borrowed(data) => data,
        }
    }
}

/// Test case builder that allows configuring deserialization options etc.
///
/// Compared to [`test()`] / [`test_complete()`] methods, `Tester` has more control over deserialization options.
/// It also allows to test a [`ConfigSchema`] with multiple configs.
///
/// # Examples
///
/// ```
/// use smart_config::{testing::Tester, ConfigSchema};
/// # use smart_config::{DescribeConfig, DeserializeConfig};
///
/// // Assume the following configs and schema are defined.
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(default, alias = "flag")]
///     boolean: bool,
/// }
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct OtherConfig {
///     str: Option<String>,
/// }
///
/// fn config_schema() -> ConfigSchema {
///     let mut schema = ConfigSchema::new(&TestConfig::DESCRIPTION, "test");
///     schema
///         .insert(&OtherConfig::DESCRIPTION, "other")
///         .unwrap();
///     schema
/// }
///
/// // Set the tester (can be shared across tests).
/// let schema: ConfigSchema = config_schema();
/// let mut tester = Tester::new(schema);
/// // Set shared deserialization options...
/// tester.coerce_serde_enums().coerce_variant_names();
///
/// let sample = smart_config::config!("test.flag": true, "other.str": "?");
/// let config: TestConfig = tester.for_config().test_complete(sample.clone())?;
/// assert!(config.boolean);
/// let config: OtherConfig = tester.for_config().test_complete(sample)?;
/// assert_eq!(config.str.unwrap(), "?");
/// # anyhow::Ok(())
/// ```
#[derive(Debug)]
pub struct Tester<'a, C> {
    data: TesterDataGoat<'a>,
    _config: PhantomData<C>,
}

impl<C: DeserializeConfig + VisitConfig> Default for Tester<'static, C> {
    fn default() -> Self {
        Self {
            data: TesterDataGoat::Owned(TesterData {
                de_options: DeserializerOptions::default(),
                schema: ConfigSchema::new(&C::DESCRIPTION, ""),
                env_guard: MockEnvGuard::default(),
            }),
            _config: PhantomData,
        }
    }
}

impl Default for Tester<'static, ()> {
    fn default() -> Self {
        Self {
            data: TesterDataGoat::Owned(TesterData {
                de_options: DeserializerOptions::default(),
                schema: ConfigSchema::default(),
                env_guard: MockEnvGuard::default(),
            }),
            _config: PhantomData,
        }
    }
}

impl Tester<'_, ()> {
    /// Creates a tester with the specified schema.
    pub fn new(schema: ConfigSchema) -> Self {
        Self {
            data: TesterDataGoat::Owned(TesterData {
                de_options: DeserializerOptions::default(),
                schema,
                env_guard: MockEnvGuard::default(),
            }),
            _config: PhantomData,
        }
    }

    /// Specializes this tester for a config.
    ///
    /// # Panics
    ///
    /// Panics if the config is not contained in the schema, or is contained at multiple locations.
    pub fn for_config<C: DeserializeConfig + VisitConfig>(&mut self) -> Tester<'_, C> {
        // Check that there's a single config of the specified type
        self.data.as_ref().schema.single(&C::DESCRIPTION).unwrap();
        Tester {
            data: TesterDataGoat::Borrowed(self.data.as_mut()),
            _config: PhantomData,
        }
    }

    /// Similar to [`Self::for_config()`], but inserts the specified config into the schema rather
    /// than expecting it to be present.
    ///
    /// # Panics
    ///
    /// Panics if the config cannot be inserted into the schema.
    pub fn insert<C: DeserializeConfig + VisitConfig>(
        &mut self,
        prefix: &'static str,
    ) -> Tester<'_, C> {
        self.data
            .as_mut()
            .schema
            .insert(&C::DESCRIPTION, prefix)
            .expect("failed inserting config into schema");
        Tester {
            data: TesterDataGoat::Borrowed(self.data.as_mut()),
            _config: PhantomData,
        }
    }
}

impl<C> Tester<'_, C> {
    /// Enables coercion of enum variant names.
    pub fn coerce_variant_names(&mut self) -> &mut Self {
        self.data.as_mut().de_options.coerce_variant_names = true;
        self
    }

    /// Enables coercion of serde-style enums.
    pub fn coerce_serde_enums(&mut self) -> &mut Self {
        self.data.as_mut().schema.coerce_serde_enums(true);
        self
    }

    /// Sets mock environment variables that will be recognized by [`Environment`](crate::Environment)
    /// and [`Env`](crate::fallback::Env) fallbacks.
    ///
    /// Beware that env variable overrides are thread-local; for this reason, `Tester` is not `Send` (cannot be sent to another thread).
    pub fn set_env(&mut self, var_name: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.data
            .as_mut()
            .env_guard
            .set_env(var_name.into(), value.into());
        self
    }

    /// Creates an empty repository based on the tester schema and the deserialization options.
    pub fn new_repository(&self) -> ConfigRepository<'_> {
        let data = self.data.as_ref();
        let mut repo = ConfigRepository::new(&data.schema);
        *repo.deserializer_options() = data.de_options.clone();
        repo
    }
}

impl<C: DeserializeConfig + VisitConfig> Tester<'_, C> {
    /// Tests config deserialization from the provided `sample`. Takes into account param aliases,
    /// performs `sample` preprocessing etc.
    ///
    /// # Errors
    ///
    /// Propagates parsing errors, which allows testing negative cases.
    ///
    /// # Examples
    ///
    /// See [`test()`] for the examples of usage.
    #[allow(clippy::missing_panics_doc)] // can only panic if the config is recursively defined, which is impossible
    pub fn test(&self, sample: impl ConfigSource) -> Result<C, ParseErrors> {
        let repo = self.new_repository();
        repo.with(sample).single::<C>().unwrap().parse()
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
    /// See [`test_complete()`] for the examples of usage.
    #[track_caller]
    pub fn test_complete(&self, sample: impl ConfigSource) -> Result<C, ParseErrors> {
        let repo = self.new_repository().with(sample);
        let (missing_params, config) =
            Self::test_with_checker(&repo, CompletenessCheckerMode::Complete)?;
        assert!(
            missing_params.is_empty(),
            "The provided sample is incomplete; missing params: {missing_params:?}"
        );
        Ok(config)
    }

    fn test_with_checker(
        repo: &ConfigRepository<'_>,
        mode: CompletenessCheckerMode,
    ) -> Result<(HashMap<String, RustType>, C), ParseErrors> {
        let config_ref = repo.single::<C>().unwrap();
        let config_prefix = config_ref.config().prefix();
        let config = config_ref.parse()?;
        let mut visitor =
            CompletenessChecker::new(mode, repo.merged(), &C::DESCRIPTION, config_prefix);
        config.visit_config(&mut visitor);
        let CompletenessChecker { found_params, .. } = visitor;
        Ok((found_params, config))
    }

    /// Tests config deserialization ensuring that only the required config params are present in `sample`.
    ///
    /// # Panics
    ///
    /// Panics if the `sample` contains params with default values. The config message
    /// will contain paths to the redundant params.
    ///
    /// # Errors
    ///
    /// Propagates parsing errors, which allows testing negative cases.
    ///
    /// # Examples
    ///
    /// See [`test_minimal()`] for the examples of usage.
    #[track_caller]
    pub fn test_minimal(&self, sample: impl ConfigSource) -> Result<C, ParseErrors> {
        let repo = self.new_repository().with(sample);
        let (redundant_params, config) =
            Self::test_with_checker(&repo, CompletenessCheckerMode::Minimal)?;
        assert!(
            redundant_params.is_empty(),
            "The provided sample is not minimal; params with default values: {redundant_params:?}"
        );
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use smart_config_derive::DescribeConfig;

    use super::*;
    use crate::{
        Environment, Json, config,
        testonly::{CompoundConfig, DefaultingConfig, EnumConfig, NestedConfig, SimpleEnum},
    };

    #[test]
    fn testing_config() {
        let config = test::<DefaultingConfig>(Json::empty("test.json")).unwrap();
        assert_eq!(config, DefaultingConfig::default());

        let config = test_minimal::<DefaultingConfig>(Json::empty("test.json")).unwrap();
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
        let json = config!("renamed": "first", "nested.renamed": "second");
        test_complete::<CompoundConfig>(json).unwrap();
    }

    #[should_panic(expected = "sample is not minimal")]
    #[test]
    fn panicking_on_redundant_sample() {
        let json = config!("renamed": "first", "other_int": 23);
        test_minimal::<NestedConfig>(json).unwrap();
    }

    #[test]
    fn minimal_testing() {
        let json = config!("renamed": "first", "nested.renamed": "second");
        let config: CompoundConfig = test_minimal(json).unwrap();
        assert_eq!(config.flat.simple_enum, SimpleEnum::First);
        assert_eq!(config.nested.simple_enum, SimpleEnum::Second);
        assert!(config.nested_opt.is_none());
        assert_eq!(config.nested_default, NestedConfig::default_nested());
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

    #[test]
    fn complete_testing_for_enum_configs() {
        let json = config!("type": "first");
        let config = test_complete::<EnumConfig>(json).unwrap();
        assert_eq!(config, EnumConfig::First);

        let json = config!("type": "Fields", "string": "!", "flag": false, "set": [1, 2]);
        let config = test_complete::<EnumConfig>(json).unwrap();
        assert_eq!(
            config,
            EnumConfig::WithFields {
                string: Some("!".to_owned()),
                flag: false,
                set: HashSet::from([1, 2]),
            }
        );
    }

    #[should_panic(expected = "missing params")]
    #[test]
    fn incomplete_enum_config() {
        let json = config!("type": "Fields");
        test_complete::<EnumConfig>(json).unwrap();
    }

    #[should_panic(expected = "opt.nested_opt.other_int")]
    #[test]
    fn panicking_on_incomplete_sample_with_optional_nested_config() {
        #[derive(Debug, DescribeConfig, DeserializeConfig)]
        #[config(crate = crate)]
        struct TestConfig {
            required: u32,
            #[config(nest)]
            opt: Option<CompoundConfig>,
        }

        let json = config!("required": 42);
        test_complete::<TestConfig>(json).ok();
    }
}
