//! # Derive macro examples
//!
//! Various examples how to use [`DescribeConfig`](macro@crate::DescribeConfig) and other derive macros
//! from the library.
//!
//! # Basic usage
//!
//! Shows how to use the macros with nested and flattened sub-configs, enum configs etc.
//!
//! ```
//! # use std::{collections::HashSet, num::NonZeroUsize, time::Duration};
//! # use smart_config::{DescribeConfig, DeserializeConfig};
//! use smart_config::metadata::TimeUnit;
//!
//! #[derive(DescribeConfig, DeserializeConfig)]
//! struct TestConfig {
//!     /// Doc comments are parsed as a description.
//!     #[config(default_t = 3)]
//!     int: u32,
//!     #[config(default)] // multiple `config` attrs are supported
//!     #[config(rename = "str", alias = "string")]
//!     renamed: String,
//!     /// Nested sub-config. E.g., the tag will be read from path `nested.version`.
//!     #[config(nest)]
//!     nested: NestedConfig,
//!     /// Flattened sub-config. E.g., `array` param will be read from `array`, not `flat.array`.
//!     #[config(flatten)]
//!     flat: FlattenedConfig,
//! }
//!
//! #[derive(DescribeConfig, DeserializeConfig)]
//! #[config(tag = "version", rename_all = "snake_case", derive(Default))]
//! enum NestedConfig {
//!     #[config(default)]
//!     V0,
//!     #[config(alias = "latest")]
//!     V1 {
//!         /// Param with a custom deserializer. In this case, it will deserialize
//!         /// a duration from a number with milliseconds unit of measurement.
//!         #[config(default_t = Duration::from_millis(50), with = TimeUnit::Millis)]
//!         latency_ms: Duration,
//!         /// `Vec`s, sets and other containers are supported out of the box.
//!         set: HashSet<NonZeroUsize>,
//!     },
//! }
//!
//! #[derive(DescribeConfig, DeserializeConfig)]
//! struct FlattenedConfig {
//!     #[config(default = FlattenedConfig::default_array)]
//!     array: [f32; 2],
//! }
//!
//! impl FlattenedConfig {
//!     const fn default_array() -> [f32; 2] { [1.0, 2.0] }
//! }
//! ```
//!
//! # Deriving `ExampleConfig`
//!
//! ```
//! # use std::collections::HashSet;
//! # use smart_config::{DescribeConfig, ExampleConfig, SerializerOptions};
//! #[derive(DescribeConfig, ExampleConfig)]
//! struct TestConfig {
//!     /// Required param that still has an example value.
//!     #[config(example = 42)]
//!     required: u32,
//!     optional: Option<String>,
//!     #[config(default_t = true)]
//!     with_default: bool,
//!     #[config(default, example = vec![5, 8])]
//!     values: Vec<u32>,
//!     #[config(nest)]
//!     nested: NestedConfig,
//! }
//!
//! #[derive(DescribeConfig, ExampleConfig)]
//! struct NestedConfig {
//!     #[config(default, example = ["eth_call".into()].into())]
//!     methods: HashSet<String>,
//! }
//!
//! let example: TestConfig = TestConfig::example_config();
//! let json = SerializerOptions::default().serialize(&example);
//! assert_eq!(
//!     serde_json::Value::from(json),
//!     serde_json::json!({
//!         "required": 42,
//!         "optional": null,
//!         "with_default": true,
//!         "values": [5, 8],
//!         "nested": {
//!             "methods": ["eth_call"],
//!         },
//!     })
//! );
//! ```
//!
//! # Advanced features
//!
//! Demonstrates some advanced library features:
//!
//! - [Parameter validation](crate::validation)
//! - [Fallback values](crate::fallback)
//! - Complex deserializers: [`Delimited`](crate::de::Delimited), [`NamedEntries`](crate::de::NamedEntries)
//!   and [`DelimitedEntries`](crate::de::DelimitedEntries)
//! - Use or [regexes](crate::pat) as delimiters and validations
//! - Deserialization of values [with units](crate::de::WithUnit), and the fixed-unit alternative
//! - [Deprecated aliases](super::derive_ref#deprecated) and the use of paths
//! - [Secret values](super::derive_ref#secret).
//!
//! ```
#![doc = include_str!("../../tests/code_samples/test_config.rs")]
//! ```
//!
//! ## Matching YAML configuration
//!
//! ```yaml
#![doc = include_str!("../../tests/code_samples/test.yml")]
//! ```
//!
//! ## Matching env variables
//!
//! Assumes the `APP_` prefix for env vars.
//!
//! ```shell
#![doc = include_str!("../../tests/code_samples/test.env")]
//! ```
//!
//! # See also
//!
//! - [Derive macro reference](super::derive_ref)
//! - [Combining config sources](super::sources)
