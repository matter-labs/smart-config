use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
#[config(tag = "version", derive(Default))]
enum TestConfig {
    V0,
    #[config(default)]
    V1 { value: u64 },
}

fn main() {}
