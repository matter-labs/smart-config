use std::{
    collections::{HashMap, HashSet},
    fmt,
    num::NonZeroU32,
    path::PathBuf,
    process,
    time::Duration,
};

use anstream::AutoStream;
use anstyle::{AnsiColor, Color, Style};
use clap::{Parser, ValueEnum};
use primitive_types::{H160 as Address, H256, U256};
use serde::{Deserialize, Serialize};
use smart_config::{
    de, fallback,
    metadata::{SizeUnit, TimeUnit},
    validation::NotEmpty,
    value::SecretString,
    visit::serialize_to_json,
    ByteSize, ConfigRepository, ConfigSchema, DescribeConfig, DeserializeConfig, Environment,
    ExampleConfig, Json, Prefixed, Yaml,
};
use smart_config_commands::{ParamRef, Printer};

/// Configuration with type params of several types.
#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig, ExampleConfig)]
pub struct TestConfig {
    /// Port to bind to.
    #[config(example = 8080, alias = "bind_to")]
    pub port: u16,
    /// Application name.
    #[config(default_t = "app".into(), validate(NotEmpty))]
    pub app_name: String,
    #[config(default_t = Duration::from_millis(500))]
    pub poll_latency: Duration,
    /// Should be greater than 0.
    #[config(default, validate(0.0..=10.0), example = Some(0.5))]
    pub scaling_factor: Option<f32>,
    /// Directory for temporary stuff.
    #[config(default_t = "/tmp".into(), fallback = &fallback::Env("TMPDIR"))]
    pub temp_dir: PathBuf,
    /// Paths to key directories.
    #[config(default, alias = "dirs", with = de::Delimited(":"))]
    #[config(example = ["./local".into()].into())]
    pub dir_paths: HashSet<PathBuf>,
    /// Timeout for some operation.
    #[config(default_t = 1 * TimeUnit::Minutes, with = TimeUnit::Seconds)]
    pub timeout_sec: Duration,
    /// In-memory cache size.
    #[config(default_t = 16 * SizeUnit::MiB)]
    pub cache_size: ByteSize,
    #[config(nest)]
    pub nested: NestedConfig,
    #[config(nest, alias = "funds")]
    pub funding: Option<FundingConfig>,
    /// Required param.
    #[config(example = 42)]
    pub required: u64,
    #[config(nest)]
    pub object_store: ObjectStoreConfig,
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig, ExampleConfig)]
#[config(derive(Default))]
pub struct NestedConfig {
    /// Whether to exit the application on error.
    #[config(default_t = true)]
    pub exit_on_error: bool,
    /// Complex parameter deserialized from an object.
    #[config(default, example = ComplexParam::example())]
    pub complex: ComplexParam,
    #[config(default, alias = "timeouts", with = de::Delimited(","))]
    #[config(example = vec![Duration::from_secs(5)])]
    pub more_timeouts: Vec<Duration>,
    /// Can be deserialized either from a map or an array of tuples.
    #[config(default, with = de::Entries::WELL_KNOWN.named("method", "rps"))]
    #[config(example = HashMap::from([
        ("eth_call".into(), NonZeroU32::new(100).unwrap()),
        ("eth_blockNumber".into(), NonZeroU32::new(1).unwrap()),
    ]))]
    pub method_limits: HashMap<String, NonZeroU32>,
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ComplexParam {
    #[serde(default)]
    pub array: Vec<u32>,
    #[serde(default)]
    pub map: HashMap<String, u32>,
}

impl ComplexParam {
    fn example() -> Self {
        Self {
            array: vec![3, 5],
            map: HashMap::from([("var".into(), 3)]),
        }
    }
}

impl de::WellKnown for ComplexParam {
    type Deserializer = de::Serde![object];
    const DE: Self::Deserializer = de::Serde![object];
}

#[derive(PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretKey(pub H256);

impl fmt::Debug for SecretKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("SecretKey").field(&"_").finish()
    }
}

