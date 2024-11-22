#[doc(hidden)] // used in the derive macro
pub use once_cell::sync::Lazy;
pub use smart_config_derive::{DescribeConfig, DeserializeConfig};

use self::metadata::ConfigMetadata;
pub use self::{
    de::ValueDeserializer,
    de_new::{DeserializeConfig, DeserializeContext, DeserializeParam},
    error::{ParseError, ParseErrors},
    source::{ConfigRepository, ConfigSource, Environment, Json, KeyValueMap, Yaml},
};

mod de;
mod de_new;
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
