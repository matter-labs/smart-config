#[doc(hidden)] // used in the derive macro
pub use once_cell::sync::Lazy;
pub use smart_config_derive::{DescribeConfig, DeserializeConfig};

use self::metadata::ConfigMetadata;
pub use self::{
    de::ValueDeserializer,
    error::{ParseError, ParseErrors},
    source::{ConfigRepository, ConfigSource, Environment, Json, KeyValueMap, Yaml},
};

mod de;
mod error;
pub mod metadata;
mod schema;
mod source;
#[cfg(test)]
mod testonly;
pub mod value;

/// Describes a configuration (i.e., a group of related parameters).
pub trait DescribeConfig: 'static {
    /// Provides the description.
    fn describe_config() -> &'static ConfigMetadata;
}

pub trait DeserializeConfig: DescribeConfig + Sized {
    fn deserialize_config_full(
        deserializer: ValueDeserializer<'_>,
        errors: &mut ParseErrors,
    ) -> Option<Self>;

    fn deserialize_config(deserializer: ValueDeserializer<'_>) -> Result<Self, ParseErrors> {
        let mut errors = ParseErrors::default();
        if let Some(config) = Self::deserialize_config_full(deserializer, &mut errors) {
            Ok(config)
        } else {
            Err(errors)
        }
    }
}
