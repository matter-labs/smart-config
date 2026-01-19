//! String patterns. Used to initialize delimiters for deserializers, like [`Delimited`]
//! and [`DelimitedEntries`].
//!
//! [`Delimited`]: crate::de::Delimited
//! [`DelimitedEntries`]: crate::de::DelimitedEntries

use std::{fmt, sync::OnceLock};

use regex::Regex;

/// Pattern (e.g., a regular expression) that can be compiled into a [`Split`] implementation.
pub trait CompiledPattern: 'static + Send + Sync + fmt::Debug {
    /// Compiled pattern.
    type Compiled: Split + ?Sized;

    /// gets the compiled pattern. Due to returning the reference, implementations are all but forced
    /// to cache the result (e.g., in a [`OnceLock`]).
    ///
    /// # Errors
    ///
    /// Returns an error if the pattern is invalid. Errors should be cached as well.
    fn compiled(&self) -> Result<&Self::Compiled, &str>;
}

/// Splitting strings.
pub trait Split {
    /// Splits the given `haystack` at most once from its start. This generalizes [`str::split_once()`].
    fn split_once<'s>(&self, haystack: &'s str) -> Option<(&'s str, &'s str)>;
    /// Splits the given `haystack`. This generalizes [`str::split()`].
    fn split<'s>(&self, haystack: &'s str) -> impl Iterator<Item = &'s str>;
}

impl<const N: usize> CompiledPattern for [char; N] {
    type Compiled = [char];

    fn compiled(&self) -> Result<&Self::Compiled, &str> {
        Ok(self)
    }
}

impl Split for [char] {
    fn split_once<'s>(&self, haystack: &'s str) -> Option<(&'s str, &'s str)> {
        haystack.split_once(self)
    }

    fn split<'s>(&self, haystack: &'s str) -> impl Iterator<Item = &'s str> {
        haystack.split(self)
    }
}

impl CompiledPattern for &'static str {
    type Compiled = str;

    fn compiled(&self) -> Result<&str, &str> {
        Ok(*self)
    }
}

impl Split for str {
    fn split_once<'s>(&self, haystack: &'s str) -> Option<(&'s str, &'s str)> {
        haystack.split_once(self)
    }

    fn split<'s>(&self, haystack: &'s str) -> impl Iterator<Item = &'s str> {
        haystack.split(self)
    }
}

/// Lazily initialized regular expression.
#[derive(Debug)]
pub struct LazyRegex {
    raw: &'static str,
    parsed: OnceLock<Result<Regex, String>>,
}

impl LazyRegex {
    /// Creates a lazily compiled regular expression.
    pub const fn new(raw: &'static str) -> LazyRegex {
        Self {
            raw,
            parsed: OnceLock::new(),
        }
    }
}

impl CompiledPattern for &'static LazyRegex {
    type Compiled = Regex;

    fn compiled(&self) -> Result<&Self::Compiled, &str> {
        self.parsed
            .get_or_init(|| Regex::new(self.raw).map_err(|err| err.to_string()))
            .as_ref()
            .map_err(String::as_str)
    }
}

impl Split for Regex {
    fn split_once<'s>(&self, haystack: &'s str) -> Option<(&'s str, &'s str)> {
        let mut it = self.splitn(haystack, 2);
        let head = it.next()?;
        let tail = it.next()?;
        Some((head, tail))
    }

    fn split<'s>(&self, haystack: &'s str) -> impl Iterator<Item = &'s str> {
        Regex::split(self, haystack)
    }
}
