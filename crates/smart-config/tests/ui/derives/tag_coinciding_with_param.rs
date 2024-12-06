use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
#[config(tag = "version")]
enum TestConfig {
    V0,
    V1 { version: u32 },
}

fn main() {}
