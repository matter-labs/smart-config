use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct NestedConfig {
    field: u64,
}

#[derive(DescribeConfig)]
struct TestConfig {
    #[config(nest, with = ())]
    bogus: NestedConfig,
}

fn main() {}
