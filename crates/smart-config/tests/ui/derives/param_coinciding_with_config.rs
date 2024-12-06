use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct NestedConfig {
    str: String,
}

#[derive(DescribeConfig)]
struct TestConfig {
    #[config(alias = "nested")]
    field: u64,
    #[config(nest)]
    nested: NestedConfig,
}

fn main() {}
