use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct TestConfig {
    #[config(nest)]
    field: u64,
}

#[derive(DescribeConfig)]
struct OtherConfig {
    #[config(flatten)]
    field: u64,
}

fn main() {}
