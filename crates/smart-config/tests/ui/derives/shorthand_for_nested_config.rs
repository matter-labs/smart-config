use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct NestedConfig {
    value: u64,
}

#[derive(DescribeConfig)]
#[config(tag = "version")]
enum TestConfig {
    V0 {
        #[config(nest, shorthand)]
        nested: NestedConfig,
    },
}

fn main() {}
