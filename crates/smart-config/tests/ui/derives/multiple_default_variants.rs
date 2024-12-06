use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
#[config(tag = "version")]
enum TestConfig {
    #[config(default)]
    V0,
    #[config(default)]
    V1 { value: u64 },
}

fn main() {}
