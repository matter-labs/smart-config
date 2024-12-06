use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct TestConfig {
    #[config(alias = "what?")]
    field: u64,
}

fn main() {}
