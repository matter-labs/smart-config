//! Parameter deserializers.

use std::{
    any, fmt,
    num::{
        NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroIsize, NonZeroU16, NonZeroU32,
        NonZeroU64, NonZeroU8, NonZeroUsize,
    },
    path::PathBuf,
    str::FromStr,
    time::Duration,
};

use serde::{
    de::{DeserializeOwned, Error as DeError},
    Deserialize,
};

use crate::{
    de::{deserializer::ValueDeserializer, DeserializeContext},
    error::ErrorWithOrigin,
    metadata::{BasicTypes, ParamMetadata, SizeUnit, TimeUnit, TypeQualifiers},
    value::{Value, WithOrigin},
    ByteSize,
};

/// Deserializes a parameter of the specified type.
///
/// # Implementations
///
/// `DeserializeParam` includes the following implementations:
///
/// - `()` is the default deserializer used unless explicitly overwritten with `#[config(with = _)]`.
///   It supports types known to deserialize well (see [`WellKnown`]), and can be switched for user-defined types
///   by implementing `WellKnown` for the type.
/// - [`Serde`] allows deserializing any type implementing [`serde::Deserialize`].
/// - [`Optional`] decorates a deserializer for `T` turning it into a deserializer for `Option<T>`
/// - [`TimeUnit`] allows deserializing [`Duration`] from a number
/// - [`SizeUnit`] similarly allows deserializing [`ByteSize`]
/// - [`Delimited`] allows deserializing arrays from delimited string (e.g., comma-delimited)
/// - [`OrString`] allows to switch between structured and string deserialization
pub trait DeserializeParam<T>: fmt::Debug + Send + Sync + 'static {
    /// Describes which parameter this deserializer is expecting.
    const EXPECTING: BasicTypes;

    /// Additional info about the deserialized type, e.g., extended description.
    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::default()
    }

    /// Performs deserialization given the context and param metadata.
    ///
    /// # Errors
    ///
    /// Returns an error if a param cannot be deserialized, e.g. if it has an incorrect type.
    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin>;
}

/// Parameter type with well-known [deserializer](DeserializeParam).
///
/// Conceptually, this means that the type is known to behave well when deserializing data from a [`Value`]
/// (ordinarily, using [`serde::Deserialize`]).
///
/// # Implementations
///
/// Basic well-known types include:
///
/// - `bool`
/// - [`String`]
/// - [`PathBuf`]
/// - Signed and unsigned integers, including non-zero variants
/// - `f32`, `f64`
///
/// It is also implemented for collections, items of which implement `WellKnown`:
///
/// - [`Option`]
/// - [`Vec`], arrays
/// - [`HashSet`], [`BTreeSet`]
/// - [`HashMap`], [`BTreeMap`]
#[diagnostic::on_unimplemented(
    message = "`{Self}` param cannot be deserialized",
    note = "Add #[config(with = _)] attribute to specify deserializer to use",
    note = "If `{Self}` is a config, add #[config(nest)] or #[config(flatten)]"
)]
pub trait WellKnown: Sized {
    /// Type of the deserializer used for this type.
    type Deserializer: DeserializeParam<Self>;
    /// Deserializer instance.
    const DE: Self::Deserializer;
}

impl<T: WellKnown> DeserializeParam<T> for () {
    const EXPECTING: BasicTypes = <T::Deserializer as DeserializeParam<T>>::EXPECTING;

    fn type_qualifiers(&self) -> TypeQualifiers {
        T::DE.type_qualifiers()
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        T::DE.deserialize_param(ctx, param)
    }
}

/// Deserializer powered by `serde`. Usually created with the help of [`Serde!`](crate::Serde!) macro;
/// see its docs for the examples of usage.
pub struct Serde<const EXPECTING: u8>;

impl<const EXPECTING: u8> fmt::Debug for Serde<EXPECTING> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Serde")
            .field(&BasicTypes::from_raw(EXPECTING))
            .finish()
    }
}

impl<T: DeserializeOwned, const EXPECTING: u8> DeserializeParam<T> for Serde<EXPECTING> {
    const EXPECTING: BasicTypes = BasicTypes::from_raw(EXPECTING);

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        let expecting = BasicTypes::from_raw(EXPECTING);
        let Some(current_value) = ctx.current_value() else {
            return Err(DeError::missing_field(param.name));
        };

        let deserializer = ValueDeserializer::new(current_value, ctx.de_options);
        let type_matches = deserializer.value().is_supported_by(expecting);
        if !type_matches {
            return Err(deserializer.invalid_type(&expecting.to_string()));
        }
        T::deserialize(deserializer)
    }
}

