//! Parameter and config validation.

#![allow(missing_docs)] // FIXME

use std::{any, fmt, marker::PhantomData};

use crate::ErrorWithOrigin;

/// Generic post-validation for a configuration parameter.
pub trait Validate<T: ?Sized>: 'static + Send + Sync + fmt::Display {
    /// # Errors
    ///
    /// Should return an error if validation fails.
    fn validate(&self, target: &T) -> Result<(), ErrorWithOrigin>;
}

impl<T: 'static + ?Sized> fmt::Debug for dyn Validate<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Validate")
            .field("description", &self.to_string())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct Custom<T>(pub &'static str, pub fn(&T) -> Result<(), ErrorWithOrigin>);

impl<T> fmt::Display for Custom<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl<T: 'static> Validate<T> for Custom<T> {
    fn validate(&self, target: &T) -> Result<(), ErrorWithOrigin> {
        (self.1)(target)
    }
}

#[derive(Debug)]
pub struct Basic<T>(pub &'static str, pub fn(&T) -> bool);

impl<T> fmt::Display for Basic<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl<T: 'static> Validate<T> for Basic<T> {
    fn validate(&self, target: &T) -> Result<(), ErrorWithOrigin> {
        if !(self.1)(target) {
            return Err(ErrorWithOrigin::custom(self.0));
        }
        Ok(())
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

impl<T: 'static, V: Validate<T>> fmt::Display for ErasedValidation<T, V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.validation, formatter)
    }
}

impl<T: 'static, V: Validate<T>> Validate<dyn any::Any> for ErasedValidation<T, V> {
    fn validate(&self, target: &dyn any::Any) -> Result<(), ErrorWithOrigin> {
        let target: &T = target
            .downcast_ref()
            .expect("Internal error: unexpected target type");
        self.validation.validate(target)
    }
}
