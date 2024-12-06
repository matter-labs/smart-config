// Documentation settings
#![doc(html_root_url = "https://docs.rs/smart-config-commands/0.1.0")]

pub use self::{debug::print_debug, help::print_help};

mod debug;
mod help;

#[cfg(doctest)]
doc_comment::doctest!("../README.md");
