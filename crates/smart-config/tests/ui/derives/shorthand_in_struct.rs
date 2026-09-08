use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct TestConfig {
    #[config(shorthand)]
    value: u64,
}

fn main() {}
