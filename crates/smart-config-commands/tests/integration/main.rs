//! **Important.** The generated snapshots are specific to stable Rust; nightly Rust provides better spanning for types
//! (`Option<u64>` instead of `Option`).

use anstream::AutoStream;
use smart_config::{ConfigSchema, DescribeConfig};
use smart_config_commands::Printer;

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
