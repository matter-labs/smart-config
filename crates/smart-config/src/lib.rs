//! `smart-config` – schema-driven layered configuration system with support of multiple configuration formats.
//!
//! # Overview
//!
//! *See [extra docs](_docs) for deep dive into library features.*
//!
//! The task solved by the library is merging configuration input from a variety of prioritized [sources](ConfigSource)
//! (JSON and YAML files, env variables, command-line args etc.) and converting this input to strongly typed
//! representation (i.e., config structs or enums). As with other config systems, config input follows the JSON object model
//! (see [`Value`](value::Value)), with each value enriched with its [origin](value::ValueOrigin) (e.g., a path in a specific JSON file,
//! or a specific env var). This allows attributing errors during deserialization.
//!
//! The defining feature of `smart-config` is its schema-driven design. Each config type has associated [metadata](ConfigMetadata)
//! defined with the help of the [`DescribeConfig`](macro@DescribeConfig) derive macro; deserialization is handled
//! by the accompanying [`DeserializeConfig`](macro@DeserializeConfig) macro.
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
//! # TL;DR
//!
//! - Rich, self-documenting configuration schema.
//! - Utilizes the schema to enrich configuration sources and intelligently merge them.
//! - Doesn't require a god object uniting all configs in the app; they may be dynamically collected and deserialized
//!   inside relevant components.
//! - Supports lazy parsing for complex / multi-component apps (only the used configs are parsed; other configs are not required).
//! - Supports multiple configuration formats and programmable source priorities (e.g., `base.yml` + overrides from the
//!   `overrides/` dir in the alphabetic order + env vars).
//! - Rich and complete deserialization errors including locations and value origins.
//! - [Built-in support for secret params](de#secrets).
//!
//! # Crate features
//!
//! ## `primitive-types`
//!
//! *(Off by default)*
//!
//! Implements deserialization for basic Ethereum types like [`H256`](primitive_types::H256) (32-byte hash)
//! and [`U256`](primitive_types::U256) (256-bit unsigned integer).
//!
//! ## `alloy`
//!
//! *(Off by default)*
//!
//! Implements deserialization for basic alloy primitive types like [`B256`](alloy::primitives::B256) (32-byte hash)
//! and [`U256`](alloy::primitives::U256) (256-bit unsigned integer).
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
//! let schema = ConfigSchema::new(&TestConfig::DESCRIPTION, "test");
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
//!     de::{Serde, WellKnown, WellKnownOption}, metadata::BasicTypes,
//!     DescribeConfig, DeserializeConfig,
//! };
//!
//! #[derive(Debug, serde::Serialize, serde::Deserialize)]
//! enum CustomEnum {
//!     First,
//!     Second,
//! }
//!
//! impl WellKnown for CustomEnum {
//!     // signals that the type should be deserialized via `serde`
//!     // and the expected input is a string
//!     type Deserializer = Serde![str];
//!     const DE: Self::Deserializer = Serde![str];
//! }
//!
//! // Signals that the type can be used with an `Option<_>`
//! impl WellKnownOption for CustomEnum {}
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
#![doc(html_root_url = "https://docs.rs/smart-config/0.4.0-pre.2")] // x-release-please-version
#![cfg_attr(docsrs, feature(doc_cfg))]
// TODO: restore the attribute once supported by the nightly toolchain used by ZKsync OS
//   #![cfg_attr(docsrs, doc(auto_cfg(hide(feature = "_docs"))))]
// Linter settings
#![warn(missing_docs)]

pub use smart_config_derive::{DescribeConfig, DeserializeConfig, ExampleConfig};

pub use self::{
    de::DeserializeConfig,
    error::{DeserializeConfigError, ErrorWithOrigin, ParseError, ParseErrorCategory, ParseErrors},
    schema::{ConfigMut, ConfigRef, ConfigSchema},
    source::{
        ConfigParser, ConfigRepository, ConfigSource, ConfigSourceKind, ConfigSources, Environment,
        Flat, Hierarchical, Json, Prefixed, SerializerOptions, SourceInfo, Yaml,
    },
    types::{ByteSize, EtherAmount},
};
use self::{metadata::ConfigMetadata, visit::VisitConfig};

#[cfg(feature = "_docs")]
pub mod _docs;
pub mod de;
mod error;
pub mod fallback;
pub mod metadata;
pub mod pat;
mod schema;
mod source;
pub mod testing;
#[cfg(test)]
mod testonly;
mod types;
mod utils;
pub mod validation;
pub mod value;
pub mod visit;

/// Describes a configuration (i.e., a group of related parameters).
pub trait DescribeConfig: 'static + VisitConfig {
    /// Provides the config description.
    const DESCRIPTION: ConfigMetadata;
}

/// Provides an example for this configuration. The produced config can be used in tests etc.
///
/// For struct configs, this can be derived via [the corresponding proc macro](macro@ExampleConfig).
pub trait ExampleConfig {
    /// Constructs an example configuration.
    fn example_config() -> Self;
}

impl<T: ExampleConfig> ExampleConfig for Option<T> {
    fn example_config() -> Self {
        Some(T::example_config())
    }
}

#[cfg(doctest)]
doc_comment::doctest!("../README.md");
