//! Configuration metadata.

use std::{any, borrow::Cow, fmt};

use crate::de::ErasedDeserializer;

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
    #[allow(dead_code)] // FIXME: ???
    pub(crate) fn help_header(&self) -> Option<&'static str> {
        let first_line = self.help.lines().next()?;
        first_line.strip_prefix("# ")
    }
}

/// Metadata for a specific configuration parameter.
#[derive(Debug, Clone, Copy)]
pub struct ParamMetadata {
    /// Canonical param name in the config sources. Not necessarily the Rust field name!
    pub name: &'static str,
    /// Param aliases.
    pub aliases: &'static [&'static str],
    /// Human-readable param help parsed from the doc comment.
    pub help: &'static str,
    /// Rust type of the parameter.
    pub ty: RustType,
    /// Basic type(s) expected by the param deserializer.
    pub expecting: BasicTypes,
    #[doc(hidden)] // implementation detail
    pub deserializer: &'static dyn ErasedDeserializer,
    #[doc(hidden)] // implementation detail
    pub default_value: Option<fn() -> Box<dyn fmt::Debug>>,
}

impl ParamMetadata {
    /// Returns the default value for the param.
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
    /// Creates a new type.
    pub fn of<T: 'static>(name_in_code: &'static str) -> Self {
        Self {
            id: any::TypeId::of::<T>(),
            name_in_code,
        }
    }

    pub(crate) fn id(&self) -> any::TypeId {
        self.id
    }

    /// Returns the name of this type as specified in code.
    pub fn name_in_code(&self) -> &'static str {
        self.name_in_code
    }
}

/// Set of one or more basic types in the JSON object model.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BasicTypes(u8);

impl BasicTypes {
    /// Boolean value.
    pub const BOOL: Self = Self(1);
    /// Integer value.
    pub const INTEGER: Self = Self(2);
    /// Floating-point value.
    pub const FLOAT: Self = Self(4 | 2);
    /// String.
    pub const STRING: Self = Self(8);
    /// Array of values.
    pub const ARRAY: Self = Self(16);
    /// Object / map of values.
    pub const OBJECT: Self = Self(32);
    /// Any value.
    pub const ANY: Self = Self(63);

    const COMPONENTS: &'static [(Self, &'static str)] = &[
        (Self::BOOL, "Boolean"),
        (Self::INTEGER, "integer"),
        (Self::FLOAT, "float"),
        (Self::STRING, "string"),
        (Self::ARRAY, "array"),
        (Self::OBJECT, "object"),
    ];

    pub(crate) const fn from_raw(raw: u8) -> Self {
        assert!(raw != 0, "Raw `BasicTypes` cannot be 0");
        assert!(
            raw <= Self::ANY.0,
            "Unused set bits in `BasicTypes` raw value"
        );
        Self(raw)
    }

    #[doc(hidden)] // should only be used via macros
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Returns a union of two sets of basic types.
    #[must_use]
    pub const fn or(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }

    /// Checks whether the `needle` is fully contained in this set.
    pub const fn contains(self, needle: Self) -> bool {
        self.0 & needle.0 == needle.0
    }
}

impl fmt::Display for BasicTypes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::ANY {
            formatter.write_str("any")
        } else {
            let mut is_empty = true;
            for &(component, name) in Self::COMPONENTS {
                if self.contains(component) {
                    if !is_empty {
                        formatter.write_str(" | ")?;
                    }
                    formatter.write_str(name)?;
                    is_empty = false;
                }
            }
            Ok(())
        }
    }
}

impl fmt::Debug for BasicTypes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

/// Human-readable description for a Rust type used in configuration parameter (Boolean value, integer, string etc.).
#[derive(Debug, Clone, Default)]
pub struct TypeQualifiers {
    description: Option<Cow<'static, str>>,
    unit: Option<UnitOfMeasurement>,
}

impl TypeQualifiers {
    /// Adds a qualifying description.
    #[must_use]
    pub fn with_description(mut self, description: &'static str) -> Self {
        self.description = Some(Cow::Borrowed(description));
        self
    }

    #[must_use]
    pub(crate) fn with_dyn_description(mut self, description: String) -> Self {
        self.description = Some(Cow::Owned(description));
        self
    }

    /// Adds a unit of measurement.
    #[must_use]
    pub fn with_unit(mut self, unit: UnitOfMeasurement) -> Self {
        self.unit = Some(unit);
        self
    }
}

impl fmt::Display for TypeQualifiers {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(description) = &self.description {
            write!(formatter, ", {description}")?;
        }
        if let Some(unit) = self.unit {
            write!(formatter, " [unit: {unit}]")?;
        }
        Ok(())
    }
}

/// Mention of a nested configuration within a configuration.
#[derive(Debug, Clone, Copy)]
pub struct NestedConfigMetadata {
    /// Name of the config in config sources. Empty for flattened configs. Not necessarily the Rust field name!
    pub name: &'static str,
    /// Config metadata.
    pub meta: &'static ConfigMetadata,
}

/// Unit of time measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TimeUnit {
    /// Millisecond (0.001 seconds).
    Millis,
    /// Base unit – second.
    Seconds,
    /// Minute (60 seconds).
    Minutes,
    /// Hour (3,600 seconds).
    Hours,
    /// Day (86,400 seconds).
    Days,
    // No larger units since they are less useful and may be ambiguous (e.g., is a month 30 days? is a year 365 days or 365.25...)
}

impl TimeUnit {
    pub(crate) fn plural(self) -> &'static str {
        match self {
            TimeUnit::Millis => "milliseconds",
            TimeUnit::Seconds => "seconds",
            TimeUnit::Minutes => "minutes",
            TimeUnit::Hours => "hours",
            TimeUnit::Days => "days",
        }
    }
}

impl fmt::Display for TimeUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.plural())
    }
}

/// Unit of byte size measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SizeUnit {
    /// Base unit – bytes.
    Bytes,
    /// Binary kilobyte (aka kibibyte) = 1,024 bytes.
    KiB,
    /// Binary megabyte (aka mibibyte) = 1,048,576 bytes.
    MiB,
    /// Binary gigabyte (aka gibibyte) = 1,073,741,824 bytes.
    GiB,
}

impl SizeUnit {
    pub(crate) const fn plural(self) -> &'static str {
        match self {
            Self::Bytes => "bytes",
            Self::KiB => "kilobytes",
            Self::MiB => "megabytes",
            Self::GiB => "gigabytes",
        }
    }

    pub(crate) const fn bytes_in_unit(self) -> u64 {
        match self {
            Self::Bytes => 1,
            Self::KiB => 1_024,
            Self::MiB => 1_024 * 1_024,
            Self::GiB => 1_024 * 1_024 * 1_024,
        }
    }
}

impl fmt::Display for SizeUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.plural())
    }
}

/// General unit of measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UnitOfMeasurement {
    /// Unit of time measurement.
    Time(TimeUnit),
    /// Unit of byte size measurement.
    ByteSize(SizeUnit),
}

impl fmt::Display for UnitOfMeasurement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Time(unit) => fmt::Display::fmt(unit, formatter),
            Self::ByteSize(unit) => fmt::Display::fmt(unit, formatter),
        }
    }
}

impl From<TimeUnit> for UnitOfMeasurement {
    fn from(unit: TimeUnit) -> Self {
        Self::Time(unit)
    }
}

impl From<SizeUnit> for UnitOfMeasurement {
    fn from(unit: SizeUnit) -> Self {
        Self::ByteSize(unit)
    }
}
