//! **Important.** The generated snapshots are specific to stable Rust; nightly Rust provides better spanning for types
//! (`Option<u64>` instead of `Option`).

use std::fmt;

use anstream::AutoStream;
use smart_config::{ConfigSchema, DescribeConfig, Environment, ExampleConfig, SerializerOptions};
use smart_config_commands::Printer;
use test_casing::{test_casing, Product};

use crate::configs::{create_mock_repo, TestConfig};

mod configs;

#[test]
fn full_config_help() {
    let schema = ConfigSchema::new(&TestConfig::DESCRIPTION, "test");

    let mut buffer = vec![];
    Printer::custom(AutoStream::never(&mut buffer))
        .print_help(&schema, |_| true)
        .unwrap();
    let buffer = String::from_utf8(buffer).unwrap();
    insta::assert_snapshot!("help_full", buffer);
}

#[test]
fn filtered_config_help() {
    let schema = ConfigSchema::new(&TestConfig::DESCRIPTION, "test");

    let mut buffer = vec![];
    Printer::custom(AutoStream::never(&mut buffer))
        .print_help(&schema, |param| {
            param.all_paths().any(|(path, _)| path.contains("fund"))
        })
        .unwrap();
    let buffer = String::from_utf8(buffer).unwrap();
    insta::assert_snapshot!("help_filtered", buffer);
}

#[test]
fn full_config_debug() {
    let schema = ConfigSchema::new(&TestConfig::DESCRIPTION, "test");
    let repo = create_mock_repo(&schema, false);

    let mut buffer = vec![];
    Printer::custom(AutoStream::never(&mut buffer))
        .print_debug(&repo, |_| true)
        .unwrap()
        .unwrap();
    let buffer = String::from_utf8(buffer).unwrap();
    insta::assert_snapshot!("debug_full", buffer);
}

#[test]
fn erroneous_config_debug() {
    let schema = ConfigSchema::new(&TestConfig::DESCRIPTION, "test");
    let repo = create_mock_repo(&schema, true);

    let mut buffer = vec![];
    Printer::custom(AutoStream::never(&mut buffer))
        .print_debug(&repo, |_| true)
        .unwrap()
        .unwrap_err();
    let buffer = String::from_utf8(buffer).unwrap();
    insta::assert_snapshot!("debug_errors", buffer);
}

#[derive(Debug, Clone, Copy)]
enum Format {
    Yaml,
    Json,
    Env,
}

impl fmt::Display for Format {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Yaml => "yaml",
            Self::Json => "json",
            Self::Env => "env",
        })
    }
}

impl Format {
    const ALL: [Self; 3] = [Self::Yaml, Self::Json, Self::Env];
}

#[test_casing(6, Product((Format::ALL, [false, true])))]
#[test]
fn printing_config_example_yaml(format: Format, diff: bool) {
    let example_config = TestConfig::example_config();
    let options = if diff {
        SerializerOptions::diff_with_default()
    } else {
        SerializerOptions::default()
    };
    let options = options.flat(matches!(format, Format::Env));
    let example_json = options.serialize(&example_config);

    let mut buffer = vec![];
    match format {
        Format::Yaml => {
            Printer::custom(AutoStream::never(&mut buffer))
                .print_yaml(&example_json.into())
                .unwrap();
        }
        Format::Json => {
            Printer::custom(AutoStream::never(&mut buffer))
                .print_json(&example_json.into())
                .unwrap();
        }
        Format::Env => {
            let env = Environment::convert_flat_params(&example_json, "APP_");
            Printer::custom(AutoStream::never(&mut buffer))
                .print_yaml(&env.into())
                .unwrap();
        }
    }

    let buffer = String::from_utf8(buffer).unwrap();
    let snapshot_name = format!("example_{}_{format}", if diff { "diff" } else { "full" });
    insta::assert_snapshot!(snapshot_name, buffer);
}
