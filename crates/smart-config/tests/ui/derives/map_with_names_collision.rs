use std::collections::HashMap;

use smart_config::{de, DescribeConfig};

#[derive(DescribeConfig)]
struct TestConfig {
    #[config(with = de::Entries::WELL_KNOWN.named("val", "val"))]
    map: HashMap<u64, String>,
}

fn main() {}
