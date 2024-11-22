use std::{any, fmt, marker::PhantomData};

use ::serde::{
    de::{DeserializeOwned, Error as DeError},
    Deserialize,
};

use self::serde::ValueDeserializer;
use crate::{
    error::{ErrorWithOrigin, LocationInConfig},
    metadata::{BasicType, ConfigMetadata, ParamMetadata, SchemaType},
    value::{Pointer, ValueOrigin, WithOrigin},
    DescribeConfig, ParseError, ParseErrors,
};

mod serde;
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

    pub fn deserialize_param<T, D: DeserializeParam<T>>(
        &mut self,
        index: usize,
        with: &D,
    ) -> Option<T> {
        let (child_ctx, param) = self.for_param(index);
        match with.deserialize_param(child_ctx, param) {
            Ok(param) => Some(param),
            Err(err) => {
                self.push_error(err, Some(LocationInConfig::Param(index)));
                None
            }
        }
    }

    pub fn deserialize_tag(
        &mut self,
        index: usize,
        expected: &'static [&'static str], // TODO: record in meta?
        default_value: Option<&'static str>,
    ) -> Option<&'static str> {
        self.deserialize_param(
            index,
            &TagDeserializer {
                expected,
                default_value,
            },
        )
    }
}

pub trait DeserializeConfig: DescribeConfig + Sized {
    fn deserialize_config(ctx: DeserializeContext<'_>) -> Option<Self>;
}

pub trait DeserializeParam<T>: fmt::Debug + Send + Sync + 'static {
    fn expecting(&self) -> SchemaType;

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin>;
}

#[derive(Debug)]
pub struct Serde(pub BasicType);

impl<T: DeserializeOwned> DeserializeParam<T> for Serde {
    fn expecting(&self) -> SchemaType {
        SchemaType::new(self.0)
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        T::deserialize(ctx.current_value_deserializer(param.name)?)
    }
}

pub struct WellKnown<T>(PhantomData<fn(T)>);

#[allow(clippy::new_without_default)] // won't make much sense, since it cannot be used in const contexts
impl<T: 'static> WellKnown<T> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: 'static> fmt::Debug for WellKnown<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WellKnown")
            .field("type", &any::type_name::<T>())
            .finish()
    }
}

impl<T: 'static + DeserializeOwned> DeserializeParam<T> for WellKnown<T> {
    fn expecting(&self) -> SchemaType {
        SchemaType::ANY // FIXME
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        T::deserialize(ctx.current_value_deserializer(param.name)?)
    }
}

pub struct WithDefault<T, D> {
    inner: D,
    default: fn() -> T,
}

impl<T: 'static, D: fmt::Debug> fmt::Debug for WithDefault<T, D> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WithDefault")
            .field("inner", &self.inner)
            .field("type", &any::type_name::<T>())
            .finish()
    }
}

impl<T: 'static, D: DeserializeParam<T>> WithDefault<T, D> {
    pub const fn new(inner: D, default: fn() -> T) -> Self {
        Self { inner, default }
    }
}

impl<T: 'static, D: DeserializeParam<T>> DeserializeParam<T> for WithDefault<T, D> {
    fn expecting(&self) -> SchemaType {
        self.inner.expecting()
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        if ctx.current_value().is_some() {
            self.inner.deserialize_param(ctx, param)
        } else {
            Ok((self.default)())
        }
    }
}

#[doc(hidden)] // Implementation detail
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
    fn expecting(&self) -> SchemaType {
        SchemaType::new(BasicType::String)
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<&'static str, ErrorWithOrigin> {
        let s = if let Some(current_value) = ctx.current_value() {
            String::deserialize(ValueDeserializer::new(current_value))?
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
                ErrorWithOrigin::new(err, origin)
            })
    }
}

/// Object-safe part of parameter deserializer. Stored in param metadata.
pub trait ObjectSafeDeserializer: 'static + fmt::Debug + Send + Sync {
    fn expecting(&self) -> SchemaType;

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Box<dyn any::Any>, ErrorWithOrigin>;
}

#[doc(hidden)]
pub struct DeserializerWrapper<T, D> {
    inner: D,
    _ty: PhantomData<fn(T)>,
}

impl<T, D: fmt::Debug> fmt::Debug for DeserializerWrapper<T, D> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("DeserializerWrapper")
            .field(&self.inner)
            .finish()
    }
}

impl<T: 'static, D: DeserializeParam<T>> DeserializerWrapper<T, D> {
    pub const fn new(inner: D) -> Self {
        Self {
            inner,
            _ty: PhantomData,
        }
    }
}

impl<T: 'static, D: DeserializeParam<T>> ObjectSafeDeserializer for DeserializerWrapper<T, D> {
    fn expecting(&self) -> SchemaType {
        self.inner.expecting()
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
