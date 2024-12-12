use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct TestConfig {
    field: u64,
}

#[derive(DescribeConfig)]
struct OtherConfig {
    #[config(flatten)]
    field: Option<TestConfig>,
}

fn main() {}
