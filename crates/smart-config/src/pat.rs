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
            Self::Regex(regex) => write!(formatter, "Regex({})", RawStr(regex)),
            Self::Generic(s) => formatter.write_str(s),
        }
    }
}

/// Wrapper for strings that outputs a string as a raw string literal, like `r"\s+"`.
#[doc(hidden)] // reused in the `commands` crate; logically private
#[derive(Clone, Copy)]
pub struct RawStr<'a>(pub &'a str);

impl fmt::Debug for RawStr<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for RawStr<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hash_count = self.hash_count();
        write!(formatter, "r")?;
        for _ in 0..hash_count {
            write!(formatter, "#")?;
        }
        write!(formatter, "\"{}\"", self.0)?;
        for _ in 0..hash_count {
            write!(formatter, "#")?;
        }
        Ok(())
    }
}

impl RawStr<'_> {
    // Determine the number of necessary `#` for the raw string specifier.
    fn hash_count(self) -> usize {
        let has_double_quotes = self.0.chars().any(|ch| ch == '"');
        if has_double_quotes {
            let mut max_hashes = 0;
            let mut hash_start = None;
            for (i, ch) in self.0.chars().enumerate() {
                if ch == '#' {
                    if hash_start.is_none() {
                        hash_start = Some(i);
                    }
                } else if let Some(hash_start) = hash_start.take() {
                    max_hashes = max_hashes.max(i - hash_start);
                }
            }
            max_hashes + 1
        } else {
            0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_count_for_raw_strings_is_correct() {
        let s = RawStr("Hello, world!");
        assert_eq!(s.hash_count(), 0);
        assert_eq!(s.to_string(), "r\"Hello, world!\"");

        let s = RawStr("####");
        assert_eq!(RawStr("####").hash_count(), 0);
        assert_eq!(s.to_string(), "r\"####\"");

        let s = RawStr(r#"x="1""#);
        assert_eq!(s.hash_count(), 1);
        assert_eq!(s.to_string(), "r#\"x=\"1\"\"#");

        let s = RawStr(r##"x="#1""##);
        assert_eq!(s.hash_count(), 2);
        assert_eq!(s.to_string(), "r##\"x=\"#1\"\"##");
    }
}
