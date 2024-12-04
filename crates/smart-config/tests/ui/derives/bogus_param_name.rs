use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
struct TestConfig {
    #[config(rename = "what?")]
    field: u64,
}

fn main() {}
