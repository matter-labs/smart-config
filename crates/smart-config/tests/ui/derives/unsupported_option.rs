use serde::{Deserialize, Serialize};
use smart_config::{de, DescribeConfig};

#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
struct CustomParam(u64);

impl de::WellKnown for CustomParam {
    type Deserializer = de::Serde![int];
    const DE: Self::Deserializer = de::Serde![int];
}

#[derive(Debug, DescribeConfig)]
struct TestConfig {
    optional: Option<CustomParam>,
}

fn main() {}
