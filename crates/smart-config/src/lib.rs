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
#![doc(html_root_url = "https://docs.rs/smart-config/0.4.0-pre")] // x-release-please-version
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
/// ## `validate`
///
/// **Type:** One of the following:
///
/// - Expression evaluating to a [`Validate`](validation::Validate) implementation (e.g., a [`Range`](std::ops::Range); see the `Validate` docs
///   for implementations). An optional human-readable string validation description may be provided delimited by the comma (e.g., to make the description
///   more domain-specific).
/// - Pointer to a function with the `fn(&_) -> Result<(), ErrorWithOrigin>` signature and the validation description separated by a comma.
/// - Pointer to a function with the `fn(&_) -> bool` signature and the validation description separated by a comma. Validation fails
///   if the function returns `false`.
///
/// See the examples in the [`validation`] module.
///
/// Specifies a post-deserialization validation for the config. This is useful to check invariants involving multiple params.
/// Multiple validations are supported by specifying the attribute multiple times.
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
/// Have the same meaning as in `serde`; i.e. allow to rename / specify additional names for the param or a nested config.
/// Names are [validated](#validations) in compile time.
///
/// In addition to simple names, *path* aliases are supported as well. A path alias starts with `.` and consists of dot-separated segments,
/// e.g. `.experimental.value` or `..value`. The paths are resolved relative to the config prefix. As in Python, more than one dot
/// at the start of the path signals that the path is relative to the parent(s) of the config.
///
/// - `alias = ".experimental.value"` with config prefix `test` resolves to the absolute path `test.experimental.value`.
/// - `alias = "..value"` with config prefix `test.experimental` resolves to the absolute path `test.value`.
///
/// If an alias requires more parents than is present in the config prefix, the alias is not applicable.
/// (E.g., `alias = "...value"` with config prefix `test`.)
///
/// Path aliases are somewhat difficult to reason about, so avoid using them unless necessary.
///
/// ## `deprecated`
///
/// **Type:** string
///
/// Similar to `alias`, with the difference that the alias is marked as deprecated in the schema docs,
/// and its usages are logged on the `WARN` level.
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
/// ## `example`
///
/// **Type:** expression with field type
///
/// Allows to specify the example value for the param. The example value can be specified together with the `default` / `default_t`
/// attribute. In this case, the example value can be more "complex" than the default, to better illustrate how the configuration works.
///
/// ## `fallback`
///
/// **Type:** constant expression evaluating to `&'static dyn `[`FallbackSource`](fallback::FallbackSource)
///
/// Allows to provide a fallback source for the param. See the [`fallback`] module docs for the discussion of fallbacks
/// and intended use cases.
///
/// ## `with`
///
/// **Type:** const expression implementing [`DeserializeParam`]
///
/// Allows changing the param deserializer. See [`de`] module docs for the overview of available deserializers.
/// For `Option`s, `with` refers to the *internal* type deserializer; it will be wrapped into an [`Optional`](crate::de::Optional) automatically.
///
/// Note that there is an alternative: implementing [`WellKnown`](de::WellKnown) for the param type.
///
/// ## `nest`
///
/// If specified, the field is treated as a nested sub-config rather than a param. Correspondingly, its type must
/// implement `DescribeConfig`, or wrap such a type in an `Option`.
///
/// ## `flatten`
///
/// If specified, the field is treated as a *flattened* sub-config rather than a param. Unlike `nest`, its params
/// will be added to the containing config instead of a separate object. The sub-config type must implement `DescribeConfig`.
///
/// ## `validate`
///
/// Has same semantics as [config validations](#validate), but applies to a specific config parameter.
///
/// ## `deserialize_if`
///
/// **Type:** same as [config validations](#validate)
///
/// Filters an `Option`al value. This is useful to coerce semantically invalid values (e.g., empty strings for URLs)
/// to `None` in the case [automated null coercion](crate::de::Optional#encoding-nulls) doesn't apply.
/// See the [`validation`] module for examples of usage.
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
/// Derives the [`ExampleConfig`](trait@ExampleConfig) trait for a type.
///
/// This macro is intended to be used together with [`DescribeConfig`](macro@DescribeConfig); it reuses
/// the same attributes. Specifically, for each config field, the default value is assigned from the following sources
/// in the decreasing priority order:
///
/// 1. `example`
/// 2. `default` / `default_t`, including implied ones for `Option`al fields
/// 3. From [`ExampleConfig`](trait@ExampleConfig) implementation (only for nested / flattened configs)
///
/// # Examples
///
/// ```
/// # use std::collections::HashSet;
/// # use smart_config::{DescribeConfig, ExampleConfig, SerializerOptions};
/// #[derive(DescribeConfig, ExampleConfig)]
/// struct TestConfig {
///     /// Required param that still has an example value.
///     #[config(example = 42)]
///     required: u32,
///     optional: Option<String>,
///     #[config(default_t = true)]
///     with_default: bool,
///     #[config(default, example = vec![5, 8])]
///     values: Vec<u32>,
///     #[config(nest)]
///     nested: NestedConfig,
/// }
///
/// #[derive(DescribeConfig, ExampleConfig)]
/// struct NestedConfig {
///     #[config(default, example = ["eth_call".into()].into())]
///     methods: HashSet<String>,
/// }
///
/// let example: TestConfig = TestConfig::example_config();
/// let json = SerializerOptions::default().serialize(&example);
/// assert_eq!(
///     serde_json::Value::from(json),
///     serde_json::json!({
///         "required": 42,
///         "optional": null,
///         "with_default": true,
///         "values": [5, 8],
///         "nested": {
///             "methods": ["eth_call"],
///         },
///     })
/// );
/// ```
pub use smart_config_derive::ExampleConfig;

pub use self::{
    de::DeserializeConfig,
    error::{DeserializeConfigError, ErrorWithOrigin, ParseError, ParseErrorCategory, ParseErrors},
    schema::{ConfigMut, ConfigRef, ConfigSchema},
    source::{
        ConfigParser, ConfigRepository, ConfigSource, ConfigSourceKind, ConfigSources, Environment,
        Flat, Hierarchical, Json, Prefixed, SerializerOptions, SourceInfo, Yaml,
    },
    types::ByteSize,
};
use self::{metadata::ConfigMetadata, visit::VisitConfig};

pub mod de;
mod error;
pub mod fallback;
pub mod metadata;
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
