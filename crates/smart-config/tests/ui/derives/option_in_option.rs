use smart_config::DescribeConfig;

#[derive(Debug, DescribeConfig)]
struct TestConfig {
    bogus: Option<Option<u64>>,
}

fn main() {}
