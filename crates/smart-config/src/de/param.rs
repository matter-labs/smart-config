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
    de::{value::UnitDeserializer, DeserializeOwned, Error as DeError},
    Deserialize,
};

use crate::{
    de::{deserializer::ValueDeserializer, DeserializeContext},
    error::ErrorWithOrigin,
    metadata::{BasicTypes, ParamMetadata, SizeUnit, TimeUnit, TypeQualifiers},
    value::{Value, ValueOrigin, WithOrigin},
    ByteSize,
};

/// Marks the param as well-known for a [deserializer](DeserializeParam).
#[diagnostic::on_unimplemented(
    message = "`{T}` param cannot be deserialized",
    note = "Add #[config(with = _)] attribute to specify deserializer to use",
    note = "Or, if you own the type, implement `ExpectParam<{T}> for {Self}`"
)]
pub trait ExpectParam<T>: DeserializeParam<T> {
    /// Describes which parameter this deserializer is expecting.
    const EXPECTING: BasicTypes;
    /// Marks whether the underlying value is required.
    const IS_REQUIRED: bool = true;

    /// Provides an extended type description.
    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::default()
    }
}

/// Deserializes a parameter of the specified type.
///
/// # Implementations
///
/// `DeserializeParam` includes the following implementations:
///
/// - `()` is the default deserializer used unless explicitly overwritten with `#[config(with = _)]`.
///   It supports types known to deserialize well (see [below](#well-known-types)), and can be switched for user-defined types
///   by implementing [`ExpectParam`]`<()>` for the type.
/// - [`Serde`] allows deserializing any type implementing [`serde::Deserialize`].
/// - [`Optional`] decorates a deserializer for `T` turning it into a deserializer for `Option<T>`
/// - [`TimeUnit`] allows deserializing [`Duration`] from a number
/// - [`SizeUnit`] similarly allows deserializing [`ByteSize`]
///
/// # Well-known types
///
/// Basic well-known types include:
///
/// - `bool`
/// - [`String`]
/// - [`PathBuf`]
/// - Signed and unsigned integers, including non-zero variants
/// - `f32`, `f64`
///
/// It is also implemented for collections, items are well-known themselves:
///
/// - [`Option`]
/// - [`Vec`], arrays up to 32 elements (which is a `serde` restriction, not a local one)
/// - [`HashSet`], [`BTreeSet`]
/// - [`HashMap`], [`BTreeMap`]
pub trait DeserializeParam<T>: fmt::Debug + Send + Sync + 'static {
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

impl<T: DeserializeOwned, const EXPECTING: u8> ExpectParam<T> for Serde<EXPECTING> {
    const EXPECTING: BasicTypes = BasicTypes::from_raw(EXPECTING);
    const IS_REQUIRED: bool = true;
}

impl BasicTypes {
    fn deserialize_param<T: DeserializeOwned>(
        self,
        ctx: &DeserializeContext<'_>,
        param: &'static ParamMetadata,
        is_required: bool,
    ) -> Result<T, ErrorWithOrigin> {
        let Some(current_value) = ctx.current_value() else {
            if is_required {
                return Err(DeError::missing_field(param.name));
            }
            return T::deserialize(UnitDeserializer::new());
        };

        let deserializer = ValueDeserializer::new(current_value, ctx.de_options);
        let type_matches = deserializer.value().is_supported_by(self);
        if !type_matches {
            return Err(deserializer.invalid_type(&self.to_string()));
        }
        T::deserialize(deserializer)
    }
}

impl<T: DeserializeOwned, const EXPECTING: u8> DeserializeParam<T> for Serde<EXPECTING> {
    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        <Self as ExpectParam<T>>::EXPECTING.deserialize_param(&ctx, param, true)
    }
}

impl<T> DeserializeParam<T> for ()
where
    T: DeserializeOwned,
    Self: ExpectParam<T>,
{
    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        <Self as ExpectParam<T>>::EXPECTING.deserialize_param(
            &ctx,
            param,
            <Self as ExpectParam<T>>::IS_REQUIRED,
        )
    }
}

impl ExpectParam<bool> for () {
    const EXPECTING: BasicTypes = BasicTypes::BOOL;
}

impl ExpectParam<String> for () {
    const EXPECTING: BasicTypes = BasicTypes::STRING;
}

impl ExpectParam<PathBuf> for () {
    const EXPECTING: BasicTypes = BasicTypes::STRING;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::default().with_description("filesystem path")
    }
}

impl ExpectParam<f32> for () {
    const EXPECTING: BasicTypes = BasicTypes::FLOAT;
}

impl ExpectParam<f64> for () {
    const EXPECTING: BasicTypes = BasicTypes::FLOAT;
}