#[derive(Debug, DescribeConfig, DeserializeConfig, ExampleConfig)]
#[config(validate(
    Self::validate_address,
    "`address` should be non-zero for non-zero `balance`"
))]
pub struct FundingConfig {
    /// Ethereum-like address to fund.
    #[config(default)]
    pub address: Address,
    /// Initial balance for the address.
    #[config(default)]
    pub balance: U256,
    /// Secret string value.
    #[config(example = Some("correct horse battery staple".into()))]
    pub api_key: Option<SecretString>,
    /// Secret key.
    #[config(secret, with = de::Optional(de::Serde![str]))]
    #[config(example = Some(SecretKey(H256::zero())))]
    pub secret_key: Option<SecretKey>,
}

impl PartialEq for FundingConfig {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
            && self.balance == other.balance
            && self.api_key.as_ref().map(SecretString::expose_secret)
                == other.api_key.as_ref().map(SecretString::expose_secret)
            && self.secret_key == other.secret_key
    }
}

impl FundingConfig {
    fn validate_address(&self) -> bool {
        self.balance.is_zero() || !self.address.is_zero()
    }
}

#[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default), tag = "type", rename_all = "snake_case")]
pub enum ObjectStoreConfig {
    /// Stores object locally as files.
    #[config(default)]
    Local {
        /// Path to the root directory.
        #[config(default_t = ".".into())]
        path: PathBuf,
    },
    /// Stores objects in AWS S3.
    S3 {
        bucket_name: String,
        region: Option<String>,
    },
    /// Stores objects in Google Cloud Storage.
    #[config(alias = "google", alias = "google_cloud")]
    Gcs {
        /// Bucket to put objects into.
        bucket_name: String,
    },
}

impl ExampleConfig for ObjectStoreConfig {
    fn example_config() -> Self {
        Self::default()
    }
}

const JSON: &str = r#"
{
  "scaling_factor": 4.2,
  "cache_size": { "kb": 256 },
  "nested": {
    "exit_on_error": false
  },
  "funding": {
    "api_key": "correct horse"
  }
}
"#;

const YAML: &str = r#"
test:
  port: 3000
  poll_latency_ms: 300
  dir_paths:
    - /bin
    - /usr/bin
  nested:
    complex:
      array: [1, 2]
      map:
        value: 25
    method_limits:
      - method: eth_getLogs
        rps: 100
      - method: eth_blockNumber
        rps: 3
  funding:
    address: "0x0000000000000000000000000000000000001234"
    balance: "0x123456"
  object_store:
    type: google
    bucket_name: test-bucket
    region: euw1
"#;

