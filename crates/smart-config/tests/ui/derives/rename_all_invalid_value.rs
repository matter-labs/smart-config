use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
#[config(tag = "version", rename_all = "unknown")]
enum TestConfig {
    V0,
    V1 { int: u64 },
}

fn main() {}
