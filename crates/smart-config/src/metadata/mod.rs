//! Configuration metadata.

use std::{any, borrow::Cow, fmt};

use self::_private::{BoxedDeserializer, BoxedVisitor};
use crate::{
    de::{DeserializeParam, _private::ErasedDeserializer},
    fallback::FallbackSource,
    ErrorWithOrigin,
};

#[doc(hidden)] // used in the derive macros
pub mod _private;
#[cfg(test)]
mod tests;

/// Generic post-validation for a configuration parameter.
#[doc(hidden)]
pub trait Validate<T: ?Sized>: 'static + Send + Sync + fmt::Debug + fmt::Display {
    fn validate(&self, target: &T) -> Result<(), ErrorWithOrigin>;
}

/// Metadata for a configuration (i.e., a group of related parameters).
#[derive(Debug, Clone)]
pub struct ConfigMetadata {
    /// Type of this configuration.
    pub ty: RustType,
    /// Help regarding the config itself.
    pub help: &'static str,
    /// Parameters included in the config.
    pub params: &'static [ParamMetadata],
    /// Tag for enumeration configs.
    pub tag: Option<ConfigTag>,
    /// Nested configs included in the config.
    pub nested_configs: &'static [NestedConfigMetadata],
    #[doc(hidden)] // implementation detail
    pub deserializer: BoxedDeserializer,
    #[doc(hidden)] // implementation detail
    pub visitor: BoxedVisitor,
    #[doc(hidden)] // implementation detail
    pub validations: &'static [&'static dyn Validate<dyn any::Any>],
}

/// Information about a config tag.
#[derive(Debug, Clone, Copy)]
pub struct ConfigTag {
    /// Parameter of the enclosing config corresponding to the tag.
    pub param: &'static ParamMetadata,
    /// Variants for the tag.
    pub variants: &'static [ConfigVariant],
    /// Default variant, if any.
    pub default_variant: Option<&'static ConfigVariant>,
}

/// Variant of a [`ConfigTag`].
#[derive(Debug, Clone, Copy)]
pub struct ConfigVariant {
    /// Canonical param name in the config sources. Not necessarily the Rust name!
    pub name: &'static str,
    /// Param aliases.
    pub aliases: &'static [&'static str],
    /// Name of the corresponding enum variant in Rust code.
    pub rust_name: &'static str,
    /// Human-readable param help parsed from the doc comment.
    pub help: &'static str,
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
    /// Name of the param field in Rust code.
    pub rust_field_name: &'static str,
    /// Rust type of the parameter.
    pub rust_type: RustType,
    /// Basic type(s) expected by the param deserializer.
    pub expecting: BasicTypes,
    /// Tag variant in the enclosing [`ConfigMetadata`] that enables this parameter. `None` means that the parameter is unconditionally enabled.
    pub tag_variant: Option<&'static ConfigVariant>,
    #[doc(hidden)] // implementation detail
    pub deserializer: &'static dyn ErasedDeserializer,
    #[doc(hidden)] // implementation detail
    pub default_value: Option<fn() -> Box<dyn fmt::Debug>>,
    #[doc(hidden)]
    pub fallback: Option<&'static dyn FallbackSource>,
}

impl ParamMetadata {
    /// Returns the default value for the param.
    pub fn default_value(&self) -> Option<impl fmt::Debug + '_> {
        self.default_value.map(|value_fn| value_fn())
    }

    /// Returns the type description for this param as provided by its deserializer.
    // TODO: can be cached if necessary
    pub fn type_description(&self) -> TypeDescription {
        let mut description = TypeDescription::default();
        self.deserializer.describe(&mut description);
        description.rust_type = self.rust_type.name_in_code;
        description
    }
}

/// Representation of a Rust type.
#[derive(Clone, Copy)]
pub struct RustType {
    id: fn() -> any::TypeId,
    name_in_code: &'static str,
}

impl fmt::Debug for RustType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name_in_code)
    }
}

impl PartialEq for RustType {
    fn eq(&self, other: &Self) -> bool {
        (self.id)() == (other.id)()
    }
}

impl RustType {
    /// Creates a new type.
    pub const fn of<T: 'static>(name_in_code: &'static str) -> Self {
        Self {
            id: any::TypeId::of::<T>,
            name_in_code,
        }
    }

    /// Returns the unique ID of this type.
    pub fn id(&self) -> any::TypeId {
        (self.id)()
    }

    /// Returns the name of this type as specified in code.
    pub const fn name_in_code(&self) -> &'static str {
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

