//! `smart-config` – schema-driven layered configuration system with support of multiple configuration formats.
//!
//! # Overview
//!
//! The task being solved by the library is merging configuration input from a variety of prioritized [sources](ConfigSource)
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
//!
//! This information provides rich human-readable info about configs. It also assists when preprocessing and merging config inputs.
//! For example, env vars are a flat string -> string map; with the help of a schema, it's possible to:
//!
//! - Correctly nest vars (e.g., transform the `API_HTTP_PORT` var into a `port` var inside `http` object inside `api` object)
//! - Transform value types from strings to expected types.
//!
//! Preprocessing and merging config sources is encapsulated in [`ConfigRepository`].

#![warn(missing_docs)]

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
