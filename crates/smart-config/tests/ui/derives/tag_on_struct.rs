use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
#[config(tag = "kind")]
struct TestConfig {
    field: u64,
}

fn main() {}
