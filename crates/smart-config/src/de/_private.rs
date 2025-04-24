//! Private functionality used by derive macros. Not part of the public API.

use std::{any, fmt, marker::PhantomData};

use serde::{de::Error as DeError, Deserialize};

use super::{deserializer::ValueDeserializer, DeserializeContext, DeserializeParam};
use crate::{
    error::ErrorWithOrigin,
    metadata::{BasicTypes, ParamMetadata, TypeDescription},
    validation::Validate,
};

pub const fn extract_expected_types<T, De: DeserializeParam<T>>(_: &De) -> BasicTypes {
    <De as DeserializeParam<T>>::EXPECTING
}

/// Erased counterpart of a parameter deserializer. Stored in param metadata.
pub trait ErasedDeserializer: fmt::Debug + Send + Sync + 'static {
    fn describe(&self, description: &mut TypeDescription);

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Box<dyn any::Any>, ErrorWithOrigin>;
}

/// Wrapper transforming [`DeserializeParam`] to [`ErasedDeserializer`].
pub struct Erased<T, De> {
    inner: De,
    _ty: PhantomData<fn(T)>,
}

impl<T, D: fmt::Debug> fmt::Debug for Erased<T, D> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Erased").field(&self.inner).finish()
    }
}

impl<T: 'static, De: DeserializeParam<T>> Erased<T, De> {
    pub const fn new(inner: De) -> Self {
        Self {
            inner,
            _ty: PhantomData,
        }
    }
}

impl<T: 'static, De: DeserializeParam<T>> ErasedDeserializer for Erased<T, De> {
    fn describe(&self, description: &mut TypeDescription) {
        self.inner.describe(description);
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Box<dyn any::Any>, ErrorWithOrigin> {
        self.inner
            .deserialize_param(ctx, param)
            .map(|val| Box::new(val) as _)
    }
}

/// Deserializer for enum tags.
#[derive(Debug)]
pub struct TagDeserializer {
    expected: &'static [&'static str],
    default_value: Option<&'static str>,
}

impl TagDeserializer {
    pub const fn new(
        expected: &'static [&'static str],
        default_value: Option<&'static str>,
    ) -> Self {
        Self {
            expected,
            default_value,
        }
    }
}

impl DeserializeParam<&'static str> for TagDeserializer {
    const EXPECTING: BasicTypes = BasicTypes::STRING;

    fn describe(&self, description: &mut TypeDescription) {
        description.set_details(format!("one of {:?}", self.expected));
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<&'static str, ErrorWithOrigin> {
        let s = if let Some(current_value) = ctx.current_value() {
            String::deserialize(ValueDeserializer::new(current_value, ctx.de_options))?
        } else if let Some(default) = self.default_value {
            return Ok(default);
        } else {
            return Err(DeError::missing_field(param.name));
        };

        self.expected
            .iter()
            .copied()
            .find(|&variant| variant == s)
            .ok_or_else(|| {
                let err = DeError::unknown_variant(&s, self.expected);
                let origin = ctx
                    .current_value()
                    .map(|val| val.origin.clone())
                    .unwrap_or_default();
                ErrorWithOrigin::json(err, origin)
            })
    }
}

/// Wrapper for params with post-validations.
pub struct Validated<T: 'static, De> {
    inner: De,
    validations: &'static [&'static dyn Validate<T>],
}

impl<T: 'static, De: fmt::Debug> fmt::Debug for Validated<T, De> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Validated")
            .field("inner", &self.inner)
            .field("validations", &self.validations)
            .finish()
    }
}

impl<T, De> Validated<T, De> {
    pub const fn new(inner: De, validations: &'static [&'static dyn Validate<T>]) -> Self {
        Self { inner, validations }
    }
}

fn validate<T>(value: &T, validations: &[&'static dyn Validate<T>]) -> Result<(), ErrorWithOrigin> {
    for &validation in validations {
        let _span = tracing::trace_span!("validation", %validation).entered();
        if let Err(err) = validation.validate(value) {
            tracing::warn!(%validation, %err, "validation failed");
            return Err(err);
        }
    }
    Ok(())
}

impl<T, De: DeserializeParam<T>> DeserializeParam<T> for Validated<T, De> {
    const EXPECTING: BasicTypes = De::EXPECTING;

    fn describe(&self, description: &mut TypeDescription) {
        self.inner.describe(description);
        description.set_validations(self.validations);
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        let value = self.inner.deserialize_param(ctx, param)?;
        validate(&value, self.validations)?;
        Ok(value)
    }
}

impl<T, De: DeserializeParam<Option<T>>> DeserializeParam<Option<T>> for Validated<T, De> {
    const EXPECTING: BasicTypes = De::EXPECTING;

    fn describe(&self, description: &mut TypeDescription) {
        self.inner.describe(description);
        description.set_validations(self.validations);
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Option<T>, ErrorWithOrigin> {
        let value = self.inner.deserialize_param(ctx, param)?;
        if let Some(value) = &value {
            validate(value, self.validations)?;
        }
        Ok(value)
    }
}
