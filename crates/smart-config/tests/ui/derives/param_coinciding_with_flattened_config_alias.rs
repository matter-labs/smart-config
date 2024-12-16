use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct NestedConfig {
    str: String,
}

#[derive(DescribeConfig)]
struct FlattenedConfig {
    #[config(nest, alias = "value")]
    nested: NestedConfig,
}

#[derive(DescribeConfig)]
struct TestConfig {
    value: u64,
    #[config(flatten)]
    flat: FlattenedConfig,
}

fn main() {}
