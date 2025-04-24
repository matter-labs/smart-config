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

#[doc(hidden)]
#[derive(Debug)]
pub struct Delegated(());

#[doc(hidden)]
#[derive(Debug)]
pub struct BoolPredicate(());

#[doc(hidden)]
#[derive(Debug)]
pub struct ResultPredicate(());

#[doc(hidden)]
#[derive(Debug)]
pub struct WithDescription<V, Kind> {
    inner: V,
    description: &'static str,
    _kind: PhantomData<Kind>,
}

impl<V, Kind> WithDescription<V, Kind> {
    pub const fn new(inner: V, description: &'static str) -> Self {
        Self {
            inner,
            description,
            _kind: PhantomData,
        }
    }
}

impl<T, V: Validate<T>> Validate<T> for WithDescription<V, Delegated> {
    fn describe(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.description)
    }

    fn validate(&self, target: &T) -> Result<(), ErrorWithOrigin> {
        self.inner.validate(target)
    }
}

impl<T, F> Validate<T> for WithDescription<F, BoolPredicate>
where
    F: Fn(&T) -> bool + Send + Sync + 'static,
{
    fn describe(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.description)
    }

    fn validate(&self, target: &T) -> Result<(), ErrorWithOrigin> {
        if !(self.inner)(target) {
            return Err(ErrorWithOrigin::custom(self.description));
        }
        Ok(())
    }
}

impl<T, F> Validate<T> for WithDescription<F, ResultPredicate>
where
    F: Fn(&T) -> Result<(), ErrorWithOrigin> + Send + Sync + 'static,
{
    fn describe(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.description)
    }

    fn validate(&self, target: &T) -> Result<(), ErrorWithOrigin> {
        (self.inner)(target)
    }
}

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
