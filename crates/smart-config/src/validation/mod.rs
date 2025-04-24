//! Parameter and config validation.

#![allow(missing_docs)] // FIXME

use std::{fmt, ops, sync::Arc};

use serde::de;

use crate::ErrorWithOrigin;

#[doc(hidden)] // only used in proc macros
pub mod _private;

/// Generic post-validation for a configuration parameter.
pub trait Validate<T: ?Sized>: 'static + Send + Sync {
    /// # Errors
    ///
    /// Should propagate formatting errors.
    fn describe(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result;

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
