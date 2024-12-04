use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
#[config(tag = "version")]
enum TestConfig {
    V0,
    V1(u64, String),
}

fn main() {}
