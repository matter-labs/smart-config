//! Procedural macros for `smart-config`.
//!
//! All macros in this crate are re-exported from the `smart-config` crate. See its docs for more details
//! and the examples of usage.

#![recursion_limit = "128"]
#![allow(missing_docs)] // Adding docs here would interfere with docs in the main crate

extern crate proc_macro;

use proc_macro::TokenStream;

mod de;
mod describe;
mod utils;

#[proc_macro_derive(DescribeConfig, attributes(config))]
pub fn describe_config(input: TokenStream) -> TokenStream {
    describe::impl_describe_config(input)
}

#[proc_macro_derive(DeserializeConfig, attributes(config))]
pub fn deserialize_config(input: TokenStream) -> TokenStream {
    de::impl_deserialize_config(input)
}
