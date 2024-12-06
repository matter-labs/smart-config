use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Duration,
};

use clap::Parser;
use serde::Deserialize;
use smart_config::{
    de,
    metadata::{SizeUnit, TimeUnit},
    ByteSize, ConfigRepository, ConfigSchema, DescribeConfig, DeserializeConfig, Environment, Json,
    Yaml,
};
use smart_config_commands::{print_debug, print_help};

/// Configuration with type params of several types.
#[derive(Debug, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct TestConfig {
    /// Port to bind to.
    #[config(default_t = 8080, alias = "bind_to")]
    pub port: u16,
    /// Application name.
    #[config(default_t = "app".into())]
    pub app_name: String,
    #[config(default_t = Duration::from_millis(500))]
    pub poll_latency: Duration,
    /// Should be greater than 0.
    #[config(default)]
    pub scaling_factor: Option<f32>,
    /// Paths to key directories.
    #[config(default, alias = "dirs", with = de::Delimited(":"))]
    pub dir_paths: HashSet<PathBuf>,
    /// Timeout for some operation.
    #[config(default_t = Duration::from_secs(60), with = TimeUnit::Seconds)]
    pub timeout_sec: Duration,
    /// In-memory cache size.
    #[config(default_t = ByteSize::new(16, SizeUnit::MiB))]
    pub cache_size: ByteSize,
    #[config(nest)]
    pub nested: NestedConfig,
}

#[derive(Debug, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct NestedConfig {
    /// Whether to exit the application on error.
    #[config(default_t = true)]
    pub exit_on_error: bool,
    /// Complex parameter deserialized from an object.
    #[config(default)]
    pub complex: ComplexParam,
    #[config(default, alias = "timeouts", with = de::Delimited(","))]
    pub more_timeouts: Vec<Duration>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ComplexParam {
    #[serde(default)]
    pub array: Vec<u32>,
    #[serde(default)]
    pub map: HashMap<String, u32>,
}

impl de::WellKnown for ComplexParam {
    type Deserializer = de::Serde![object];
    const DE: Self::Deserializer = de::Serde![object];
}

const JSON: &str = r#"
{
  "test": {
    "scaling_factor": 4.2,
    "cache_size": { "kb": 256 },
    "nested": {
      "exit_on_error": false
    }
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
"#;

fn create_mock_repo(schema: &ConfigSchema, bogus: bool) -> ConfigRepository<'_> {
    let json = serde_json::from_str(JSON).unwrap();
    let json = Json::new("/config/base.json", json);
    let yaml = serde_yaml::from_str(YAML).unwrap();
    let yaml = Yaml::new("/config/test.yml", yaml).unwrap();
    let env_vars = Environment::from_iter(
        "APP_",
        [
            ("APP_TEST_APP_NAME", "test"),
            ("APP_TEST_DIRS", "/usr/bin:usr/local/bin"),
            ("APP_TEST_CACHE_SIZE", "128 MiB"),
        ],
    );
    let mut repo = ConfigRepository::new(schema)
        .with(json)
        .with(yaml)
        .with(env_vars);

    if bogus {
        let bogus_vars = Environment::from_iter(
            "BOGUS_",
            [
                ("BOGUS_TEST_TIMEOUT_SEC", "what?"),
                ("BOGUS_TEST_NESTED_TIMEOUTS", "nope,124us"),
                ("BOGUS_TEST_NESTED_COMPLEX", r#"{ "array": [1, true] }"#),
                ("BOGUS_TEST_CACHE_SIZE", "128 MiBis"),
            ],
        );
        repo = repo.with(bogus_vars);
    }
    repo
}

#[derive(Debug, Parser)]
enum Cli {
    /// Prints configuration help.
    Print,
    /// Debugs configuration values.
    Debug {
        /// Whether to inject incorrect config values.
        #[arg(long)]
        bogus: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    let mut schema = ConfigSchema::default();
    schema.insert::<TestConfig>("test").unwrap();

    match cli {
        Cli::Print => {
            print_help(&schema, |_| true).unwrap();
        }
        Cli::Debug { bogus } => {
            let repo = create_mock_repo(&schema, bogus);
            print_debug(&repo).unwrap();
        }
    }
}