#[derive(Debug, Clone)]
struct ChildDescription {
    expecting: BasicTypes,
    description: Box<TypeDescription>,
}

impl ChildDescription {
    fn new<T: 'static, De: DeserializeParam<T>>(deserializer: &De) -> Self {
        let mut description = Box::default();
        deserializer.describe(&mut description);
        description.rust_type = any::type_name::<T>();
        Self {
            expecting: De::EXPECTING,
            description,
        }
    }
}

/// Human-readable description for a Rust type used in configuration parameter (Boolean value, integer, string etc.).
///
/// If a configuration parameter supports complex inputs (objects and/or arrays), this information *may* contain
/// info on child types (array items; map keys / values).
#[derive(Debug, Clone, Default)]
pub struct TypeDescription {
    rust_type: &'static str,
    details: Option<Cow<'static, str>>,
    unit: Option<UnitOfMeasurement>,
    pub(crate) is_secret: bool,
    items: Option<ChildDescription>,
    entries: Option<(ChildDescription, ChildDescription)>,
}

impl TypeDescription {
    #[doc(hidden)]
    pub fn rust_type(&self) -> &str {
        self.rust_type
    }

    /// Gets the type details.
    pub fn details(&self) -> Option<&str> {
        self.details.as_deref()
    }

    /// Gets the unit of measurement.
    pub fn unit(&self) -> Option<UnitOfMeasurement> {
        self.unit
    }

    /// Returns the description of array items, if one was provided.
    pub fn items(&self) -> Option<(BasicTypes, &Self)> {
        self.items
            .as_ref()
            .map(|child| (child.expecting, &*child.description))
    }

    /// Returns the description of map keys, if one was provided.
    pub fn keys(&self) -> Option<(BasicTypes, &Self)> {
        let keys = &self.entries.as_ref()?.0;
        Some((keys.expecting, &*keys.description))
    }

    /// Returns the description of map values, if one was provided.
    pub fn values(&self) -> Option<(BasicTypes, &Self)> {
        let keys = &self.entries.as_ref()?.1;
        Some((keys.expecting, &*keys.description))
    }

    /// Checks whether this type or any child types (e.g., array items or map keys / values) are marked
    /// as secret.
    pub fn contains_secrets(&self) -> bool {
        if self.is_secret {
            return true;
        }
        if let Some(item) = &self.items {
            if item.description.contains_secrets() {
                return true;
            }
        }
        if let Some((key, value)) = &self.entries {
            if key.description.contains_secrets() {
                return true;
            }
            if value.description.contains_secrets() {
                return true;
            }
        }
        false
    }

    /// Sets human-readable type details.
    pub fn set_details(&mut self, details: impl Into<Cow<'static, str>>) -> &mut Self {
        self.details = Some(details.into());
        self
    }

    /// Adds a unit of measurement.
    pub fn set_unit(&mut self, unit: UnitOfMeasurement) -> &mut Self {
        self.unit = Some(unit);
        self
    }

    /// Marks the value as secret.
    pub fn set_secret(&mut self) -> &mut Self {
        self.is_secret = true;
        self
    }

    /// Adds a description of array items. This only makes sense for params accepting array input.
    pub fn set_items<T: 'static>(&mut self, items: &impl DeserializeParam<T>) -> &mut Self {
        self.items = Some(ChildDescription::new(items));
        self
    }

    /// Adds a description of keys and values. This only makes sense for params accepting object input.
    pub fn set_entries<K: 'static, V: 'static>(
        &mut self,
        keys: &impl DeserializeParam<K>,
        values: &impl DeserializeParam<V>,
    ) -> &mut Self {
        self.entries = Some((ChildDescription::new(keys), ChildDescription::new(values)));
        self
    }
}

impl fmt::Display for TypeDescription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(description) = &self.details {
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
    /// Aliases for the config. Cannot be present for flattened configs.
    pub aliases: &'static [&'static str],
    /// Name of the config field in Rust code.
    pub rust_field_name: &'static str,
    /// Tag variant in the enclosing [`ConfigMetadata`] that enables this parameter. `None` means that the parameter is unconditionally enabled.
    pub tag_variant: Option<&'static ConfigVariant>,
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
    /// Week (7 days).
    Weeks,
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
            TimeUnit::Weeks => "weeks",
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
