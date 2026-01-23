//! String pattern matching.
//!
//! # Overview
//!
//! In the library, pattern matching is used to initialize delimiters for deserializers, like [`Delimited`]
//! and [`DelimitedEntries`], and for [validating](crate::validation) string config params. Pattern matching
//! essentially generalizes the [`Pattern`](std::str::pattern::Pattern) trait from the standard library.
//! Since the trait itself is unstable, we don't use it directly; instead, we define the [`Split`] trait
//! and implement it for `&str` and `[char; _]` (via `Pattern`) and for [`Regex`]es from the eponymous crate
//! (via the [`LazyRegex`] wrapper; see its docs for why this wrapper is needed).
//!
//! [`Delimited`]: crate::de::Delimited
//! [`DelimitedEntries`]: crate::de::DelimitedEntries
//!
//! # Examples
//!
//! - See [`Delimited`](crate::de::Delimited#examples) and [`DelimitedEntries`](crate::de::DelimitedEntries#examples)
//!   for the examples of usage of delimiters.
//! - See the [`validation`](crate::validation) module for the examples of string validation using [`LazyRegex`].

use std::{fmt, ops, sync::LazyLock};

pub use regex::Regex;

/// Human-readable (for people familiar with regexes) representation of a compiled pattern.
#[doc(hidden)] // not stable yet
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum PatternDisplay {
    /// Pattern is an exact string match.
    Exact(&'static str),
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

/// Pattern usable for splitting strings. Used in [`Delimited`](crate::de::Delimited)
/// and [`DelimitedEntries`](crate::de::DelimitedEntries) deserializers.
///
/// # Standard implementations
///
/// - `&str`: matches a string exactly
/// - `[char; _]`: matches any of the chars
/// - [`LazyRegex`]\: matches a regular expression
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
        PatternDisplay::Exact(self)
    }
}

/// Transparent wrapper around a type dereferencing to a [`Regex`]. Can be used as [a separator](Split),
/// or in [param validation](crate::validation).
///
/// # Why a separate type?
///
/// A separate type is necessary to circumvent orphaning rules. We want to implement [`Split`]
/// and [`Validate`](crate::validation::Validate) for any type (e.g., [`LazyLock`]) that lazily initializes a `Regex`,
/// since a `Regex` on its own cannot be initialized in compile time. Similarly, such a type cannot
/// be dereferenced in compile time, which rules out implementing these traits for `&'static Regex`.
///
/// # Examples
///
/// The easiest way to initialize a wrapper is the [`lazy_regex!`] macro.
///
/// ```
/// use smart_config::{de::Delimited, pat::{lazy_regex, LazyRegex}};
/// # use smart_config::{DescribeConfig, DeserializeConfig};
///
/// static NAME_REGEX: LazyRegex = lazy_regex!(r"^[a-z][-a-z0-9]*$");
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(validate(NAME_REGEX))]
///     app: String,
///     // The macro also can be inlined!
///     #[config(with = Delimited::new(lazy_regex!(ref r"\s*,\s*")))]
///     numbers: Vec<u64>,
/// }
/// ```
pub struct LazyRegex<T = LazyLock<Regex>>(pub T);

/// Creates a [`LazyRegex`].
///
/// - If supplied a string literal, it will create [`LazyRegex`] from it.
/// - If the literal is prepended with `ref`, this will create a private static and reference it
///   (i.e., return `&'static LazyRegex`). This is useful for single-use regexes inlined into `config` attributes.
///
/// # Examples
///
/// See [`LazyRegex` docs](LazyRegex#examples) for the examples of usage.
#[macro_export]
macro_rules! lazy_regex {
    ($regex:tt) => {
        $crate::pat::LazyRegex(::std::sync::LazyLock::new(|| {
            $crate::pat::Regex::new($regex).unwrap()
        }))
    };
    (ref $regex:tt) => {{
        static __REGEX: $crate::pat::LazyRegex = $crate::pat::lazy_regex!($regex);
        const { &__REGEX }
    }};
}

pub use lazy_regex;

impl<T: fmt::Debug> fmt::Debug for LazyRegex<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

impl<T> Split for &'static LazyRegex<T>
where
    T: ops::Deref<Target = Regex> + Send + Sync,
{
    fn split_once<'s>(&self, haystack: &'s str) -> Option<(&'s str, &'s str)> {
        let mut it = self.0.splitn(haystack, 2);
        let head = it.next()?;
        let tail = it.next()?;
        Some((head, tail))
    }

    fn split<'s>(&self, haystack: &'s str) -> impl Iterator<Item = &'s str> {
        Regex::split(&self.0, haystack)
    }

    fn display(&self) -> PatternDisplay {
        PatternDisplay::Regex(self.0.as_str().to_owned())
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
