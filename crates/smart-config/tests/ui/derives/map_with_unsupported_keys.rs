use std::collections::HashMap;

use serde::Deserialize;
use smart_config::{de, DescribeConfig};

#[derive(PartialEq, Eq, Hash, Deserialize)]
struct StructKey {
    int: u64,
    str: String,
}

impl de::WellKnown for StructKey {
    type Deserializer = de::Serde![object];
    const DE: Self::Deserializer = de::Serde![object];
}

#[derive(DescribeConfig)]
struct TestConfig {
    map: HashMap<StructKey, u64>,
}

fn main() {}
