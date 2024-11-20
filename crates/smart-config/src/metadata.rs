//! Configuration metadata.

use std::{any, fmt};

#[doc(hidden)] // used in the derive macro
pub use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
pub use smart_config_derive::DescribeConfig;

/// Describes a configuration (i.e., a group of related parameters).
pub trait DescribeConfig: 'static + DeserializeOwned {
    /// Provides the description.
    fn describe_config() -> &'static ConfigMetadata;
}

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
    pub ty: RustType,
    pub base_type: RustType,
    pub base_type_kind: TypeKind,
    pub unit: Option<UnitOfMeasurement>,
    #[doc(hidden)] // set by derive macro
    pub default_value: Option<fn() -> Box<dyn fmt::Debug>>,
}

impl ParamMetadata {
    pub fn default_value(&self) -> Option<impl fmt::Debug + '_> {
        self.default_value.map(|value_fn| value_fn())
    }
}

#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum UnitOfMeasurement {
    Seconds,
    Milliseconds,
    Bytes,
    Megabytes,
}

impl fmt::Display for UnitOfMeasurement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Seconds => "seconds",
            Self::Milliseconds => "milliseconds",
            Self::Bytes => "bytes",
            Self::Megabytes => "megabytes (IEC)",
        })
    }
}

impl UnitOfMeasurement {
    pub fn detect(param_name: &str, base_type_kind: TypeKind) -> Option<Self> {
        if base_type_kind != TypeKind::Integer {
            return None;
        }

        if param_name.ends_with("_ms") {
            Some(Self::Milliseconds)
        } else if param_name.ends_with("_sec") {
            Some(Self::Seconds)
        } else if param_name.ends_with("_bytes") {
            Some(Self::Bytes)
        } else if param_name.ends_with("_mb") {
            Some(Self::Megabytes)
        } else {
            None
        }
    }
}

/// Representation of a Rust type.
#[derive(Debug, Clone, Copy)]
pub struct RustType {
    id: any::TypeId,
    name_in_code: &'static str,
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

/// Human-readable kind for a Rust type used in configuration parameter (Boolean value, integer, string etc.).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum TypeKind {
    Bool,
    Integer,
    Float,
    String,
    Path,
    // TODO: support URLs
}

impl fmt::Display for TypeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool => formatter.write_str("Boolean"),
            Self::Integer => formatter.write_str("integer"),
            Self::Float => formatter.write_str("floating-point value"),
            Self::String => formatter.write_str("string"),
            Self::Path => formatter.write_str("filesystem path"),
        }
    }
}

/// Mention of a nested configuration within a configuration.
#[derive(Debug, Clone, Copy)]
pub struct NestedConfigMetadata {
    pub name: &'static str,
    pub meta: &'static ConfigMetadata,
}
