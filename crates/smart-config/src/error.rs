//! Config deserialization errors.

use std::{fmt, sync::Arc};

use serde::de;

use crate::{
    metadata::{ConfigMetadata, NestedConfigMetadata, ParamMetadata},
    value::{ValueOrigin, WithOrigin},
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum LocationInConfig {
    Param(usize),
    Nested(usize),
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
    inner: serde_json::Error,
    // FIXME: make mandatory
    path: Option<String>,
    origin: Option<Arc<ValueOrigin>>,
    config: Option<&'static ConfigMetadata>,
    location_in_config: Option<LocationInConfig>,
}

impl fmt::Debug for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ParseError")
            .field("inner", &self.inner)
            .field("origin", &self.origin)
            .field("path", &self.path)
            .field("config.ty", &self.config.map(|meta| meta.ty))
            .field("location_in_config", &self.location_in_config)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let field = self.location_in_config.and_then(|location| {
            Some(match location {
                LocationInConfig::Param(idx) => {
                    let param = self.config?.params.get(idx)?;
                    format!("param `{}`", param.name)
                }
                LocationInConfig::Nested(idx) => {
                    let nested = self.config?.nested_configs.get(idx)?;
                    format!("nested config `{}`", nested.meta.ty.name_in_code())
                }
            })
        });
        let field = field.as_deref().unwrap_or("value");
        let config = self.config.map_or_else(String::new, |config| {
            format!(" in `{}`", config.ty.name_in_code())
        });
        let at = self
            .path
            .as_ref()
            .map_or_else(String::new, |path| format!(" at `{path}`"));
        let origin = self
            .origin
            .as_ref()
            .map_or_else(String::new, |origin| format!(" [origin: {origin}]"));

        write!(
            formatter,
            "error parsing {field}{config}{at}{origin}: {err}",
            err = self.inner
        )
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
    }
}

impl From<serde_json::Error> for ParseError {
    fn from(err: serde_json::Error) -> Self {
        Self {
            inner: err,
            origin: None,
            path: None,
            config: None,
            location_in_config: None,
        }
    }
}

impl ParseError {
    /// Returns the wrapped error.
    pub fn inner(&self) -> &serde_json::Error {
        &self.inner
    }

    /// Returns an absolute path on which this error has occurred, if any.
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Returns an origin of the value deserialization of which failed, if any.
    pub fn origin(&self) -> Option<&ValueOrigin> {
        self.origin.as_deref()
    }

    /// Returns metadata for the failing config, if any.
    pub fn config(&self) -> Option<&'static ConfigMetadata> {
        self.config
    }

    /// Returns metadata for the failing parameter if this error concerns a parameter. The parameter
    /// is guaranteed to be contained in [`Self::config()`].
    pub fn param(&self) -> Option<&'static ParamMetadata> {
        if let LocationInConfig::Param(idx) = self.location_in_config? {
            self.config?.params.get(idx)
        } else {
            None
        }
    }

    /// Returns metadata for the failing nested config if this error concerns a nested config. The config
    /// is guaranteed to be nested in [`Self::config()`].
    pub fn nested_config(&self) -> Option<&'static NestedConfigMetadata> {
        if let LocationInConfig::Nested(idx) = self.location_in_config? {
            self.config?.nested_configs.get(idx)
        } else {
            None
        }
    }

    pub(crate) fn with_origin(mut self, origin: Option<&Arc<ValueOrigin>>) -> Self {
        if self.origin.is_none() {
            self.origin = origin.cloned();
        }
        self
    }

    pub(crate) fn with_path(mut self, path: &str) -> Self {
        if self.path.is_none() {
            self.path = Some(path.to_owned());
        }
        self
    }

    pub(crate) fn for_config(mut self, metadata: Option<&'static ConfigMetadata>) -> Self {
        self.config = self.config.or(metadata);
        self
    }

    pub(crate) fn with_location(
        mut self,
        metadata: Option<&'static ConfigMetadata>,
        location: LocationInConfig,
    ) -> Self {
        if metadata.is_some() {
            self.config = metadata;
            self.location_in_config = Some(location);
        }
        self
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