impl WellKnown for bool {
    type Deserializer = super::Serde![bool];
    const DE: Self::Deserializer = super::Serde![bool];
}

impl WellKnown for String {
    type Deserializer = super::Serde![str];
    const DE: Self::Deserializer = super::Serde![str];
}

impl WellKnown for PathBuf {
    type Deserializer = Qualified<super::Serde![str]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![str], "filesystem path");
}

impl WellKnown for f32 {
    type Deserializer = super::Serde![float];
    const DE: Self::Deserializer = super::Serde![float];
}

impl WellKnown for f64 {
    type Deserializer = super::Serde![float];
    const DE: Self::Deserializer = super::Serde![float];
}

macro_rules! impl_well_known_int {
    ($($int:ty),+) => {
        $(
        impl WellKnown for $int {
            type Deserializer = super::Serde![int];
            const DE: Self::Deserializer = super::Serde![int];
        }
        )+
    };
}

impl_well_known_int!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, usize, isize);
impl_well_known_int!(
    NonZeroU8,
    NonZeroI8,
    NonZeroU16,
    NonZeroI16,
    NonZeroU32,
    NonZeroI32,
    NonZeroU64,
    NonZeroI64,
    NonZeroUsize,
    NonZeroIsize
);

impl<T: WellKnown> WellKnown for Option<T> {
    type Deserializer = Optional<T::Deserializer>;
    const DE: Self::Deserializer = Optional(T::DE);
}

/// [Deserializer](DeserializeParam) decorator that provides additional [qualifiers](TypeQualifiers)
/// for the deserialized type.
#[derive(Debug)]
pub struct Qualified<De> {
    inner: De,
    // Cannot use `TypeQualifiers` directly because it wouldn't allow to drop the type in const contexts.
    description: &'static str,
}

impl<De> Qualified<De> {
    /// Creates a new instance with the extended type description.
    pub const fn new(inner: De, description: &'static str) -> Self {
        Self { inner, description }
    }
}

impl<T, De> DeserializeParam<T> for Qualified<De>
where
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = <De as DeserializeParam<T>>::EXPECTING;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new(self.description)
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        self.inner.deserialize_param(ctx, param)
    }
}

/// Deserializer decorator that defaults to the provided value if the input for the param is missing.
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
            .finish_non_exhaustive()
    }
}

impl<T: 'static, De: DeserializeParam<T>> WithDefault<T, De> {
    /// Creates a new instance.
    pub const fn new(inner: De, default: fn() -> T) -> Self {
        Self { inner, default }
    }
}

impl<T: 'static, De: DeserializeParam<T>> DeserializeParam<T> for WithDefault<T, De> {
    const EXPECTING: BasicTypes = De::EXPECTING;

    fn type_qualifiers(&self) -> TypeQualifiers {
        self.inner.type_qualifiers()
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

/// Deserializer decorator that wraps the output of the underlying decorator in `Some` and returns `None`
/// if the input for the param is missing.
#[derive(Debug)]
pub struct Optional<De>(pub De);

impl<T, De: DeserializeParam<T>> DeserializeParam<Option<T>> for Optional<De> {
    const EXPECTING: BasicTypes = De::EXPECTING;

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Option<T>, ErrorWithOrigin> {
        let current_value = ctx.current_value().map(|val| &val.inner);
        if matches!(current_value, None | Some(Value::Null)) {
            return Ok(None);
        }
        self.0.deserialize_param(ctx, param).map(Some)
    }
}

impl TimeUnit {
    fn overflow_err(self, raw_val: u64) -> serde_json::Error {
        let plural = self.plural();
        DeError::custom(format!(
            "{raw_val} {plural} does not fit into `u64` when converted to seconds"
        ))
    }
}

/// Supports deserializing a [`Duration`] from a number, with `self` being the unit of measurement.
///
/// # Examples
///
/// ```
/// # use std::time::Duration;
/// # use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};
/// use smart_config::testing;
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(with = TimeUnit::Millis)]
///     time_ms: Duration,
/// }
///
/// let source = smart_config::config!("time_ms": 100);
/// let config = testing::test::<TestConfig>(source)?;
/// assert_eq!(config.time_ms, Duration::from_millis(100));
/// # anyhow::Ok(())
/// ```
impl DeserializeParam<Duration> for TimeUnit {
    const EXPECTING: BasicTypes = BasicTypes::INTEGER;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new("time duration").with_unit((*self).into())
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Duration, ErrorWithOrigin> {
        const SECONDS_IN_MINUTE: u64 = 60;
        const SECONDS_IN_HOUR: u64 = 3_600;
        const SECONDS_IN_DAY: u64 = 86_400;

        let deserializer = ctx.current_value_deserializer(param.name)?;
        let raw_value = u64::deserialize(deserializer)?;
        Ok(match self {
            Self::Millis => Duration::from_millis(raw_value),
            Self::Seconds => Duration::from_secs(raw_value),
            Self::Minutes => {
                let val = raw_value
                    .checked_mul(SECONDS_IN_MINUTE)
                    .ok_or_else(|| deserializer.enrich_err(self.overflow_err(raw_value)))?;
                Duration::from_secs(val)
            }
            Self::Hours => {
                let val = raw_value
                    .checked_mul(SECONDS_IN_HOUR)
                    .ok_or_else(|| deserializer.enrich_err(self.overflow_err(raw_value)))?;
                Duration::from_secs(val)
            }
            Self::Days => {
                let val = raw_value
                    .checked_mul(SECONDS_IN_DAY)
                    .ok_or_else(|| deserializer.enrich_err(self.overflow_err(raw_value)))?;
                Duration::from_secs(val)
            }
        })
    }
}

