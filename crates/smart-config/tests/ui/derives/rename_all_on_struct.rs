use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
#[config(rename_all = "camelCase")]
struct TestConfig {
    int: u64,
}

fn main() {}
