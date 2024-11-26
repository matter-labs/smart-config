//! `smart-config` – schema-driven layered configuration system with support of multiple configuration formats.
//!
//! # Overview
//!
//! The task solved by the library is merging configuration input from a variety of prioritized [sources](ConfigSource)
//! (JSON and YAML files, env variables, command-line args etc.) and converting this input to strongly typed
//! representation (i.e., config structs or enums). As with other config systems, config input follows the JSON object model
//! (see [`Value`](value::Value)), with each value enriched with its [origin](value::ValueOrigin) (e.g., a path in a specific JSON file,
//! or a specific env var). This allows attributing errors during deserialization.
//!
//! The defining feature of `smart-config` is its schema-driven design. Each config type has associated [metadata](ConfigMetadata)
//! defined with the help of the [`DescribeConfig`] derive macro; deserialization is handled by the accompanying [`DeserializeConfig`] macro.
//! Metadata includes a variety of info extracted from the config type:
//!
//! - [Parameter info](metadata::ParamMetadata): name (including aliases and renaming), help (extracted from doc comments),
//!   type, [deserializer for the param](de::DeserializeParam) etc.
//! - [Nested configurations](metadata::NestedConfigMetadata).
//!
//! Multiple configurations are collected into a global [`ConfigSchema`]. Each configuration is *mounted* at a specific path.
//! E.g., if a large app has an HTTP server component, it may be mounted at `api.http`. Multiple config types may be mounted
//! at the same path (e.g., flattened configs); conversely, a single config type may be mounted at multiple places.
//! As a result, there doesn't need to be a god object uniting all configs in the app; they may be dynamically collected and deserialized
//! inside relevant components.
//!
//! This information provides rich human-readable info about configs. It also assists when preprocessing and merging config inputs.
//! For example, env vars are a flat string -> string map; with the help of a schema, it's possible to:
//!
//! - Correctly nest vars (e.g., transform the `API_HTTP_PORT` var into a `port` var inside `http` object inside `api` object)
//! - Transform value types from strings to expected types.
//!
//! Preprocessing and merging config sources is encapsulated in [`ConfigRepository`].
//!
//! # Features
//!
//! - Rich, self-documenting configuration schema.
//! - Utilizes the schema to enrich configuration sources and intelligently merge them.
//! - Doesn't require a god object uniting all configs in the app; they may be dynamically collected and deserialized
//!   inside relevant components.
//! - Supports lazy parsing for complex / multi-component apps (only the used configs are parsed; other configs are not required).
//! - Supports multiple configuration formats and programmable source priorities (e.g., `base.yml` + overrides from the
//!   `overrides/` dir in the alphabetic order + env vars).
//! - Rich and complete deserialization errors including locations and value origins.
//!
//! # Examples
//!
//! ## Basic workflow
//!
//! ```
//! use smart_config::{
//!     config, ConfigSchema, ConfigRepository, DescribeConfig, DeserializeConfig, Yaml, Environment,
//! };
//!
//! #[derive(Debug, DescribeConfig, DeserializeConfig)]
//! pub struct TestConfig {
//!     pub port: u16,
//!     #[config(default_t = "test".into())]
//!     pub name: String,
//!     #[config(default_t = true)]
//!     pub tracing: bool,
//! }
//!
//! let schema = ConfigSchema::default().insert::<TestConfig>("test");
//! // Assume we use two config sources: a YAML file and env vars,
//! // the latter having higher priority.
//! let yaml = r"
//! test:
//!   port: 4000
//!   name: app
//! ";
//! let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml)?)?;
//! let env = Environment::from_iter("APP_", [("APP_TEST_PORT", "8000")]);
//! // Add both sources to a repo.
//! let repo = ConfigRepository::new(&schema).with(yaml).with(env);
//! // Get the parser for the config.
//! let parser = repo.single::<TestConfig>()?;
//! let config = parser.parse()?;
//! assert_eq!(config.port, 8_000); // from the env var
//! assert_eq!(config.name, "app"); // from YAML
//! assert!(config.tracing); // from the default value
//! # anyhow::Ok(())
//! ```
//!
//! ## Declaring type as well-known
//!
//! ```
//! use std::collections::HashMap;
//! use smart_config::{
//!     de::{DeserializeParam, WellKnown},
//!     metadata::BasicType, DescribeConfig, DeserializeConfig,
//! };
//!
//! #[derive(Debug, serde::Deserialize)]
//! enum CustomEnum {
//!     First,
//!     Second,
//! }
//!
//! impl WellKnown for CustomEnum {
//!     // signals that the type should be deserialized via `serde`
//!     // and the expected input is a string
//!     const DE: &'static dyn DeserializeParam<Self> = &BasicType::String;
//! }
//!
//! // Then, the type can be used in configs basically everywhere:
//! #[derive(Debug, DescribeConfig, DeserializeConfig)]
//! struct TestConfig {
//!     value: CustomEnum,
//!     optional: Option<CustomEnum>,
//!     repeated: Vec<CustomEnum>,
//!     map: HashMap<String, CustomEnum>,
//! }
//! ```

// Documentation settings
#![doc(html_root_url = "https://docs.rs/smart-config/0.1.0")]

#[doc(hidden)] // used in the derive macro
pub use once_cell::sync::Lazy;
pub use smart_config_derive::{DescribeConfig, DeserializeConfig};

use self::metadata::ConfigMetadata;
pub use self::{
    de::DeserializeConfig,
    error::{ParseError, ParseErrors},
    schema::{Alias, ConfigMut, ConfigRef, ConfigSchema},
    source::{ConfigParser, ConfigRepository, ConfigSource, Environment, Json, Yaml},
    types::ByteSize,
};

pub mod de;
mod error;
pub mod metadata;
mod schema;
mod source;
pub mod testing;
#[cfg(test)]
mod testonly;
mod types;
pub mod value;

/// Describes a configuration (i.e., a group of related parameters).
pub trait DescribeConfig: 'static {
    /// Provides the config description.
    fn describe_config() -> &'static ConfigMetadata;
}

#[cfg(doctest)]
doc_comment::doctest!("../README.md");
