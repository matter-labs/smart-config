use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
#[config(tag = "version", derive(Default))]
enum TestConfig {
    V0,
    V1 { value: u64 },
}

fn main() {}
