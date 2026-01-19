//! String patterns. Used to initialize delimiters for deserializers, like [`Delimited`]
//! and [`DelimitedEntries`].
//!
//! [`Delimited`]: crate::de::Delimited
//! [`DelimitedEntries`]: crate::de::DelimitedEntries

use std::{fmt, sync::LazyLock};

pub use regex::Regex;

/// Human-readable (for people familiar with regexes) representation of a compiled pattern.
#[doc(hidden)] // not stable yet
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum PatternDisplay {
    /// Pattern is an exact string match.
    Exact(String),
    /// Pattern is a regular expression conforming to the syntax supported by the `regex` crate.
    Regex(String),
    /// Pattern is generic `Debug` representation (e.g., an array of chars).
    Generic(String),
}

impl fmt::Display for PatternDisplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(s) => write!(formatter, "{s:?}"),
            Self::Regex(regex) => write!(formatter, "Regex({regex:?})"),
            Self::Generic(s) => formatter.write_str(s),
        }
    }
}

/// Splitting strings.
pub trait Split: Send + Sync + 'static {
    /// Splits the given `haystack` at most once from its start. This generalizes [`str::split_once()`].
    fn split_once<'s>(&self, haystack: &'s str) -> Option<(&'s str, &'s str)>;
    /// Splits the given `haystack`. This generalizes [`str::split()`].
    fn split<'s>(&self, haystack: &'s str) -> impl Iterator<Item = &'s str>;

    #[doc(hidden)]
    fn display(&self) -> PatternDisplay;
}

impl<const N: usize> Split for [char; N] {
    fn split_once<'s>(&self, haystack: &'s str) -> Option<(&'s str, &'s str)> {
        haystack.split_once(self)
    }

    fn split<'s>(&self, haystack: &'s str) -> impl Iterator<Item = &'s str> {
        haystack.split(self)
    }

    fn display(&self) -> PatternDisplay {
        PatternDisplay::Generic(format!("{self:?}"))
    }
}

impl Split for &'static str {
    fn split_once<'s>(&self, haystack: &'s str) -> Option<(&'s str, &'s str)> {
        haystack.split_once(self)
    }

    fn split<'s>(&self, haystack: &'s str) -> impl Iterator<Item = &'s str> {
        haystack.split(self)
    }

    fn display(&self) -> PatternDisplay {
        PatternDisplay::Exact((*self).to_owned())
    }
}

// We cannot implement `Split for R: Deref<Target = Regex>` here because of orphaning rules,
// and the direct implementation for `&'static Regex` would be useless because it cannot be initialized in compile time.
impl Split for &'static LazyLock<Regex> {
    fn split_once<'s>(&self, haystack: &'s str) -> Option<(&'s str, &'s str)> {
        let mut it = self.splitn(haystack, 2);
        let head = it.next()?;
        let tail = it.next()?;
        Some((head, tail))
    }

    fn split<'s>(&self, haystack: &'s str) -> impl Iterator<Item = &'s str> {
        Regex::split(self, haystack)
    }

    fn display(&self) -> PatternDisplay {
        PatternDisplay::Regex(self.as_str().to_owned())
    }
}
