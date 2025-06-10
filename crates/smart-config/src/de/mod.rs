//! Configuration deserialization logic.
//!
//! # How it works
//!
//! [`DeserializeConfig`] derive macro visits all config parameters invoking associated [`DeserializeParam`]
//! implementations. Unlike `serde` deserialization, deserialization does not stop early on error (we want to get
//! errors for all params). Nested / flattened configs do not use `serde` either for a couple of reasons:
//!
//! - To reach all params regardless of encountered errors as mentioned above
//! - `serde` sometimes collects params in intermediate containers (e.g., in structs with `#[serde(flatten)]`
//!   or in tagged enums), which leads to param deserialization potentially getting broken in unpredictable ways.
//!
//! So, each config param is deserialized in isolation from an optional [`Value`] [`WithOrigin`]
//! encapsulated in [`DeserializeContext`].
//!
//! # Deserializers
//!
//! The default deserializer is extracted from the param type with the help of [`WellKnown`] trait.
//! If you have a custom type defined locally which you want to use in configs, the easiest solution
//! would be to implement `WellKnown` for it.
//! Alternatively, it's possible to specify a custom deserializer using `#[config(with = _)]` attribute.
//!
//! ## Universal deserializers
//!
//! [`Serde`](struct@Serde) (usually instantiated via [the eponymous macro](macro@Serde)) can deserialize
//! any param implementing [`serde::Deserialize`]. An important caveat is that these deserializers require
//! the input `Value` to be present; otherwise, they'll fail with a "missing value" error. As such,
//! for [`Option`]al types, it's necessary to wrap a deserializer in the [`Optional`] decorator.
//!
//! ## Durations and byte sizes
//!
//! [`Duration`]s and [`ByteSize`]s can be deserialized in two ways:
//!
//! - By default, they are deserialized from an integer + unit either encapsulated in a string like "200ms" or
//!   in a single-key object like `{ "mb": 4 }`. See [`WithUnit`] for more details.
//! - Alternatively, [`TimeUnit`](crate::metadata::TimeUnit) and [`SizeUnit`](crate::metadata::SizeUnit) can be used
//!   on `Duration`s and `ByteSize`s, respectively.
//!
//! ## Secrets
//!
//! A param is secret iff it uses a [`Secret`] deserializer (perhaps, with decorators on top, like
//! [`Optional`] / [`WithDefault`]). Secret params must be deserializable from a string; this is because
//! strings are the only type of secret values currently supported.
//!
//! Secret values are wrapped in opaque, zero-on-drop wrappers during source preprocessing so that
//! they do not get accidentally exposed via debug logs etc. See [`ConfigRepository`](crate::ConfigRepository)
//! for details.
//!
//! [`Duration`]: std::time::Duration
//! [`ByteSize`]: crate::ByteSize

use std::any;

use serde::de::Error as DeError;

use self::deserializer::ValueDeserializer;
pub use self::{
    deserializer::DeserializerOptions,
    macros::Serde,
    param::{DeserializeParam, Optional, OrString, Qualified, Serde, WellKnown, WithDefault},
    repeated::{Delimited, Entries, NamedEntries, Repeated},
    secret::{FromSecretString, Secret},
    units::WithUnit,
};
use crate::{
    error::{ErrorWithOrigin, LocationInConfig, LowLevelError},
    metadata::{BasicTypes, ConfigMetadata, ParamMetadata},
    value::{Pointer, StrValue, Value, ValueOrigin, WithOrigin},
    DescribeConfig, DeserializeConfigError, ParseError, ParseErrors,
};

#[doc(hidden)]
pub mod _private;
mod deserializer;
mod macros;
mod param;
#[cfg(feature = "primitive-types")]
mod primitive_types_impl;
mod repeated;
mod secret;
#[cfg(test)]
mod tests;
mod units;

/// Context for deserializing a configuration.
#[derive(Debug)]
pub struct DeserializeContext<'a> {
    de_options: &'a DeserializerOptions,
    root_value: &'a WithOrigin,
    path: String,
    patched_current_value: Option<&'a WithOrigin>,
    current_config: &'static ConfigMetadata,
    location_in_config: Option<LocationInConfig>,
    errors: &'a mut ParseErrors,
}

impl<'a> DeserializeContext<'a> {
    pub(crate) fn new(
        de_options: &'a DeserializerOptions,
        root_value: &'a WithOrigin,
        path: String,
        current_config: &'static ConfigMetadata,
        errors: &'a mut ParseErrors,
    ) -> Self {
        Self {
            de_options,
            root_value,
            path,
            patched_current_value: None,
            current_config,
            location_in_config: None,
            errors,
        }
    }

