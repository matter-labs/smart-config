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
//! let mut schema = ConfigSchema::default();
//! schema.insert::<TestConfig>("test")?;
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
//!     de::{Serde, WellKnown}, metadata::BasicTypes,
//!     DescribeConfig, DeserializeConfig,
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
//!     type Deserializer = Serde![str];
//!     const DE: Self::Deserializer = Serde![str];
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
#![cfg_attr(docsrs, feature(doc_cfg))]
// Linter settings
#![warn(missing_docs)]

/// Derives the [`DescribeConfig`](trait@DescribeConfig) trait for a type.
///
/// This macro supports both structs and enums. It is conceptually similar to `Deserialize` macro from `serde`.
/// Macro behavior can be configured with `#[config(_)]` attributes. Multiple `#[config(_)]` attributes
/// on a single item are supported.
///
/// Each field in the struct / each enum variant is considered a configuration param (by default),
/// or a sub-config (if `#[config(nest)]` or `#[config(flatten)]` is present for the field).
///
/// # Container attributes
///
/// ## `tag`
///
/// **Type:** string
///
/// Specifies the param name holding the enum tag, similar to the corresponding attribute in `serde`.
/// Unlike `serde`, this attribute is *required* for enums; this is to ensure that source merging is well-defined.
///
/// ## `rename_all`
///
/// **Type:** string; one of `lowercase`, `UPPERCASE`, `camelCase`, `snake_case`, `SCREAMING_SNAKE_CASE`,
/// `kebab-case`, `SCREAMING-KEBAB-CASE`
///
/// Renames all variants in an enum config according to the provided transform. Unlike in `serde`, this attribute
/// *only* works on enum variants. Params / sub-configs are always expected to have `snake_case` naming.
///
/// Caveats:
///
/// - `rename_all` assumes that original variant names are in `PascalCase` (i.e., follow Rust naming conventions).
/// - `rename_all` requires original variant names to consist of ASCII chars.
/// - Each letter of capitalized acronyms (e.g., "HTTP" in `HTTPServer`) is treated as a separate word.
///   E.g., `rename_all = "snake_case"` will rename `HTTPServer` to `h_t_t_p_server`.
///   Note that [it is recommended][clippy-acronyms] to not capitalize acronyms (i.e., use `HttpServer`).
/// - No spacing is inserted before numbers or other non-letter chars. E.g., `rename_all = "snake_case"`
///   will rename `Status500` to `status500`, not to `status_500`.
///
/// [clippy-acronyms]: https://rust-lang.github.io/rust-clippy/master/index.html#/upper_case_acronyms
///
/// ## `derive(Default)`
///
/// Derives `Default` according to the default values of params (+ the default variant for enum configs).
/// To work, all params must have a default value specified.
///
/// # Variant attributes
///
/// ## `rename`, `alias`
///
/// **Type:** string
///
/// Have the same meaning as in `serde`; i.e. allow to rename / specify additional names for the tag(s)
/// corresponding to the variant. `alias` can be specified multiple times.
///
/// ## `default`
///
/// If specified, marks the variant as default – one which will be used if the tag param is not set in the input.
/// At most one variant can be marked as default.
///
/// # Field attributes
///
/// ## `rename`, `alias`
///
/// **Type:** string
///
/// Have the same meaning as in `serde`; i.e. allow to rename / specify additional names for the param.
/// Param names are [validated](#validations) in compile time.
///
/// ## `default`
///
/// **Type:** path to function (optional)
///
/// Has the same meaning as in `serde`, i.e. allows to specify a constructor of the default value for the param.
/// Without a value, [`Default`] is used for this purpose. Unlike `serde`, the path shouldn't be quoted.
///
/// ## `default_t`
///
/// **Type:** expression with param type
///
/// Allows to specify the default typed value for the param. The provided expression doesn't need to be constant.
///
/// ## `with`
///
/// **Type:** const expression implementing [`DeserializeParam`]
///
/// Allows changing the param deserializer. See [`de`] module docs for the overview of available deserializers.
/// Note that there is an alternative: implementing [`WellKnown`](de::WellKnown) for the param type.
///
/// ## `nest`
///
/// If specified, the field is treated as a nested sub-config rather than a param. Correspondingly, its type must
/// implement `DescribeConfig`.
///
/// ## `flatten`
///
/// If specified, the field is treated as a *flattened* sub-config rather than a param. Unlike `nest`, its params
/// will be added to the containing config instead of a separate object. The sub-config type must implement `DescribeConfig`.
///
/// # Validations
///
/// The following validations are performed by the macro in compile time:
///
/// - Param / sub-config names and aliases must be non-empty, consist of lowercase ASCII alphanumeric chars or underscore
///   and not start with a digit (i.e., follow the `[a-z_][a-z0-9_]*` regex).
/// - Param names / aliases cannot coincide with nested config names.
///
/// [`DeserializeParam`]: de::DeserializeParam
///
/// # Examples
///
/// ```
/// # use std::{collections::HashSet, num::NonZeroUsize, time::Duration};
/// # use smart_config::{DescribeConfig, DeserializeConfig};
/// use smart_config::metadata::TimeUnit;
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     /// Doc comments are parsed as a description.
///     #[config(default_t = 3)]
///     int: u32,
///     #[config(default)] // multiple `config` attrs are supported
///     #[config(rename = "str", alias = "string")]
///     renamed: String,
///     /// Nested sub-config. E.g., the tag will be read from path `nested.version`.
///     #[config(nest)]
///     nested: NestedConfig,
///     /// Flattened sub-config. E.g., `array` param will be read from `array`, not `flat.array`.
///     #[config(flatten)]
///     flat: FlattenedConfig,
/// }
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// #[config(tag = "version", rename_all = "snake_case", derive(Default))]
/// enum NestedConfig {
///     #[config(default)]
///     V0,
///     #[config(alias = "latest")]
///     V1 {
///         /// Param with a custom deserializer. In this case, it will deserialize
///         /// a duration from a number with milliseconds unit of measurement.
///         #[config(default_t = Duration::from_millis(50), with = TimeUnit::Millis)]
///         latency_ms: Duration,
///         /// `Vec`s, sets and other containers are supported out of the box.
///         set: HashSet<NonZeroUsize>,
///     },
/// }
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct FlattenedConfig {
///     #[config(default = FlattenedConfig::default_array)]
///     array: [f32; 2],
/// }
///
/// impl FlattenedConfig {
///     const fn default_array() -> [f32; 2] { [1.0, 2.0] }
/// }
/// ```
pub use smart_config_derive::DescribeConfig;
/// Derives the [`DeserializeConfig`](trait@DeserializeConfig) trait for a type.
///
/// This macro is intended to be used together with [`DescribeConfig`](macro@DescribeConfig). It reuses
/// the same attributes, so see `DescribeConfig` docs for details and examples of usage.
pub use smart_config_derive::DeserializeConfig;

use self::metadata::ConfigMetadata;
pub use self::{
    de::DeserializeConfig,
    error::{ParseError, ParseErrors},
    schema::{ConfigMut, ConfigRef, ConfigSchema},
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
    const DESCRIPTION: ConfigMetadata;
}

#[cfg(doctest)]
doc_comment::doctest!("../README.md");
