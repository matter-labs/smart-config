//! Command-line extensions for `smart-config` library.
//!
//! The extensions are as follows:
//!
//! - [Printing help](Printer::print_help()) for configuration params with optional filtering.
//! - [Debugging](Printer::print_debug()) param values and deserialization errors.
//!
//! All extensions are encapsulated in [`Printer`].
//!
//! # Examples
//!
//! See the crate readme or the `examples` dir for captured output samples.
//!
//! ## Printing help
//!
//! ```
//! use smart_config::ConfigSchema;
//! use smart_config_commands::Printer;
//!
//! let mut schema = ConfigSchema::default();
//! // Add configurations to the schema...
//!
//! Printer::stderr().print_help(&schema, |_| true)?;
//! # std::io::Result::Ok(())
//! ```
//!
//! ## Debugging param values
//!
//! ```
//! use smart_config::{ConfigSchema, ConfigRepository};
//! use smart_config_commands::Printer;
//!
//! let mut schema = ConfigSchema::default();
//! // Add configurations to the schema...
//! let mut repo = ConfigRepository::new(&schema);
//! // Add sources to the repository...
//!
//! Printer::stderr().print_debug(&repo)?;
//! # std::io::Result::Ok(())
//! ```

// Documentation settings
#![doc(html_root_url = "https://docs.rs/smart-config-commands/0.1.0")]
// Linter settings
#![warn(missing_docs)]

use std::{
    io,
    io::{StderrLock, StdoutLock},
};

use anstream::{stream::RawStream, AutoStream};

mod debug;
mod help;

/// Wrapper around an I/O writer. Will style the output with ANSI sequences if appropriate.
///
/// Internally, the printer is based on [`anstream`] / [`anstyle`]; see their docs to find out how styling support
/// is detected by default. (TL;DR: based on `NO_COLOR`, `CLICOLOR_FORCE` and `CLICOLOR` env vars, and whether
/// the output is a terminal.) If this detection doesn't work for you, you can always [create](Self::custom()) a fully custom `Printer`.
///
/// [`anstream`]: https://docs.rs/anstream/
/// [`anstyle`]: https://docs.rs/anstyle/
#[derive(Debug)]
pub struct Printer<W: RawStream> {
    writer: AutoStream<W>,
}

impl Printer<StdoutLock<'static>> {
    /// Creates a printer to stdout. The stdout is locked while the printer is alive!
    pub fn stdout() -> Self {
        Self {
            writer: AutoStream::auto(io::stdout()).lock(),
        }
    }
}

impl Printer<StderrLock<'static>> {
    /// Creates a printer to stderr. The stderr is locked while the printer is alive!
    pub fn stderr() -> Self {
        Self {
            writer: AutoStream::auto(io::stderr()).lock(),
        }
    }
}

impl<W: RawStream> Printer<W> {
    /// Creates a custom printer.
    pub fn custom(writer: AutoStream<W>) -> Self {
        Self { writer }
    }
}

#[cfg(doctest)]
doc_comment::doctest!("../README.md");
