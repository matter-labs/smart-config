use serde::{
    de::{DeserializeOwned, Error as DeError},
    Deserialize,
};

use crate::{
    error::{ErrorWithOrigin, LocationInConfig},
    metadata::{ConfigMetadata, ParamMetadata},
    value::{Pointer, ValueOrigin, WithOrigin},
    DescribeConfig, ParseError, ParseErrors, ValueDeserializer,
};

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
    ) -> Option<Self> {
        Some(Self {
            root_value,
            path,
            current_config,
            errors,
        })
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

    fn push_error(&mut self, err: ErrorWithOrigin, location: Option<LocationInConfig>) {
        let mut origin = err.origin;
        if matches!(origin.as_ref(), ValueOrigin::Unknown) {
            if let Some(val) = self.current_value() {
                origin = val.origin.clone();
            }
        }

        let path = if let Some(location) = location {
            &Pointer(&self.path).join(match location {
                LocationInConfig::Param(idx) => self.current_config.params[idx].name,
            })
        } else {
            &self.path
        };

        let mut err = ParseError::from(err.inner)
            .with_path(path)
            .with_origin(Some(&origin))
            .for_config(Some(self.current_config));
        if let Some(location) = location {
            err = err.with_location(Some(self.current_config), location);
        }
        self.errors.push(err);
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

pub trait DeserializeParam<T> {
    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin>;
}

impl<T: DeserializeOwned> DeserializeParam<T> for () {
    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        T::deserialize(ctx.current_value_deserializer(param.name)?)
    }
}

#[derive(Debug)]
pub struct WithDefault<T, D> {
    inner: D,
    default: fn() -> T,
}

impl<T, D: DeserializeParam<T>> WithDefault<T, D> {
    pub const fn new(inner: D, default: fn() -> T) -> Self {
        Self { inner, default }
    }
}

impl<T, D: DeserializeParam<T>> DeserializeParam<T> for WithDefault<T, D> {
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

#[derive(Debug)]
struct TagDeserializer {
    expected: &'static [&'static str],
    default_value: Option<&'static str>,
}

impl DeserializeParam<&'static str> for TagDeserializer {
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
