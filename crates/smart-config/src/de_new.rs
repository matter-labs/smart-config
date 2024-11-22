use serde::{
    de::{DeserializeOwned, Error as DeError},
    Deserialize,
};

use crate::{
    error::{ErrorWithOrigin, LocationInConfig},
    metadata::ConfigMetadata,
    value::{Pointer, WithOrigin},
    DescribeConfig, ParseError, ParseErrors, ValueDeserializer,
};

/// Context for deserializing a configuration.
#[derive(Debug)]
pub struct DeserializeContext<'a> {
    root_value: &'a WithOrigin,
    current_value: &'a WithOrigin,
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
        let current_value = root_value.get(Pointer(&path))?;
        Some(Self {
            root_value,
            current_value,
            path,
            current_config,
            errors,
        })
    }

    fn child(&mut self, path: &str) -> Option<DeserializeContext<'_>> {
        let child_value = if path.is_empty() {
            self.current_value
        } else {
            self.current_value
                .inner
                .as_object()
                .and_then(|map| map.get(path))?
        };
        Some(DeserializeContext {
            root_value: self.root_value,
            current_value: child_value,
            path: Pointer(&self.path).join(path),
            current_config: self.current_config,
            errors: self.errors,
        })
    }

    /// Returns context for a nested configuration.
    fn for_nested_config(&mut self, index: usize) -> Option<DeserializeContext<'_>> {
        let nested_meta = self.current_config.nested_configs.get(index).unwrap_or_else(|| {
            panic!("Internal error: called `for_nested_config()` with missing config index {index}")
        });
        let path = nested_meta.name;
        Some(DeserializeContext {
            current_config: nested_meta.meta,
            ..self.child(path)?
        })
    }

    fn for_param(&mut self, index: usize) -> Option<DeserializeContext<'_>> {
        let param = self.current_config.params.get(index).unwrap_or_else(|| {
            panic!("Internal error: called `for_param()` with missing param index {index}")
        });
        dbg!(index, param.name);
        self.child(param.name)
    }

    fn push_error(&mut self, err: ErrorWithOrigin, location: Option<LocationInConfig>) {
        let mut err = ParseError::from(err.inner)
            .with_path(&self.path)
            .with_origin(Some(&self.current_value.origin))
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
        err_if_missing: bool,
    ) -> Option<T> {
        match self.for_nested_config(index) {
            Some(child_ctx) => T::deserialize_config(child_ctx),
            None => {
                if err_if_missing {
                    let nested_meta = self.current_config.nested_configs[index];
                    self.push_error(
                        DeError::missing_field(nested_meta.name),
                        Some(LocationInConfig::Nested(index)),
                    );
                }
                None
            }
        }
    }

    pub fn deserialize_param<T, D: DeserializeParam<T>>(
        &mut self,
        index: usize,
        err_if_missing: bool,
        with: &D,
    ) -> Option<T> {
        match self.for_param(index) {
            Some(param_ctx) => match with.deserialize_param(param_ctx) {
                Ok(param) => Some(param),
                Err(err) => {
                    self.push_error(err, Some(LocationInConfig::Param(index)));
                    None
                }
            },
            None => {
                if err_if_missing {
                    let nested_meta = self.current_config.params[index];
                    self.push_error(
                        DeError::missing_field(nested_meta.name),
                        Some(LocationInConfig::Param(index)),
                    );
                }
                None
            }
        }
    }

    pub fn deserialize_tag(
        &mut self,
        index: usize,
        expected: &'static [&'static str], // TODO: record in meta?
        err_if_missing: bool,
    ) -> Option<&'static str> {
        self.deserialize_param(index, err_if_missing, &TagDeserializer { expected })
    }
}

pub trait DeserializeConfig: DescribeConfig + Sized {
    fn deserialize_config(ctx: DeserializeContext<'_>) -> Option<Self>;
}

pub trait DeserializeParam<T> {
    fn deserialize_param(&self, ctx: DeserializeContext<'_>) -> Result<T, ErrorWithOrigin>;
}

impl<T: DeserializeOwned> DeserializeParam<T> for () {
    fn deserialize_param(&self, ctx: DeserializeContext<'_>) -> Result<T, ErrorWithOrigin> {
        T::deserialize(ValueDeserializer::new(ctx.current_value))
    }
}

#[derive(Debug)]
struct TagDeserializer {
    expected: &'static [&'static str],
}

impl DeserializeParam<&'static str> for TagDeserializer {
    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
    ) -> Result<&'static str, ErrorWithOrigin> {
        let s = String::deserialize(ValueDeserializer::new(ctx.current_value))?;
        self.expected
            .iter()
            .copied()
            .find(|&variant| variant == s)
            .ok_or_else(|| {
                let err = DeError::unknown_variant(&s, self.expected);
                ErrorWithOrigin::new(err, ctx.current_value.origin.clone())
            })
    }
}
