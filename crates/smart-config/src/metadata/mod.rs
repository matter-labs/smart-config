//! Configuration metadata.

use std::{any, borrow::Cow, fmt, ops, time::Duration};

use self::_private::{BoxedDeserializer, BoxedVisitor};
use crate::{
    de::{_private::ErasedDeserializer, DeserializeParam},
    fallback::FallbackSource,
    validation::Validate,
};

#[doc(hidden)] // used in the derive macros
pub mod _private;
#[cfg(test)]
mod tests;

/// Options for a param or config alias.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq))]
#[non_exhaustive]
pub struct AliasOptions {
    /// Is this alias deprecated?
    pub is_deprecated: bool,
}

impl Default for AliasOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl AliasOptions {
    /// Creates default options.
    pub const fn new() -> Self {
        AliasOptions {
            is_deprecated: false,
        }
    }

    /// Marks the alias as deprecated.
    #[must_use]
    pub const fn deprecated(mut self) -> Self {
        self.is_deprecated = true;
        self
    }

    #[doc(hidden)] // not stable yet
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        Self {
            is_deprecated: self.is_deprecated || other.is_deprecated,
        }
    }
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
    pub aliases: &'static [(&'static str, AliasOptions)],
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
    pub default_value: Option<fn() -> Box<dyn any::Any>>,
    #[doc(hidden)] // implementation detail
    pub example_value: Option<fn() -> Box<dyn any::Any>>,
    #[doc(hidden)]
    pub fallback: Option<&'static dyn FallbackSource>,
}

impl ParamMetadata {
    /// Returns the default value for the param.
    pub fn default_value(&self) -> Option<Box<dyn any::Any>> {
        self.default_value.map(|value_fn| value_fn())
    }

    /// Returns the default value for the param serialized into JSON.
    pub fn default_value_json(&self) -> Option<serde_json::Value> {
        self.default_value()
            .map(|val| self.deserializer.serialize_param(val.as_ref()))
    }

    /// Returns the example value for the param serialized into JSON.
    pub fn example_value_json(&self) -> Option<serde_json::Value> {
        let example = self.example_value?();
        Some(self.deserializer.serialize_param(example.as_ref()))
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
    #[allow(clippy::incompatible_msrv)] // false positive; `TypeId::of` is referenced, not invoked
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
    fn new<T: 'static, De: DeserializeParam<T>>(deserializer: &De, set_type: bool) -> Self {
        let mut description = Box::default();
        deserializer.describe(&mut description);
        if set_type {
            description.rust_type = any::type_name::<T>();
        }
        Self {
            expecting: De::EXPECTING,
            description,
        }
    }
}

/// Recognized suffixes for a param type used during object nesting when preprocessing config sources.
/// Only these suffixes will be recognized as belonging to the param and activate its object nesting.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
#[doc(hidden)] // not stable yet
pub enum TypeSuffixes {
    /// All possible suffixes.
    All,
    /// Duration units like `_sec` or `_millis`. May be prepended with `_in`, e.g. `_in_secs`.
    DurationUnits,
    /// Byte size units like `_mb` or `_bytes`. May be prepended with `_in`, e.g. `_in_mb`.
    SizeUnits,
    /// Ether units like `_wei` or `_ether`. May be prepended with `_in`, e.g. `_in_wei`.
    EtherUnits,
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
    suffixes: Option<TypeSuffixes>,
    pub(crate) is_secret: bool,
    validations: Vec<String>,
    deserialize_if: Option<String>,
    items: Option<ChildDescription>,
    entries: Option<(ChildDescription, ChildDescription)>,
    fallback: Option<ChildDescription>,
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

    #[doc(hidden)] // not stable yet
    pub fn suffixes(&self) -> Option<TypeSuffixes> {
        self.suffixes
    }

    #[doc(hidden)] // exposes implementation details
    pub fn validations(&self) -> &[String] {
        &self.validations
    }

