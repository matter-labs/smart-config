//! Procedural macros for `smart-config`.
//!
//! All macros in this crate are re-exported from the `smart-config` crate. See its docs for more details
//! and the examples of usage.

// Documentation settings
#![doc(html_root_url = "https://docs.rs/smart-config-derive/0.4.0-pre.2")] // x-release-please-version
// General settings
#![recursion_limit = "128"]

extern crate proc_macro;

use proc_macro::TokenStream;

mod de;
mod describe;
mod example;
mod utils;

#[proc_macro_derive(DescribeConfig, attributes(config))]
pub fn describe_config(input: TokenStream) -> TokenStream {
    describe::impl_describe_config(input)
}

#[proc_macro_derive(ExampleConfig, attributes(config))]
pub fn example_config(input: TokenStream) -> TokenStream {
    example::impl_example_config(input)
}

#[proc_macro_derive(DeserializeConfig, attributes(config))]
pub fn deserialize_config(input: TokenStream) -> TokenStream {
    de::impl_deserialize_config(input)
}
