use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
enum TestConfig {
    V0,
    V1 { value: u64 },
}

fn main() {}