    fn child(
        &mut self,
        path: &str,
        location_in_config: Option<LocationInConfig>,
    ) -> DeserializeContext<'_> {
        DeserializeContext {
            de_options: self.de_options,
            root_value: self.root_value,
            path: Pointer(&self.path).join(path),
            patched_current_value: self.patched_current_value.and_then(|val| {
                if path.is_empty() {
                    Some(val)
                } else if let Value::Object(object) = &val.inner {
                    object.get(path)
                } else {
                    None
                }
            }),
            current_config: self.current_config,
            location_in_config,
            errors: self.errors,
        }
    }

    /// Mutably borrows this context with a shorter lifetime.
    pub fn borrow(&mut self) -> DeserializeContext<'_> {
        DeserializeContext {
            de_options: self.de_options,
            root_value: self.root_value,
            path: self.path.clone(),
            patched_current_value: self.patched_current_value,
            current_config: self.current_config,
            location_in_config: self.location_in_config,
            errors: self.errors,
        }
    }

    /// Allows to pretend that `current_value` is as supplied.
    fn patched<'s>(&'s mut self, current_value: &'s WithOrigin) -> DeserializeContext<'s> {
        DeserializeContext {
            de_options: self.de_options,
            root_value: self.root_value,
            path: self.path.clone(),
            patched_current_value: Some(current_value),
            current_config: self.current_config,
            location_in_config: self.location_in_config,
            errors: self.errors,
        }
    }

    pub(crate) fn current_value(&self) -> Option<&'a WithOrigin> {
        self.patched_current_value
            .or_else(|| self.root_value.get(Pointer(&self.path)))
    }

    /// Returns a `serde` deserializer for the current value.
    ///
    /// # Errors
    ///
    /// Returns an error if the current value is missing.
    pub fn current_value_deserializer(
        &self,
        name: &'static str,
    ) -> Result<ValueDeserializer<'a>, ErrorWithOrigin> {
        if let Some(value) = self.current_value() {
            Ok(ValueDeserializer::new(value, self.de_options))
        } else {
            Err(DeError::missing_field(name))
        }
    }

    /// Returns context for a nested configuration.
    fn for_nested_config(&mut self, index: usize) -> DeserializeContext<'_> {
        let nested_meta = self.current_config.nested_configs.get(index).unwrap_or_else(|| {
            panic!("Internal error: called `for_nested_config()` with missing config index {index}")
        });
        let path = nested_meta.name;
        DeserializeContext {
            current_config: nested_meta.meta,
            ..self.child(path, None)
        }
    }

    fn for_param(&mut self, index: usize) -> (DeserializeContext<'_>, &'static ParamMetadata) {
        let param = self.current_config.params.get(index).unwrap_or_else(|| {
            panic!("Internal error: called `for_param()` with missing param index {index}")
        });
        (
            self.child(param.name, Some(LocationInConfig::Param(index))),
            param,
        )
    }

    /// Pushes a deserialization error into the context.
    pub fn push_error(&mut self, err: ErrorWithOrigin) {
        self.push_generic_error(err, None);
    }

    #[cold]
    fn push_generic_error(&mut self, err: ErrorWithOrigin, validation: Option<String>) {
        let (inner, category) = match err.inner {
            LowLevelError::Json { err, category } => (err, category),
            LowLevelError::InvalidArray
            | LowLevelError::InvalidObject
            | LowLevelError::Validation => return,
        };

        let mut origin = err.origin;
        if matches!(origin.as_ref(), ValueOrigin::Unknown) {
            if let Some(val) = self.current_value() {
                origin = val.origin.clone();
            }
        }

        self.errors.push(ParseError {
            inner,
            category,
            path: self.path.clone(),
            origin,
            config: self.current_config,
            location_in_config: self.location_in_config,
            validation,
        });
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(path = self.path, config = ?self.current_config.ty)
    )]
    pub(crate) fn deserialize_any_config(
        mut self,
    ) -> Result<Box<dyn any::Any>, DeserializeConfigError> {
        // It is technically possible to coerce a value to an object here, but this would make merging sources not obvious:
        // should a config specified as a string override / be overridden atomically? (Probably not, but if so, it needs to be coerced to an object
        // before the merge, potentially recursively.)

        if let Some(val) = self.current_value() {
            if !matches!(&val.inner, Value::Object(_)) {
                self.push_error(val.invalid_type("config object"));
                return Err(DeserializeConfigError::new());
            }
        }
        let config = (self.current_config.deserializer)(self.borrow())?;

        let mut has_errors = false;
        for &validation in self.current_config.validations {
            let _span = tracing::trace_span!("validation", %validation).entered();
            if let Err(err) = validation.validate(config.as_ref()) {
                tracing::info!(%validation, origin = %err.origin, "config validation failed: {}", err.inner);
                self.push_generic_error(err, Some(validation.to_string()));
                has_errors = true;
            }
        }

        if has_errors {
            Err(DeserializeConfigError::new())
        } else {
            Ok(config)
        }
    }

    /// Caller is responsible to downcast the config to the correct type.
    pub(crate) fn deserialize_config<C: 'static>(self) -> Result<C, DeserializeConfigError> {
        Ok(*self
            .deserialize_any_config()?
            .downcast::<C>()
            .expect("Internal error: config deserializer output has wrong type"))
    }
}