fn create_mock_repo(schema: &ConfigSchema, bogus: bool) -> ConfigRepository<'_> {
    let json = serde_json::from_str(JSON).unwrap();
    let json = Json::new("/config/base.json", json);
    let json = Prefixed::new(json, "test");
    let yaml = serde_yaml::from_str(YAML).unwrap();
    let yaml = Yaml::new("/config/test.yml", yaml).unwrap();

    let mut env_vars = vec![
        ("APP_TEST_APP_NAME", "test"),
        ("APP_TEST_DIRS", "/usr/bin:/usr/local/bin"),
        ("APP_TEST_CACHE_SIZE", "128 MiB"),
        ("APP_TEST_FUNDS_API_KEY", "correct horse battery staple"),
        (
            "APP_TEST_FUNDS_SECRET_KEY",
            "0x000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f",
        ),
    ];
    if !bogus {
        env_vars.push(("APP_TEST_REQUIRED", "123"));
    }
    let env_vars = Environment::from_iter("APP_", env_vars);

    let mut repo = ConfigRepository::new(schema)
        .with(json)
        .with(yaml)
        .with(env_vars);

    if bogus {
        let bogus_vars = Environment::from_iter(
            "BOGUS_",
            [
                ("BOGUS_TEST_APP_NAME", ""),
                ("BOGUS_TEST_TIMEOUT_SEC", "what?"),
                ("BOGUS_TEST_SCALING_FACTOR", "-1"),
                ("BOGUS_TEST_NESTED_TIMEOUTS", "nope,124us"),
                ("BOGUS_TEST_NESTED_COMPLEX", r#"{ "array": [1, true] }"#),
                ("BOGUS_TEST_NESTED_METHOD_LIMITS", r#"{ "eth_getLogs": 0 }"#),
                ("BOGUS_TEST_CACHE_SIZE", "128 MiBis"),
                (
                    "BOGUS_TEST_FUNDS_ADDRESS",
                    "0x0000000000000000000000000000000000000000",
                ),
                ("BOGUS_TEST_OBJECT_STORE_TYPE", "file"),
            ],
        );
        repo = repo.with(bogus_vars);
    }
    repo
}

#[derive(Debug, Parser)]
enum Cli {
    /// Prints configuration help.
    Print {
        /// Filter for param paths.
        filter: Option<String>,
    },
    /// Debugs configuration values.
    Debug {
        /// Whether to inject incorrect config values.
        #[arg(long)]
        bogus: bool,
        /// Filter for param paths.
        filter: Option<String>,
    },
    /// Serializes example config.
    Serialize {
        // FIXME: example config; diff
        /// Serialization format.
        #[arg(long, value_enum, default_value_t = SerializationFormat::Yaml)]
        format: SerializationFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SerializationFormat {
    Json,
    Yaml,
}

const ERROR: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));

fn main() {
    let cli = Cli::parse();
    let schema = ConfigSchema::new(&TestConfig::DESCRIPTION, "test");

    match cli {
        Cli::Print { filter } => {
            let filter = |param_ref: ParamRef<'_>| {
                filter.as_ref().map_or(true, |needle| {
                    param_ref.all_paths().any(|path| path.contains(needle))
                })
            };
            Printer::stderr().print_help(&schema, filter).unwrap();
        }
        Cli::Debug { bogus, filter } => {
            let repo = create_mock_repo(&schema, bogus);
            let filter = |param_ref: ParamRef<'_>| {
                filter.as_ref().map_or(true, |needle| {
                    param_ref.all_paths().any(|path| path.contains(needle))
                })
            };

            let res = Printer::stderr().print_debug(&repo, filter).unwrap();
            if let Err(err) = res {
                anstream::eprintln!(
                    "\n{ERROR}There were errors parsing configuration params:\n{err}{ERROR:#}"
                );
                process::exit(1);
            }
        }
        Cli::Serialize { format } => {
            let repo = create_mock_repo(&schema, false);
            let original_config: TestConfig = repo.single().unwrap().parse().unwrap();
            let canonical_json = repo.canonicalize().unwrap();
            let canonical_json = serde_json::Value::from(canonical_json);

            let mut buffer = vec![];
            let restored_repo = match format {
                SerializationFormat::Json => {
                    Printer::stderr().print_json(&canonical_json).unwrap();

                    // Parse the produced JSON back and check that it describes the same config.
                    Printer::custom(AutoStream::never(&mut buffer))
                        .print_json(&canonical_json)
                        .unwrap();
                    let deserialized = serde_json::from_slice(&buffer).unwrap();
                    let source = Json::new("deserialized.json", deserialized);
                    ConfigRepository::new(&schema).with(source)
                }
                SerializationFormat::Yaml => {
                    Printer::stderr().print_yaml(&canonical_json).unwrap();

                    Printer::custom(AutoStream::never(&mut buffer))
                        .print_yaml(&canonical_json)
                        .unwrap();
                    let deserialized = serde_yaml::from_slice(&buffer).unwrap();
                    let source = Yaml::new("deserialized.yaml", deserialized).unwrap();
                    ConfigRepository::new(&schema).with(source)
                }
            };
            let restored_config: TestConfig = restored_repo.single().unwrap().parse().unwrap();
            assert_eq!(original_config, restored_config);
        }
    }
}
