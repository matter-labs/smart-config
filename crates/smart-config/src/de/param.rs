//! Parameter deserializers.

use std::{
    any,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    hash::{BuildHasher, Hash},
    marker::PhantomData,
    num::{
        NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroIsize, NonZeroU16, NonZeroU32,
        NonZeroU64, NonZeroU8, NonZeroUsize,
    },
    path::PathBuf,
    str::FromStr,
    sync::Arc,
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
    value::{Value, ValueOrigin, WithOrigin},
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
/// - [`Vec`], arrays up to 32 elements (which is a `serde` restriction, not a local one)
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

impl<T: WellKnown + DeserializeOwned> WellKnown for Vec<T> {
    type Deserializer = super::Serde![array];
    const DE: Self::Deserializer = super::Serde![array];
}

impl<T, const N: usize> WellKnown for [T; N]
where
    [T; N]: DeserializeOwned, // `serde` implements `Deserialize` for separate lengths rather for generics
{
    type Deserializer = Qualified<super::Serde![array]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![array], "fixed-size array");
}

// Heterogeneous tuples don't look like a good idea to mark as well-known because they wouldn't look well-structured
// (it'd be better to define either multiple params or a struct param).

impl<T, S> WellKnown for HashSet<T, S>
where
    T: Eq + Hash + DeserializeOwned + WellKnown,
    S: 'static + Default + BuildHasher,
{
    type Deserializer = Qualified<super::Serde![array]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![array], "set");
}

impl<T> WellKnown for BTreeSet<T>
where
    T: Eq + Ord + DeserializeOwned + WellKnown,
{
    type Deserializer = Qualified<super::Serde![array]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![array], "set");
}

/// Keys are intentionally restricted by [`FromStr`] in order to prevent runtime errors when dealing with keys
/// that do not serialize to strings.
impl<K, V, S> WellKnown for HashMap<K, V, S>
where
    K: 'static + DeserializeOwned + Eq + Hash + FromStr,
    V: DeserializeOwned + WellKnown,
    S: 'static + Default + BuildHasher,
{
    type Deserializer = Qualified<super::Serde![object]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![object], "map");
}

impl<K, V> WellKnown for BTreeMap<K, V>
where
    K: 'static + DeserializeOwned + Eq + Ord + FromStr,
    V: DeserializeOwned + WellKnown,
{
    type Deserializer = Qualified<super::Serde![object]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![object], "map");
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

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        self.inner.deserialize_param(ctx, param)
    }

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new(self.description)
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

/// Deserializer for enum tags.
#[doc(hidden)] // Implementation detail
#[derive(Debug)]
pub struct TagDeserializer {
    expected: &'static [&'static str],
    default_value: Option<&'static str>,
}

impl TagDeserializer {
    pub const fn new(
        expected: &'static [&'static str],
        default_value: Option<&'static str>,
    ) -> Self {
        Self {
            expected,
            default_value,
        }
    }
}

impl DeserializeParam<&'static str> for TagDeserializer {
    const EXPECTING: BasicTypes = BasicTypes::STRING;

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<&'static str, ErrorWithOrigin> {
        let s = if let Some(current_value) = ctx.current_value() {
            String::deserialize(ValueDeserializer::new(current_value, ctx.de_options))?
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

/// Deserializer that supports either an array of values, or a string in which values are delimited
/// by the specified separator.
///
/// # Examples
///
/// ```
/// use std::{collections::HashSet, path::PathBuf};
/// use smart_config::{de, testing, DescribeConfig, DeserializeConfig};
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(default, with = de::Delimited(","))]
///     strings: Vec<String>,
///     // More complex types are supported as well
///     #[config(with = de::Delimited(":"))]
///     paths: Vec<PathBuf>,
///     // ...and more complex collections (here together with string -> number coercion)
///     #[config(with = de::Delimited(";"))]
///     ints: HashSet<u64>,
/// }
///
/// let sample = smart_config::config!(
///     "strings": ["test", "string"], // standard array value is still supported
///     "paths": "/usr/bin:/usr/local/bin",
///     "ints": "12;34;12",
/// );
/// let config: TestConfig = testing::test(sample)?;
/// assert_eq!(config.strings.len(), 2);
/// assert_eq!(config.strings[0], "test");
/// assert_eq!(config.paths.len(), 2);
/// assert_eq!(config.paths[1].as_os_str(), "/usr/local/bin");
/// assert_eq!(config.ints, HashSet::from([12, 34]));
/// # anyhow::Ok(())
/// ```
///
/// The wrapping logic is smart enough to catch in compile time an attempt to apply `Delimited` to a type
/// that cannot be deserialized from an array:
///
/// ```compile_fail
/// use smart_config::{de, DescribeConfig, DeserializeConfig};
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct Fail {
///     // will fail with "evaluation of `<Delimited as DeserializeParam<u64>>::EXPECTING` failed"
///     #[config(default, with = de::Delimited(","))]
///     test: u64,
/// }
/// ```
#[derive(Debug)]
pub struct Delimited(pub &'static str);

impl<T: DeserializeOwned + WellKnown> DeserializeParam<T> for Delimited {
    const EXPECTING: BasicTypes = {
        let base = <T::Deserializer as DeserializeParam<T>>::EXPECTING;
        assert!(
            base.contains(BasicTypes::ARRAY),
            "can only apply `Delimited` to types that support deserialization from array"
        );
        base.or(BasicTypes::STRING)
    };

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::dynamic(format!("using {:?} delimiter", self.0))
    }

    fn deserialize_param(
        &self,
        mut ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        let Some(WithOrigin {
            inner: Value::String(s),
            origin,
        }) = ctx.current_value()
        else {
            return T::DE.deserialize_param(ctx, param);
        };

        let array_origin = Arc::new(ValueOrigin::Synthetic {
            source: origin.clone(),
            transform: format!("{:?}-delimited string", self.0),
        });
        let array_items = s.split(self.0).enumerate().map(|(i, part)| {
            let item_origin = ValueOrigin::Path {
                source: array_origin.clone(),
                path: i.to_string(),
            };
            WithOrigin::new(Value::String(part.to_owned()), Arc::new(item_origin))
        });
        let array = WithOrigin::new(Value::Array(array_items.collect()), array_origin);
        T::DE.deserialize_param(ctx.patched(&array), param)
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
            ErrorWithOrigin::new(err, origin.clone())
        })
    }
}

/// Erased counterpart of a parameter deserializer. Stored in param metadata.
#[doc(hidden)]
pub trait ErasedDeserializer: fmt::Debug + Send + Sync + 'static {
    fn type_qualifiers(&self) -> TypeQualifiers;

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Box<dyn any::Any>, ErrorWithOrigin>;
}

/// Wrapper transforming [`DeserializeParam`] to [`ErasedDeserializer`].
#[doc(hidden)]
pub struct Erased<T, De> {
    inner: De,
    _ty: PhantomData<fn(T)>,
}

impl<T, D: fmt::Debug> fmt::Debug for Erased<T, D> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Erased").field(&self.inner).finish()
    }
}

impl<T: 'static, De: DeserializeParam<T>> Erased<T, De> {
    pub const fn new(inner: De) -> Self {
        Self {
            inner,
            _ty: PhantomData,
        }
    }
}

impl<T: 'static, De: DeserializeParam<T>> ErasedDeserializer for Erased<T, De> {
    fn type_qualifiers(&self) -> TypeQualifiers {
        self.inner.type_qualifiers()
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Box<dyn any::Any>, ErrorWithOrigin> {
        self.inner
            .deserialize_param(ctx, param)
            .map(|val| Box::new(val) as _)
    }
}
