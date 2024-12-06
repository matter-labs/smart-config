use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
#[config(tag = "version")]
enum TestConfig {
    V0,
    #[config(alias = "V0")]
    V1 { value: u64 },
}

#[derive(DescribeConfig)]
#[config(tag = "version")]
enum OtherConfig {
    #[config(rename = "V1")]
    V0,
    V1 { value: u64 },
}

fn main() {}