/// Supports deserializing a [`ByteSize`] from a number, with `self` being the unit of measurement.
///
/// # Examples
///
/// ```
/// # use std::time::Duration;
/// # use smart_config::{metadata::SizeUnit, DescribeConfig, DeserializeConfig, ByteSize};
/// use smart_config::testing;
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(with = SizeUnit::MiB)]
///     size_mb: ByteSize,
/// }
///
/// let source = smart_config::config!("size_mb": 4);
/// let config = testing::test::<TestConfig>(source)?;
/// assert_eq!(config.size_mb, ByteSize(4 << 20));
/// # anyhow::Ok(())
/// ```
impl DeserializeParam<ByteSize> for SizeUnit {
    const EXPECTING: BasicTypes = BasicTypes::INTEGER;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new("byte size").with_unit((*self).into())
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<ByteSize, ErrorWithOrigin> {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        let raw_value = u64::deserialize(deserializer)?;
        ByteSize::checked(raw_value, *self).ok_or_else(|| {
            let err = DeError::custom(format!(
                "{raw_value} {unit} does not fit into `u64`",
                unit = self.plural()
            ));
            deserializer.enrich_err(err)
        })
    }
}

/// Deserializer that supports parsing either from a default format (usually an object or array) via [`Deserialize`](serde::Deserialize),
/// or from string via [`FromStr`].
///
/// # Examples
///
/// ```
/// # use std::{collections::HashSet, str::FromStr};
/// use anyhow::Context as _;
/// # use serde::Deserialize;
/// use smart_config::{de, testing, DescribeConfig, DeserializeConfig};
///
/// #[derive(Deserialize)]
/// #[serde(transparent)]
/// struct MySet(HashSet<u64>);
///
/// impl FromStr for MySet {
///     type Err = anyhow::Error;
///
///     fn from_str(s: &str) -> Result<Self, Self::Err> {
///         s.split(',')
///             .map(|part| part.trim().parse().context("invalid value"))
///             .collect::<anyhow::Result<_>>()
///             .map(Self)
///     }
/// }
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(with = de::OrString(de::Serde![array]))]
///     value: MySet,
/// }
///
/// let sample = smart_config::config!("value": "2, 3, 2");
/// let config: TestConfig = testing::test(sample)?;
/// assert_eq!(config.value.0, HashSet::from([2, 3]));
///
/// // Parsing from array works, too
/// let sample = smart_config::config!("value": [2, 3, 2]);
/// let config: TestConfig = testing::test(sample)?;
/// assert_eq!(config.value.0, HashSet::from([2, 3]));
/// # anyhow::Ok(())
/// ```
#[derive(Debug)]
pub struct OrString<De>(pub De);

impl<T, De> DeserializeParam<T> for OrString<De>
where
    T: FromStr,
    T::Err: fmt::Display,
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = <De as DeserializeParam<T>>::EXPECTING.or(BasicTypes::STRING);

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        let Some(WithOrigin {
            inner: Value::String(s),
            origin,
        }) = ctx.current_value()
        else {
            return self.0.deserialize_param(ctx, param);
        };

        T::from_str(s).map_err(|err| {
            let err = serde_json::Error::custom(err);
            ErrorWithOrigin::json(err, origin.clone())
        })
    }
}