    #[doc(hidden)] // exposes implementation details
    pub fn deserialize_if(&self) -> Option<&str> {
        self.deserialize_if.as_deref()
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

    /// Returns the fallback description, if any.
    pub fn fallback(&self) -> Option<(BasicTypes, &Self)> {
        let fallback = self.fallback.as_ref()?;
        Some((fallback.expecting, &*fallback.description))
    }

    /// Checks whether this type or any child types (e.g., array items or map keys / values) are marked
    /// as secret.
    pub fn contains_secrets(&self) -> bool {
        if self.is_secret {
            return true;
        }
        if let Some(item) = &self.items
            && item.description.contains_secrets()
        {
            return true;
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

    pub(crate) fn set_suffixes(&mut self, suffixes: TypeSuffixes) -> &mut Self {
        self.suffixes = Some(suffixes);
        self
    }

    /// Sets validation for the type.
    pub fn set_validations<T>(&mut self, validations: &[&'static dyn Validate<T>]) -> &mut Self {
        self.validations = validations.iter().map(ToString::to_string).collect();
        self
    }

    /// Sets a "deserialize if" condition for the type.
    pub fn set_deserialize_if<T>(&mut self, condition: &'static dyn Validate<T>) -> &mut Self {
        self.deserialize_if = Some(condition.to_string());
        self
    }

    /// Marks the value as secret.
    pub fn set_secret(&mut self) -> &mut Self {
        self.is_secret = true;
        self
    }

    /// Adds a description of array items. This only makes sense for params accepting array input.
    pub fn set_items<T: 'static>(&mut self, items: &impl DeserializeParam<T>) -> &mut Self {
        self.items = Some(ChildDescription::new(items, true));
        self
    }

    /// Adds a description of keys and values. This only makes sense for params accepting object input.
    pub fn set_entries<K: 'static, V: 'static>(
        &mut self,
        keys: &impl DeserializeParam<K>,
        values: &impl DeserializeParam<V>,
    ) -> &mut Self {
        self.entries = Some((
            ChildDescription::new(keys, true),
            ChildDescription::new(values, true),
        ));
        self
    }

    /// Adds a fallback deserializer description.
    pub fn set_fallback<T: 'static>(&mut self, fallback: &impl DeserializeParam<T>) {
        self.fallback = Some(ChildDescription::new(fallback, false));
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
    pub aliases: &'static [(&'static str, AliasOptions)],
    /// Name of the config field in Rust code.
    pub rust_field_name: &'static str,
    /// Tag variant in the enclosing [`ConfigMetadata`] that enables this parameter. `None` means that the parameter is unconditionally enabled.
    pub tag_variant: Option<&'static ConfigVariant>,
    /// Config metadata.
    pub meta: &'static ConfigMetadata,
}

/// Unit of time measurement.
///
/// # Examples
///
/// You can use multiplication to define durations (e.g., for parameter values):
///
/// ```
/// # use std::time::Duration;
/// # use smart_config::metadata::TimeUnit;
/// let dur = 5 * TimeUnit::Hours;
/// assert_eq!(dur, Duration::from_secs(5 * 3_600));
/// ```
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

    /// Multiplies this time unit by the specified factor.
    pub fn checked_mul(self, factor: u64) -> Option<Duration> {
        Some(match self {
            Self::Millis => Duration::from_millis(factor),
            Self::Seconds => Duration::from_secs(factor),
            Self::Minutes => {
                let val = factor.checked_mul(60)?;
                Duration::from_secs(val)
            }
            Self::Hours => {
                let val = factor.checked_mul(3_600)?;
                Duration::from_secs(val)
            }
            Self::Days => {
                let val = factor.checked_mul(86_400)?;
                Duration::from_secs(val)
            }
            Self::Weeks => {
                let val = factor.checked_mul(86_400 * 7)?;
                Duration::from_secs(val)
            }
        })
    }
}

impl fmt::Display for TimeUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.plural())
    }
}

impl From<TimeUnit> for Duration {
    fn from(unit: TimeUnit) -> Self {
        match unit {
            TimeUnit::Millis => Duration::from_millis(1),
            TimeUnit::Seconds => Duration::from_secs(1),
            TimeUnit::Minutes => Duration::from_secs(60),
            TimeUnit::Hours => Duration::from_secs(3_600),
            TimeUnit::Days => Duration::from_secs(86_400),
            TimeUnit::Weeks => Duration::from_secs(86_400 * 7),
        }
    }
}

/// Panics on overflow.
impl ops::Mul<u64> for TimeUnit {
    type Output = Duration;

    fn mul(self, rhs: u64) -> Self::Output {
        self.checked_mul(rhs)
            .unwrap_or_else(|| panic!("Integer overflow getting {rhs} * {self}"))
    }
}

/// Panics on overflow.
impl ops::Mul<TimeUnit> for u64 {
    type Output = Duration;

    fn mul(self, rhs: TimeUnit) -> Self::Output {
        rhs.checked_mul(self)
            .unwrap_or_else(|| panic!("Integer overflow getting {self} * {rhs}"))
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
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Bytes => "bytes",
            Self::KiB => "kilobytes",
            Self::MiB => "megabytes",
            Self::GiB => "gigabytes",
        }
    }

    pub(crate) const fn value_in_unit(self) -> u64 {
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
        formatter.write_str(self.as_str())
    }
}

/// Unit of ether amount measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EtherUnit {
    /// Smallest unit of measurement.
    Wei,
    /// `10^9` wei.
    Gwei,
    /// `10^18` wei.
    Ether,
}

impl fmt::Display for EtherUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl EtherUnit {
    pub(crate) const fn value_in_unit(self) -> u128 {
        match self {
            Self::Wei => 1,
            Self::Gwei => 1_000_000_000,
            Self::Ether => 1_000_000_000_000_000_000,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Wei => "wei",
            Self::Gwei => "gwei",
            Self::Ether => "ether",
        }
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
    /// Unit of ether amount measurement.
    Ether(EtherUnit),
}

impl fmt::Display for UnitOfMeasurement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Time(unit) => fmt::Display::fmt(unit, formatter),
            Self::ByteSize(unit) => fmt::Display::fmt(unit, formatter),
            Self::Ether(unit) => fmt::Display::fmt(unit, formatter),
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

impl From<EtherUnit> for UnitOfMeasurement {
    fn from(unit: EtherUnit) -> Self {
        Self::Ether(unit)
    }
}
