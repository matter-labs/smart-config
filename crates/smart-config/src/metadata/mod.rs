//! Configuration metadata.

use std::{any, fmt};

use crate::de::ObjectSafeDeserializer;

#[cfg(test)]
mod tests;
#[doc(hidden)] // used in the derive macro
pub mod validation;

/// Metadata for a configuration (i.e., a group of related parameters).
#[derive(Debug, Clone)]
pub struct ConfigMetadata {
    /// Type of this configuration.
    pub ty: RustType,
    /// Help regarding the config itself.
    pub help: &'static str,
    /// Parameters included in the config.
    pub params: Box<[ParamMetadata]>,
    /// Nested configs included in the config.
    pub nested_configs: Box<[NestedConfigMetadata]>,
}

impl ConfigMetadata {
    pub(crate) fn help_header(&self) -> Option<&'static str> {
        let first_line = self.help.lines().next()?;
        first_line.strip_prefix("# ")
    }
}

/// Metadata for a specific configuration parameter.
#[derive(Debug, Clone, Copy)]
pub struct ParamMetadata {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub help: &'static str,
    /// Type with a potential `Option<_>` wrapper stripped.
    pub ty: RustType,
    pub deserializer: &'static dyn ObjectSafeDeserializer,
    #[doc(hidden)] // set by derive macro
    pub default_value: Option<fn() -> Box<dyn fmt::Debug>>,
}

impl ParamMetadata {
    pub fn default_value(&self) -> Option<impl fmt::Debug + '_> {
        self.default_value.map(|value_fn| value_fn())
    }
}

/// Representation of a Rust type.
#[derive(Clone, Copy)]
pub struct RustType {
    id: any::TypeId,
    name_in_code: &'static str,
}

impl fmt::Debug for RustType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name_in_code)
    }
}

impl PartialEq for RustType {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl RustType {
    pub fn of<T: 'static>(name_in_code: &'static str) -> Self {
        Self {
            id: any::TypeId::of::<T>(),
            name_in_code,
        }
    }

    pub(crate) fn id(&self) -> any::TypeId {
        self.id
    }

    pub fn name_in_code(&self) -> &'static str {
        self.name_in_code
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum BasicType {
    Bool,
    Integer,
    Float,
    String,
    Array,
    Object,
}

impl fmt::Display for BasicType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool => formatter.write_str("Boolean"),
            Self::Integer => formatter.write_str("integer"),
            Self::Float => formatter.write_str("floating-point value"),
            Self::String => formatter.write_str("string"),
            Self::Array => formatter.write_str("array"),
            Self::Object => formatter.write_str("object"),
        }
    }
}

/// Human-readable kind for a Rust type used in configuration parameter (Boolean value, integer, string etc.).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SchemaType {
    pub(crate) base: Option<BasicType>,
}

impl SchemaType {
    pub const ANY: Self = Self { base: None };

    pub const fn new(base: BasicType) -> Self {
        Self { base: Some(base) }
    }
}

impl fmt::Display for SchemaType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(base) = &self.base {
            fmt::Display::fmt(base, formatter)
        } else {
            formatter.write_str("any")
        }
    }
}

/// Mention of a nested configuration within a configuration.
#[derive(Debug, Clone, Copy)]
pub struct NestedConfigMetadata {
    pub name: &'static str,
    pub meta: &'static ConfigMetadata,
}
