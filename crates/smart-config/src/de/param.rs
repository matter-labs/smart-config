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
    time::Duration,
};

use serde::{
    de::{DeserializeOwned, Error as DeError},
    Deserialize,
};

use crate::{
    de::{deserializer::ValueDeserializer, DeserializeContext},
    error::ErrorWithOrigin,
    metadata::{BasicType, ParamMetadata, SchemaType, SizeUnit, TimeUnit},
    value::Value,
    ByteSize,
};

/// Deserializes a parameter of the specified type.
pub trait DeserializeParam<T>: fmt::Debug + Send + Sync + 'static {
    /// Describes which parameter this deserializer is expecting.
    fn expecting(&self) -> SchemaType;

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

impl<T, De> DeserializeParam<T> for &'static De
where
    De: DeserializeParam<T> + ?Sized,
{
    fn expecting(&self) -> SchemaType {
        (*self).expecting()
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        (*self).deserialize_param(ctx, param)
    }
}

/// Parameter type with well-known [deserializer](DeserializeParam).
///
/// Conceptually, this means that the type is known to behave well when deserializing data from a [`Value`]
/// (ordinarily, using [`serde::Deserialize`]).
#[diagnostic::on_unimplemented(
    message = "`{Self}` param cannot be deserialized",
    note = "Add #[config(with = _)] attribute to specify deserializer to use"
)]
pub trait WellKnown: 'static {
    /// Standard deserializer for the param.
    const DE: &'static dyn DeserializeParam<Self>;
}

/// This deserializer assumes that the value is required. Hence, optional params should be wrapped in [`Optional`] to work correctly.
impl<T: DeserializeOwned> DeserializeParam<T> for SchemaType {
    fn expecting(&self) -> SchemaType {
        *self
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        // Permissively assume that optional values are allowed.
        if let (Some(actual_type), Some(expected_type)) =
            (deserializer.value().basic_type(), self.base)
        {
            let types_match = actual_type == expected_type
                || (expected_type == BasicType::Float && actual_type == BasicType::Integer)
                // FIXME: probably worth to get rid of
                || (expected_type == BasicType::Array && actual_type == BasicType::String);
            if !types_match {
                return Err(deserializer.invalid_type(expected_type.as_str()));
            }
        }
        T::deserialize(deserializer)
    }
}

/// Proxies to the corresponding [`SchemaType`].
impl<T: DeserializeOwned> DeserializeParam<T> for BasicType {
    fn expecting(&self) -> SchemaType {
        SchemaType::new(*self)
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        SchemaType::new(*self).deserialize_param(ctx, param)
    }
}

impl WellKnown for bool {
    const DE: &'static dyn DeserializeParam<Self> = &SchemaType::new(BasicType::Bool);
}

impl WellKnown for String {
    const DE: &'static dyn DeserializeParam<Self> = &SchemaType::new(BasicType::String);
}

impl WellKnown for PathBuf {
    const DE: &'static dyn DeserializeParam<Self> =
        &SchemaType::new(BasicType::String).with_qualifier("filesystem path");
}

impl WellKnown for f32 {
    const DE: &'static dyn DeserializeParam<Self> = &SchemaType::new(BasicType::Float);
}

impl WellKnown for f64 {
    const DE: &'static dyn DeserializeParam<Self> = &SchemaType::new(BasicType::Float);
}

