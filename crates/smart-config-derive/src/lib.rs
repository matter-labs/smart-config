#![recursion_limit = "128"]

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
