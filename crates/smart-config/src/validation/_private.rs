use std::{any, fmt, marker::PhantomData};

use crate::{ErrorWithOrigin, validation::Validate};

/// Tag for `WithDescription` wrapping a type that already implements a validation.
#[derive(Debug)]
pub struct Delegated(());

/// Tag for `WithDescription` wrapping a Boolean predicate.
#[derive(Debug)]
pub struct BoolPredicate(());

/// Tag for `WithDescription` wrapping a predicate of form `fn(&T) -> Result<(), ErrorWithOrigin>`.
#[derive(Debug)]
pub struct ResultPredicate(());

/// Wrapper for validation allowing to (re)define its description.
///
/// The `Kind` type param is inferred automatically by the compiler and allows to distinguish between
/// 3 types of wrappers currently supported.
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

/// Erases the validated type (`T`) from `Validate`.
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