macro_rules! impl_well_known_int {
    ($($int:ty),+) => {
        $(
        impl WellKnown for $int {
            const DE: &'static dyn DeserializeParam<Self> = &SchemaType::new(BasicType::Integer);
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
    const DE: &'static dyn DeserializeParam<Self> = &Optional(T::DE);
}

impl<T: WellKnown + DeserializeOwned> WellKnown for Vec<T> {
    const DE: &'static dyn DeserializeParam<Self> = &SchemaType::new(BasicType::Array);
}

impl<T: WellKnown, const N: usize> WellKnown for [T; N]
where
    [T; N]: DeserializeOwned, // `serde` implements `Deserialize` for separate lengths rather for generics
{
    const DE: &'static dyn DeserializeParam<Self> = &SchemaType::new(BasicType::Array);
}

// Heterogeneous tuples don't look like a good idea to mark as well-known because they wouldn't look well-structured
// (it'd be better to define either multiple params or a struct param).

impl<T, S> WellKnown for HashSet<T, S>
where
    T: WellKnown + Eq + Hash + DeserializeOwned,
    S: 'static + Default + BuildHasher,
{
    const DE: &'static dyn DeserializeParam<Self> =
        &SchemaType::new(BasicType::Array).with_qualifier("set");
}

impl<T> WellKnown for BTreeSet<T>
where
    T: WellKnown + Eq + Ord + DeserializeOwned,
{
    const DE: &'static dyn DeserializeParam<Self> =
        &SchemaType::new(BasicType::Array).with_qualifier("set");
}

/// Keys are intentionally restricted by [`FromStr`] in order to prevent runtime errors when dealing with keys
/// that do not serialize to strings.
impl<K, V, S> WellKnown for HashMap<K, V, S>
where
    K: 'static + DeserializeOwned + Eq + Hash + FromStr,
    V: WellKnown + DeserializeOwned,
    S: 'static + Default + BuildHasher,
{
    const DE: &'static dyn DeserializeParam<Self> =
        &SchemaType::new(BasicType::Object).with_qualifier("map");
}

impl<K, V> WellKnown for BTreeMap<K, V>
where
    K: 'static + DeserializeOwned + Eq + Ord + FromStr,
    V: WellKnown + DeserializeOwned,
{
    const DE: &'static dyn DeserializeParam<Self> =
        &SchemaType::new(BasicType::Object).with_qualifier("map");
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
    fn expecting(&self) -> SchemaType {
        self.inner.expecting()
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
    fn expecting(&self) -> SchemaType {
        self.0.expecting()
    }

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
    fn expecting(&self) -> SchemaType {
        SchemaType::new(BasicType::String)
    }

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

impl DeserializeParam<Duration> for TimeUnit {
    fn expecting(&self) -> SchemaType {
        SchemaType::new(BasicType::Integer)
            .with_qualifier("time duration")
            .with_unit((*self).into())
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

impl DeserializeParam<ByteSize> for SizeUnit {
    fn expecting(&self) -> SchemaType {
        SchemaType::new(BasicType::Integer)
            .with_qualifier("byte size")
            .with_unit((*self).into())
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

/// Object-safe part of parameter deserializer. Stored in param metadata.
#[doc(hidden)]
pub trait ObjectSafeDeserializer: 'static + fmt::Debug + Send + Sync {
    /// Describes which parameter this deserializer is expecting.
    fn expecting(&self) -> SchemaType;

    /// Performs deserialization given the context and param metadata and wraps the output in a type-erased `Box`.
    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Box<dyn any::Any>, ErrorWithOrigin>;
}

/// Wrapper transforming [`DeserializeParam`] to [`ObjectSafeDeserializer`].
#[doc(hidden)]
pub struct DeserializerWrapper<T, De> {
    inner: De,
    _ty: PhantomData<fn(T)>,
}

impl<T, D: fmt::Debug> fmt::Debug for DeserializerWrapper<T, D> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("DeserializerWrapper")
            .field(&self.inner)
            .finish()
    }
}

impl<T: 'static, De: DeserializeParam<T>> DeserializerWrapper<T, De> {
    pub const fn new(inner: De) -> Self {
        Self {
            inner,
            _ty: PhantomData,
        }
    }
}

impl<T: 'static, De: DeserializeParam<T>> ObjectSafeDeserializer for DeserializerWrapper<T, De> {
    fn expecting(&self) -> SchemaType {
        self.inner.expecting()
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
