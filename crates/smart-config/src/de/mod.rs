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
//! So, each config param is deserialized in isolation from an optional [`Value`](crate::value::Value) [`WithOrigin`]
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
//! [`BasicType`](crate::metadata::BasicType) and [`SchemaType`](crate::metadata::SchemaType) can deserialize
//! any param implementing [`serde::Deserialize`]. An important caveat is that these deserializers require
//! the input `Value` to be present; otherwise, they'll fail with a "missing value" error. As such,
//! for [`Option`]al types, it's necessary to wrap a deserializer in the [`Optional`] decorator.
//!
//! ## Durations and byte sizes
//!
//! [`TimeUnit`](crate::metadata::TimeUnit) and [`SizeUnit`](crate::metadata::SizeUnit) can deserialize [`Duration`]s
//! and [`ByteSize`](crate::ByteSize)s, respectively. See their docs for more details.
//!
//! [`Duration`]: std::time::Duration

use serde::de::Error as DeError;

use self::deserializer::ValueDeserializer;
pub use self::{
    deserializer::DeserializerOptions,
    param::{
        DeserializeParam, DeserializerWrapper, ObjectSafeDeserializer, Optional, TagDeserializer,
        WellKnown, WithDefault,
    },
};
use crate::{
    error::{ErrorWithOrigin, LocationInConfig},
    metadata::{ConfigMetadata, ParamMetadata},
    value::{Pointer, ValueOrigin, WithOrigin},
    DescribeConfig, ParseError, ParseErrors,
};

mod deserializer;
mod param;
#[cfg(feature = "primitive-types")]
mod primitive_types_impl;
#[cfg(test)]
mod tests;

/// Context for deserializing a configuration.
#[derive(Debug)]
pub struct DeserializeContext<'a> {
    de_options: &'a DeserializerOptions,
    root_value: &'a WithOrigin,
    path: String,
    current_config: &'static ConfigMetadata,
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
            current_config,
            errors,
        }
    }

    fn child(&mut self, path: &str) -> DeserializeContext<'_> {
        DeserializeContext {
            de_options: self.de_options,
            root_value: self.root_value,
            path: Pointer(&self.path).join(path),
            current_config: self.current_config,
            errors: self.errors,
        }
    }

    fn current_value(&self) -> Option<&'a WithOrigin> {
        self.root_value.get(Pointer(&self.path))
    }

    fn current_value_deserializer(
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
            ..self.child(path)
        }
    }

    fn for_param(&mut self, index: usize) -> (DeserializeContext<'_>, &'static ParamMetadata) {
        let param = self.current_config.params.get(index).unwrap_or_else(|| {
            panic!("Internal error: called `for_param()` with missing param index {index}")
        });
        (self.child(param.name), param)
    }

    #[cold]
    fn push_error(&mut self, err: ErrorWithOrigin, location: Option<LocationInConfig>) {
        let mut origin = err.origin;
        if matches!(origin.as_ref(), ValueOrigin::Unknown) {
            if let Some(val) = self.current_value() {
                origin = val.origin.clone();
            }
        }

        let path = if let Some(location) = location {
            Pointer(&self.path).join(match location {
                LocationInConfig::Param(idx) => self.current_config.params[idx].name,
            })
        } else {
            self.path.clone()
        };

        self.errors.push(ParseError {
            inner: err.inner,
            path,
            origin,
            config: self.current_config,
            location_in_config: location,
        });
    }
}

/// Methods used in proc macros. Not a part of public API.
#[doc(hidden)]
impl DeserializeContext<'_> {
    pub fn deserialize_nested_config<T: DeserializeConfig>(
        &mut self,
        index: usize,
        default_fn: Option<fn() -> T>,
    ) -> Option<T> {
        let child_ctx = self.for_nested_config(index);
        if child_ctx.current_value().is_none() {
            if let Some(default) = default_fn {
                return Some(default());
            }
        }
        T::deserialize_config(child_ctx)
    }

    pub fn deserialize_param<T: 'static>(&mut self, index: usize) -> Option<T> {
        let (child_ctx, param) = self.for_param(index);
        match param.deserializer.deserialize_param(child_ctx, param) {
            Ok(param) => Some(
                *param
                    .downcast()
                    .expect("Internal error: deserializer output has wrong type"),
            ),
            Err(err) => {
                self.push_error(err, Some(LocationInConfig::Param(index)));
                None
            }
        }
    }
}

/// Deserializes this configuration from the provided context.
pub trait DeserializeConfig: DescribeConfig + Sized {
    /// Performs deserialization. If it fails, the method should return `None` and maybe add
    /// errors to the context.
    fn deserialize_config(ctx: DeserializeContext<'_>) -> Option<Self>;
}
