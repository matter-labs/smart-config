use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
#[config(derive(Default))]
struct TestConfig {
    field: u64,
}

fn main() {}