macro_rules! impl_well_known_int {
    ($($int:ty),+) => {
        $(
        impl ExpectParam<$int> for () {
            const EXPECTING: BasicTypes = BasicTypes::INTEGER;
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

impl<T: DeserializeOwned> ExpectParam<Option<T>> for ()
where
    Self: ExpectParam<T>,
{
    const EXPECTING: BasicTypes = <Self as ExpectParam<T>>::EXPECTING;
    const IS_REQUIRED: bool = false;

    fn type_qualifiers(&self) -> TypeQualifiers {
        ExpectParam::<T>::type_qualifiers(&()) // copy qualifiers of the base param
    }
}

impl<T: DeserializeOwned> ExpectParam<Vec<T>> for ()
where
    Self: ExpectParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;
}

impl<T, const N: usize> ExpectParam<[T; N]> for ()
where
    [T; N]: DeserializeOwned, // `serde` implements `Deserialize` for separate lengths rather for generics
    Self: ExpectParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::default().with_dyn_description(format!("with {N} elements"))
    }
}

// Heterogeneous tuples don't look like a good idea to mark as well-known because they wouldn't look well-structured
// (it'd be better to define either multiple params or a struct param).

impl<T, S> ExpectParam<HashSet<T, S>> for ()
where
    T: Eq + Hash + DeserializeOwned,
    S: 'static + Default + BuildHasher,
    Self: ExpectParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::default().with_description("set")
    }
}

impl<T> ExpectParam<BTreeSet<T>> for ()
where
    T: Eq + Ord + DeserializeOwned,
    Self: ExpectParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::default().with_description("set")
    }
}

/// Keys are intentionally restricted by [`FromStr`] in order to prevent runtime errors when dealing with keys
/// that do not serialize to strings.
impl<K, V, S> ExpectParam<HashMap<K, V, S>> for ()
where
    K: 'static + DeserializeOwned + Eq + Hash + FromStr,
    V: DeserializeOwned,
    S: 'static + Default + BuildHasher,
    Self: ExpectParam<V>,
{
    const EXPECTING: BasicTypes = BasicTypes::OBJECT;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::default().with_description("map")
    }
}

impl<K, V> ExpectParam<BTreeMap<K, V>> for ()
where
    K: 'static + DeserializeOwned + Eq + Ord + FromStr,
    V: DeserializeOwned,
    Self: ExpectParam<V>,
{
    const EXPECTING: BasicTypes = BasicTypes::OBJECT;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::default().with_description("map")
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

impl<T: 'static, De: ExpectParam<T>> ExpectParam<T> for WithDefault<T, De> {
    const EXPECTING: BasicTypes = De::EXPECTING;
    const IS_REQUIRED: bool = false;

    fn type_qualifiers(&self) -> TypeQualifiers {
        self.inner.type_qualifiers()
    }
}

impl<T: 'static, De: DeserializeParam<T>> DeserializeParam<T> for WithDefault<T, De> {
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

impl<T, De: ExpectParam<T>> ExpectParam<Option<T>> for Optional<De> {
    const EXPECTING: BasicTypes = De::EXPECTING;
    const IS_REQUIRED: bool = false;
}

impl<T, De: DeserializeParam<T>> DeserializeParam<Option<T>> for Optional<De> {
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

impl ExpectParam<&'static str> for TagDeserializer {
    const EXPECTING: BasicTypes = BasicTypes::STRING;
}

impl DeserializeParam<&'static str> for TagDeserializer {
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

impl ExpectParam<Duration> for TimeUnit {
    const EXPECTING: BasicTypes = BasicTypes::INTEGER;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::default()
            .with_description("time duration")
            .with_unit((*self).into())
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

impl ExpectParam<ByteSize> for SizeUnit {
    const EXPECTING: BasicTypes = BasicTypes::INTEGER;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::default()
            .with_description("byte size")
            .with_unit((*self).into())
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
///     // will fail with "evaluation of `<Delimited as ExpectParam<u64>>::EXPECTING` failed"
///     #[config(default, with = de::Delimited(","))]
///     test: u64,
/// }
/// ```
#[derive(Debug)]
pub struct Delimited(pub &'static str);

impl<T: DeserializeOwned> ExpectParam<T> for Delimited
where
    (): ExpectParam<T>,
{
    const EXPECTING: BasicTypes = {
        let base = <() as ExpectParam<T>>::EXPECTING;
        assert!(
            base.contains(BasicTypes::ARRAY),
            "can only apply `Delimited` to types that support deserialization from array"
        );
        base.or(BasicTypes::STRING)
    };

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::default().with_dyn_description(format!("using {:?} delimiter", self.0))
    }
}

impl<T: DeserializeOwned> DeserializeParam<T> for Delimited
where
    (): ExpectParam<T>,
{
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
            return ().deserialize_param(ctx, param);
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
        ().deserialize_param(ctx.patched(&array), param)
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

impl<T, De> ExpectParam<T> for OrString<De>
where
    T: FromStr,
    T::Err: fmt::Display,
    De: ExpectParam<T>,
{
    const EXPECTING: BasicTypes = <De as ExpectParam<T>>::EXPECTING.or(BasicTypes::STRING);
}

impl<T, De> DeserializeParam<T> for OrString<De>
where
    T: FromStr,
    T::Err: fmt::Display,
    De: ExpectParam<T>,
{
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
pub trait ErasedDeserializer: DeserializeParam<Box<dyn any::Any>> {
    fn type_qualifiers(&self) -> TypeQualifiers;
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

impl<T: 'static, De: ExpectParam<T>> DeserializeParam<Box<dyn any::Any>> for Erased<T, De> {
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

impl<T: 'static, De: ExpectParam<T>> ErasedDeserializer for Erased<T, De> {
    fn type_qualifiers(&self) -> TypeQualifiers {
        self.inner.type_qualifiers()
    }
}
