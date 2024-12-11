use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct NestedConfig {
    str: String,
}

#[derive(DescribeConfig)]
struct TestConfig {
    field: u64,
    #[config(nest, alias = "field")]
    nested: NestedConfig,
}

fn main() {}
