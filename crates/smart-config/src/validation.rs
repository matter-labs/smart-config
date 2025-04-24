//! Parameter and config validation.

#![allow(missing_docs)] // FIXME

use std::{any, fmt, marker::PhantomData, ops, sync::Arc};

use serde::de;

use crate::ErrorWithOrigin;

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
            .finish_non_exhaustive()
    }
}

impl<T: 'static + ?Sized> fmt::Display for dyn Validate<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.describe(formatter)
    }
}

#[derive(Debug)]
pub struct Custom<T>(pub &'static str, pub fn(&T) -> Result<(), ErrorWithOrigin>);

impl<T: 'static> Validate<T> for Custom<T> {
    fn describe(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }

    fn validate(&self, target: &T) -> Result<(), ErrorWithOrigin> {
        (self.1)(target)
    }
}

#[derive(Debug)]
pub struct Basic<T>(pub &'static str, pub fn(&T) -> bool);

impl<T: 'static> Validate<T> for Basic<T> {
    fn describe(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }

    fn validate(&self, target: &T) -> Result<(), ErrorWithOrigin> {
        if !(self.1)(target) {
            return Err(ErrorWithOrigin::custom(self.0));
        }
        Ok(())
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

#[doc(hidden)] // used in proc macros
#[derive(Debug)]
pub struct ErasedValidation<T, V> {
    validation: V,
    _ty: PhantomData<fn(&T)>,
}

impl<T: 'static, V: Validate<T>> ErasedValidation<T, V> {
    pub const fn new(validation: V) -> Self {
        Self {
            validation,
            _ty: PhantomData,
        }
    }
}

impl<T: 'static, V: Validate<T>> Validate<dyn any::Any> for ErasedValidation<T, V> {
    fn describe(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.validation.describe(formatter)
    }

    fn validate(&self, target: &dyn any::Any) -> Result<(), ErrorWithOrigin> {
        let target: &T = target
            .downcast_ref()
            .expect("Internal error: unexpected target type");
        self.validation.validate(target)
    }
}
