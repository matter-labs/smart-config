use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct TestConfig {
    #[config(deprecated = "..what..")]
    field: u64,
}

fn main() {}
