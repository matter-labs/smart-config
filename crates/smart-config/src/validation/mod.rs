//! Parameter and config validation.
//!
//! # Overview
//!
//! The core validation functionality is encapsulated in the [`Validate`] trait.
//!
//! # Examples
//!
//! ```
//! use secrecy::{ExposeSecret, SecretString};
//! use smart_config::validation;
//! # use smart_config::{testing, DescribeConfig, DeserializeConfig, ErrorWithOrigin};
//!
//! #[derive(DescribeConfig, DeserializeConfig)]
//! #[config(validate(
//!     Self::validate_secret_key,
//!     "secret key must have expected length"
//! ))]
//! struct ValidatedConfig {
//!     secret_key: SecretString,
//!     /// Reference key length. If specified, the secret key length
//!     /// will be checked against it.
//!     #[config(validate(..=100))]
//!     // ^ Validates that the value is in the range. Note that validations
//!     // handle `Option`s intelligently; if the value isn't specified
//!     // (i.e., is `None`), it will pass validation.
//!     secret_key_len: Option<usize>,
//!     #[config(validate(not_empty, "must not be empty"))]
//!     app_name: String,
//! }
//!
//! // We have to use `&String` rather than more idiomatic `&str` in order to
//! // exactly match the validated type.
//! fn not_empty(s: &String) -> bool {
//!     !s.is_empty()
//! }
//!
//! impl ValidatedConfig {
//!     fn validate_secret_key(&self) -> Result<(), ErrorWithOrigin> {
//!         if let Some(expected_len) = self.secret_key_len {
//!             let actual_len = self.secret_key.expose_secret().len();
//!             if expected_len != actual_len {
//!                 return Err(ErrorWithOrigin::custom(format!(
//!                     "unexpected `secret_key` length ({actual_len}); \
//!                      expected {expected_len}"
//!                 )));
//!             }
//!         }
//!         Ok(())
//!     }
//! }
//! ```

use std::{fmt, ops, sync::Arc};

use serde::de;

use crate::ErrorWithOrigin;

#[doc(hidden)] // only used in proc macros
pub mod _private;

/// Generic post-validation for a configuration parameter or a config.
///
/// # Implementations
///
/// Validations are implemented for the following types:
///
/// - [`Range`](ops::Range), [`RangeInclusive`](ops::RangeInclusive) etc. Validate whether the type is within the provided bounds.
pub trait Validate<T: ?Sized>: 'static + Send + Sync {
    /// Describes this validation.
    ///
    /// # Errors
    ///
    /// Should propagate formatting errors.
    fn describe(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result;

    /// Validates a parameter / config.
    ///
    /// # Errors
    ///
    /// Should return an error if validation fails.
    fn validate(&self, target: &T) -> Result<(), ErrorWithOrigin>;
}

impl<T: 'static + ?Sized> fmt::Debug for dyn Validate<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Validate")
            .field(&self.to_string())
            .finish()
    }
}

impl<T: 'static + ?Sized> fmt::Display for dyn Validate<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.describe(formatter)
    }
}

macro_rules! impl_validate_for_range {
    ($range:path) => {
        impl<T> Validate<T> for $range
        where
            T: 'static + Send + Sync + PartialOrd + fmt::Debug,
        {
            fn describe(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "must be in range {self:?}")
            }

            fn validate(&self, target: &T) -> Result<(), ErrorWithOrigin> {
                if !self.contains(target) {
                    let err = de::Error::invalid_value(
                        de::Unexpected::Other(&format!("{target:?}")),
                        &format!("value in range {self:?}").as_str(),
                    );
                    return Err(ErrorWithOrigin::json(err, Arc::default()));
                }
                Ok(())
            }
        }
    };
}

impl_validate_for_range!(ops::Range<T>);
impl_validate_for_range!(ops::RangeInclusive<T>);
impl_validate_for_range!(ops::RangeTo<T>);
impl_validate_for_range!(ops::RangeToInclusive<T>);
impl_validate_for_range!(ops::RangeFrom<T>);
