use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
    path::PathBuf,
    time::Duration,
};

use smart_config::{
    ByteSize, DescribeConfig, DeserializeConfig, EtherAmount, ExampleConfig, de, fallback,
    metadata::{SizeUnit, TimeUnit},
    pat::{LazyRegex, lazy_regex},
    value::SecretString,
};

static APP_NAME_REGEX: LazyRegex = lazy_regex!(r"^[a-z][-a-z0-9]*$");

/// Configuration with type params of several types.
#[derive(Debug, DescribeConfig, DeserializeConfig, ExampleConfig)]
pub(crate) struct TestConfig {
    /// Application name.
    // We validate that the name conforms to the regex defined above.
    #[config(default_t = "app".into(), validate(APP_NAME_REGEX))]
    pub app_name: String,
    /// Port to bind to.
    // `deprecated` allows to specify an alias that will be highlighted as deprecated
    // in docs (e.g., `smart-config-commands` output).
    #[config(example = 8080, deprecated = "bind_to")]
    pub port: u16,
    /// Should be greater than 0.
    // When a range is used as a validation, it's checked whether tha value
    // is in the range.
    #[config(default, validate(0.0..=10.0), example = Some(0.5))]
    pub scaling_factor: Option<f32>,
    /// Directory for temporary stuff.
    // `fallback` allows to read the value from another source that does not fit
    // into the hierarchical config schema.
    #[config(default_t = "/tmp".into(), fallback = &fallback::Env("TMPDIR"))]
    pub temp_dir: PathBuf,
    /// Paths to key directories.
    // `Delimited` deserializer allows to read paths both from an array
    // and from a ':'-delimited string. This is useful if the config
    // is parsed from env vars.
    #[config(default, alias = "dirs", with = de::Delimited::new(":"))]
    #[config(example = HashSet::from_iter(["./local".into()]))]
    pub dir_paths: HashSet<PathBuf>,
    /// In-memory cache size.
    // The default deserializer for `ByteSize`s, `Duration`s and `EtherUnit`s
    // accepts a numeric value together with a unit, e.g. '16 MB'. In case of durations
    // and `EtherUnit`, the value may be a decimal, like '0.5 days' or '0.02 ether'.
    //
    // Note that `deprecated` in this case is a relative path, not just a name.
    #[config(default_t = 16 * SizeUnit::MiB, deprecated = ".experimental.cache_size")]
    pub cache_size: ByteSize,
    /// Timeout for some operation.
    // Using a specific unit as a deserializer will accept numeric values
    // measured in this unit. This is not recommended to use; the default deserializer
    // for united values automatically handles suffixed values.
    #[config(default_t = 1 * TimeUnit::Minutes, with = TimeUnit::Seconds)]
    pub timeout_sec: Duration,
    // Nested configuration.
    #[config(nest)]
    pub experimental: ExperimentalConfig,
}

#[derive(Debug, DescribeConfig, DeserializeConfig, ExampleConfig)]
#[config(derive(Default))]
pub(crate) struct ExperimentalConfig {
    /// Secret string value.
    #[config(example = Some("correct horse battery staple".into()))]
    pub api_key: Option<SecretString>,
    /// Can be deserialized either from a map or an array of tuples.
    #[config(default, with = de::Entries::WELL_KNOWN.named("method", "rps"))]
    #[config(example = HashMap::from_iter([
        ("eth_call".into(), NonZeroU32::new(100).unwrap()),
        ("eth_blockNumber".into(), NonZeroU32::new(1).unwrap()),
    ]))]
    pub method_limits: HashMap<String, NonZeroU32>,
    /// Can be deserialized from a string.
    #[config(
        default,
        with = de::Entries::WELL_KNOWN.delimited(
            lazy_regex!(ref r"\s*[,\n]\s*"),
            lazy_regex!(ref r"\s*=\s*"),
        )
    )]
    pub balances: HashMap<u64, EtherAmount>,
}
