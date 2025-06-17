use std::process;

use anstream::AutoStream;
use anstyle::{AnsiColor, Color, Style};
use clap::{Parser, ValueEnum};
use smart_config::{
    ConfigRepository, ConfigSchema, DescribeConfig, Environment, ExampleConfig, Json,
    SerializerOptions, Yaml,
};
use smart_config_commands::{ParamRef, Printer};

use crate::configs::{create_mock_repo, TestConfig};

#[path = "../../tests/integration/configs.rs"]
mod configs;

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
        /// Use example config instead of parsing sources.
        #[arg(long)]
        example: bool,
        /// Do not output default param values.
        #[arg(long)]
        diff: bool,
        /// Serialization format.
        #[arg(long, value_enum, default_value_t = SerializationFormat::Yaml)]
        format: SerializationFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SerializationFormat {
    Json,
    Yaml,
    Env,
}

const ERROR: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));

fn main() {
    let cli = Cli::parse();
    let schema = ConfigSchema::new(&TestConfig::DESCRIPTION, "test");

    match cli {
        Cli::Print { filter } => {
            let filter = |param_ref: ParamRef<'_>| {
                filter.as_ref().map_or(true, |needle| {
                    param_ref.all_paths().any(|(path, _)| path.contains(needle))
                })
            };
            Printer::stderr().print_help(&schema, filter).unwrap();
        }
        Cli::Debug { bogus, filter } => {
            let repo = create_mock_repo(&schema, bogus);
            let filter = |param_ref: ParamRef<'_>| {
                filter.as_ref().map_or(true, |needle| {
                    param_ref.all_paths().any(|(path, _)| path.contains(needle))
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
        Cli::Serialize {
            example,
            diff,
            format,
        } => {
            let mut options = if diff {
                SerializerOptions::diff_with_default()
            } else {
                SerializerOptions::default()
            };
            options = options.flat(matches!(format, SerializationFormat::Env));

            let (json, original_config) = if example {
                let example_config = TestConfig::example_config();
                let json = options.serialize(&example_config);
                // Need to wrap the serialized value with the 'test' prefix so that it corresponds to the schema.
                (serde_json::json!({ "test": json }), example_config)
            } else {
                let repo = create_mock_repo(&schema, false);
                let original_config: TestConfig = repo.single().unwrap().parse().unwrap();
                (repo.canonicalize(&options).unwrap().into(), original_config)
            };

            let mut buffer = vec![];
            let restored_repo = match format {
                SerializationFormat::Json => {
                    Printer::stderr().print_json(&json).unwrap();

                    // Parse the produced JSON back and check that it describes the same config.
                    Printer::custom(AutoStream::never(&mut buffer))
                        .print_json(&json)
                        .unwrap();
                    let deserialized = serde_json::from_slice(&buffer).unwrap();
                    let source = Json::new("deserialized.json", deserialized);
                    ConfigRepository::new(&schema).with(source)
                }
                SerializationFormat::Yaml => {
                    Printer::stderr().print_yaml(&json).unwrap();

                    Printer::custom(AutoStream::never(&mut buffer))
                        .print_yaml(&json)
                        .unwrap();
                    let deserialized = serde_yaml::from_slice(&buffer).unwrap();
                    let source = Yaml::new("deserialized.yaml", deserialized).unwrap();
                    ConfigRepository::new(&schema).with(source)
                }
                SerializationFormat::Env => {
                    let env =
                        Environment::convert_flat_params(json.as_object().unwrap(), "APP_").into();
                    Printer::stderr().print_yaml(&env).unwrap();
                    let env = env.as_object().unwrap().iter().map(|(name, value)| {
                        let value = match value {
                            serde_json::Value::String(s) => s.clone(),
                            _ => value.to_string(),
                        };
                        (name.as_str(), value)
                    });
                    let mut env = Environment::from_iter("APP_", env);
                    env.coerce_json().unwrap();
                    ConfigRepository::new(&schema).with(env)
                }
            };
            let restored_config: TestConfig = restored_repo.single().unwrap().parse().unwrap();
            assert_eq!(original_config, restored_config);
        }
    }
}
