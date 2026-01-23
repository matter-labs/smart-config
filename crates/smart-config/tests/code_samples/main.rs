//! Code samples used in the extra docs. Here, we test that they match.

use smart_config::{Environment, Yaml, testing};

use crate::test_config::TestConfig;

mod test_config;

#[test]
fn test_config_may_be_parsed_from_yaml() {
    let yaml = serde_yaml::from_str(include_str!("test.yml")).unwrap();
    let yaml = Yaml::new("test.yml", yaml).unwrap();
    testing::test_complete::<TestConfig>(yaml).unwrap();
}

#[test]
fn test_config_may_be_parsed_from_env() {
    let env = include_str!("test.env");
    let mut env = Environment::from_dotenv("test.env", env)
        .unwrap()
        .strip_prefix("APP_");
    env.coerce_json().unwrap();
    testing::test_complete::<TestConfig>(env).unwrap();
}
