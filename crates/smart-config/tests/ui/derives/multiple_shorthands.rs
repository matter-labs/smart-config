use smart_config::DescribeConfig;

#[derive(DescribeConfig)]
#[config(tag = "version")]
enum TestConfig {
    V0 {
        #[config(shorthand)]
        value: u64,
    },
    V1 {
        #[config(shorthand)]
        str: String,
    },
}

fn main() {}
