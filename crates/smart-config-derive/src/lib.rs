//! Procedural macros for `smart-config`.
//!
//! All macros in this crate are re-exported from the `smart-config` crate. See its docs for more details
//! and the examples of usage.
//!
//! - [Macro reference](https://docs.rs/smart-config/0.4.0-pre.3/smart_config/_docs/derive_ref/) <!-- x-release-please-version -->
//! - [Examples of usage](https://docs.rs/smart-config/0.4.0-pre.3/smart_config/_docs/derive_examples/) <!-- x-release-please-version -->

// Documentation settings
#![doc(html_root_url = "https://docs.rs/smart-config-derive/0.4.0-pre.3")] // x-release-please-version
// General settings
#![recursion_limit = "128"]

extern crate proc_macro;

use proc_macro::TokenStream;

mod de;
mod describe;
mod example;
mod utils;

/// Derives the `DescribeConfig` trait for a type. Typically used together with [`DeserializeConfig`]
/// to generate a fully deserializable, self-descriptive configuration.
///
/// # See also
///
/// - [Macro reference](https://docs.rs/smart-config/0.4.0-pre.3/smart_config/_docs/derive_ref/) <!-- x-release-please-version -->
/// - [Examples of usage](https://docs.rs/smart-config/0.4.0-pre.3/smart_config/_docs/derive_examples/) <!-- x-release-please-version -->
#[proc_macro_derive(DescribeConfig, attributes(config))]
pub fn describe_config(input: TokenStream) -> TokenStream {
    describe::impl_describe_config(input)
}

/// Derives the `ExampleConfig` trait for a type. This allows to provide an example for all config parameters.
///
/// This macro is intended to be used together with [`DescribeConfig`]; it reuses
/// the same attributes. Specifically, for each config field, the default value is assigned from the following sources
/// in the decreasing priority order:
///
/// 1. `example`
/// 2. `default` / `default_t`, including implied ones for `Option`al fields
/// 3. From `ExampleConfig` implementation (only for nested / flattened configs)
///
/// # See also
///
/// - [Macro reference](https://docs.rs/smart-config/0.4.0-pre.3/smart_config/_docs/derive_ref/) <!-- x-release-please-version -->
/// - [Examples of usage](https://docs.rs/smart-config/0.4.0-pre.3/smart_config/_docs/derive_examples/) <!-- x-release-please-version -->
#[proc_macro_derive(ExampleConfig, attributes(config))]
pub fn example_config(input: TokenStream) -> TokenStream {
    example::impl_example_config(input)
}

/// Derives the `DeserializeConfig` trait for a type. Typically used together with [`DescribeConfig`]
/// to generate a fully deserializable, self-descriptive configuration.
///
/// # See also
///
/// - [Macro reference](https://docs.rs/smart-config/0.4.0-pre.3/smart_config/_docs/derive_ref/) <!-- x-release-please-version -->
/// - [Examples of usage](https://docs.rs/smart-config/0.4.0-pre.3/smart_config/_docs/derive_examples/) <!-- x-release-please-version -->
#[proc_macro_derive(DeserializeConfig, attributes(config))]
pub fn deserialize_config(input: TokenStream) -> TokenStream {
    de::impl_deserialize_config(input)
}
