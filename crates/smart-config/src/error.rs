//! Config deserialization errors.

use std::{fmt, sync::Arc};

use serde::de;

use crate::{
    metadata::{ConfigMetadata, ParamMetadata},
    value::{ValueOrigin, WithOrigin},
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum LocationInConfig {
    Param(usize),
}

pub(crate) type ErrorWithOrigin = WithOrigin<serde_json::Error>;

impl de::Error for ErrorWithOrigin {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self {
            inner: de::Error::custom(msg),
            origin: Arc::default(),
        }
    }
}

impl fmt::Display for ErrorWithOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}]: {}", self.origin, self.inner)
    }
}

impl std::error::Error for ErrorWithOrigin {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
    }
}

/// Config deserialization errors.
pub struct ParseError {
    pub(crate) inner: serde_json::Error,
    pub(crate) path: String,
    pub(crate) origin: Arc<ValueOrigin>,
    pub(crate) config: &'static ConfigMetadata,
    pub(crate) location_in_config: Option<LocationInConfig>,
}

impl fmt::Debug for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ParseError")
            .field("inner", &self.inner)
            .field("origin", &self.origin)
            .field("path", &self.path)
            .field("config.ty", &self.config.ty)
            .field("location_in_config", &self.location_in_config)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let field = self.location_in_config.and_then(|location| {
            Some(match location {
                LocationInConfig::Param(idx) => {
                    let param = self.config.params.get(idx)?;
                    format!("param `{}`", param.name)
                }
            })
        });
        let field = field.as_deref().unwrap_or("value");

        let origin = if matches!(self.origin(), ValueOrigin::Unknown) {
            String::new()
        } else {
            format!(" [origin: {}]", self.origin)
        };

        write!(
            formatter,
            "error parsing {field} in `{config}` at `{path}`{origin}: {err}",
            err = self.inner,
            config = self.config.ty.name_in_code(),
            path = self.path
        )
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
    }
}

impl ParseError {
    /// Returns the wrapped error.
    pub fn inner(&self) -> &serde_json::Error {
        &self.inner
    }

    /// Returns an absolute path on which this error has occurred.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns an origin of the value deserialization of which failed.
    pub fn origin(&self) -> &ValueOrigin {
        &self.origin
    }

    /// Returns metadata for the failing config.
    pub fn config(&self) -> &'static ConfigMetadata {
        self.config
    }

    /// Returns metadata for the failing parameter if this error concerns a parameter. The parameter
    /// is guaranteed to be contained in [`Self::config()`].
    pub fn param(&self) -> Option<&'static ParamMetadata> {
        let LocationInConfig::Param(idx) = self.location_in_config?;
        self.config.params.get(idx)
    }
}

#[derive(Debug, Default)]
pub struct ParseErrors {
    errors: Vec<ParseError>,
}

impl ParseErrors {
    #[doc(hidden)]
    pub fn push(&mut self, err: ParseError) {
        self.errors.push(err);
    }

    pub fn iter(&self) -> impl Iterator<Item = &ParseError> + '_ {
        self.errors.iter()
    }

    #[allow(clippy::len_without_is_empty)] // is_empty should always return false
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    pub fn first(&self) -> &ParseError {
        self.errors.first().expect("no errors")
    }
}

impl fmt::Display for ParseErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for err in &self.errors {
            writeln!(formatter, "{err}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseErrors {}