/// Methods used in proc macros. Not a part of public API.
#[doc(hidden)]
impl DeserializeContext<'_> {
    pub fn deserialize_nested_config<T: DeserializeConfig>(
        &mut self,
        index: usize,
        default_fn: Option<fn() -> T>,
    ) -> Result<T, DeserializeConfigError> {
        let child_ctx = self.for_nested_config(index);
        if child_ctx.current_value().is_none() {
            if let Some(default) = default_fn {
                return Ok(default());
            }
        }
        child_ctx.deserialize_config()
    }

    pub fn deserialize_nested_config_opt<T: DeserializeConfig>(
        &mut self,
        index: usize,
    ) -> Result<Option<T>, DeserializeConfigError> {
        let child_ctx = self.for_nested_config(index);
        if child_ctx.current_value().is_none() {
            return Ok(None);
        }
        child_ctx.deserialize_config().map(Some)
    }

    #[tracing::instrument(
        level = "trace",
        name = "deserialize_param",
        skip_all,
        fields(path = self.path, config = ?self.current_config.ty, param)
    )]
    pub(crate) fn deserialize_any_param(
        &mut self,
        index: usize,
    ) -> Result<Box<dyn any::Any>, DeserializeConfigError> {
        let (mut child_ctx, param) = self.for_param(index);
        tracing::Span::current().record("param", param.rust_field_name);

        // Coerce value to the expected type.
        let maybe_coerced = child_ctx
            .current_value()
            .and_then(|val| val.coerce_value_type(param.expecting));
        let mut child_ctx = if let Some(coerced) = &maybe_coerced {
            child_ctx.patched(coerced)
        } else {
            child_ctx
        };
        tracing::trace!(
            deserializer = ?param.deserializer,
            value = ?child_ctx.current_value(),
            "deserializing param"
        );

        match param
            .deserializer
            .deserialize_param(child_ctx.borrow(), param)
        {
            Ok(param) => Ok(param),
            Err(err) => {
                tracing::info!(origin = %err.origin, "deserialization failed: {}", err.inner);
                child_ctx.push_error(err);
                Err(DeserializeConfigError::new())
            }
        }
    }

    pub fn deserialize_param<T: 'static>(
        &mut self,
        index: usize,
    ) -> Result<T, DeserializeConfigError> {
        self.deserialize_any_param(index).map(|val| {
            *val.downcast()
                .expect("Internal error: deserializer output has wrong type")
        })
    }
}

impl WithOrigin {
    #[tracing::instrument(level = "trace", skip(self))]
    fn coerce_value_type(&self, expecting: BasicTypes) -> Option<Self> {
        let Value::String(StrValue::Plain(str)) = &self.inner else {
            return None; // we only know how to coerce strings so far
        };

        // Attempt to transform the type to the expected type
        match expecting {
            // We intentionally use exact comparisons; if a type supports multiple primitive representations,
            // we do nothing.
            BasicTypes::BOOL => match str.parse::<bool>() {
                Ok(bool_value) => {
                    return Some(Self::new(bool_value.into(), self.origin.clone()));
                }
                Err(err) => {
                    tracing::info!(%expecting, "failed coercing value: {err}");
                }
            },
            BasicTypes::INTEGER | BasicTypes::FLOAT => match str.parse::<serde_json::Number>() {
                Ok(number) => {
                    return Some(Self::new(number.into(), self.origin.clone()));
                }
                Err(err) => {
                    tracing::info!(%expecting, "failed coercing value: {err}");
                }
            },
            _ => { /* do nothing */ }
        }
        None
    }
}

/// Deserializes this configuration from the provided context.
pub trait DeserializeConfig: DescribeConfig + Sized {
    /// Performs deserialization.
    ///
    /// # Errors
    ///
    /// Returns an error marker if deserialization fails for at least one of recursively contained params.
    /// Error info should is contained in the context.
    fn deserialize_config(ctx: DeserializeContext<'_>) -> Result<Self, DeserializeConfigError>;
}
