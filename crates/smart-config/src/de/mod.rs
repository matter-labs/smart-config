use serde::de::Error as DeError;

use self::deserializer::ValueDeserializer;
pub use self::param::{
    DeserializeParam, DeserializerWrapper, ObjectSafeDeserializer, Optional, TagDeserializer,
    WellKnown, WithDefault,
};
use crate::{
    error::{ErrorWithOrigin, LocationInConfig},
    metadata::{ConfigMetadata, ParamMetadata},
    value::{Pointer, ValueOrigin, WithOrigin},
    DescribeConfig, ParseError, ParseErrors,
};

mod deserializer;
mod param;
#[cfg(test)]
mod tests;

/// Context for deserializing a configuration.
#[derive(Debug)]
pub struct DeserializeContext<'a> {
    root_value: &'a WithOrigin,
    path: String,
    current_config: &'static ConfigMetadata,
    errors: &'a mut ParseErrors,
}

impl<'a> DeserializeContext<'a> {
    pub(crate) fn new(
        root_value: &'a WithOrigin,
        path: String,
        current_config: &'static ConfigMetadata,
        errors: &'a mut ParseErrors,
    ) -> Self {
        Self {
            root_value,
            path,
            current_config,
            errors,
        }
    }

    fn child(&mut self, path: &str) -> DeserializeContext<'_> {
        DeserializeContext {
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
            Ok(ValueDeserializer::new(value))
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

pub trait DeserializeConfig: DescribeConfig + Sized {
    fn deserialize_config(ctx: DeserializeContext<'_>) -> Option<Self>;
}
