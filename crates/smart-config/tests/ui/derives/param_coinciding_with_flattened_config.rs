use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct NestedConfig {
    str: String,
}

#[derive(DescribeConfig)]
struct FlattenedConfig {
    #[config(nest)]
    nested: NestedConfig,
}

#[derive(DescribeConfig)]
struct TestConfig {
    nested: u64,
    #[config(flatten)]
    flat: FlattenedConfig,
}

fn main() {}
