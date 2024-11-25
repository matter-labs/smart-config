//! Parameter deserializers.

use std::{
    any,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    hash::Hash,
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
    ByteSize,
};

#[diagnostic::on_unimplemented(
    message = "`{T}` param cannot be deserialized",
    note = "Add #[config(with = Serde(_)] attribute to deserialize any param implementing `serde::Deserialize`"
)]
pub trait DeserializeParam<T>: fmt::Debug + Send + Sync + 'static {
    fn expecting(&self) -> SchemaType;

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin>;
}

/// Generic [`DeserializeParam`] implementation for any type implementing [`serde::Deserialize`].
/// Usually makes sense if the param type is not [`WellKnown`], and it cannot be marked as such.
#[derive(Debug)]
pub struct Assume(pub BasicType);

impl<T: DeserializeOwned> DeserializeParam<T> for Assume {
    fn expecting(&self) -> SchemaType {
        SchemaType::new(self.0)
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        // Permissively assume that optional values are allowed.
        if let Some(actual_type) = deserializer.value().basic_type() {
            let types_match = actual_type == self.0
                || (self.0 == BasicType::Float && actual_type == BasicType::Integer);
            if !types_match {
                return Err(deserializer.invalid_type(self.0.as_str()));
            }
        }
        T::deserialize(deserializer)
    }
}

pub trait WellKnown: 'static + DeserializeOwned {
    const TYPE: SchemaType;
}

impl WellKnown for bool {
    const TYPE: SchemaType = SchemaType::new(BasicType::Bool);
}

impl WellKnown for String {
    const TYPE: SchemaType = SchemaType::new(BasicType::String);
}

impl WellKnown for PathBuf {
    const TYPE: SchemaType = SchemaType::new(BasicType::String).with_qualifier("filesystem path");
}

impl WellKnown for f32 {
    const TYPE: SchemaType = SchemaType::new(BasicType::Float);
}

impl WellKnown for f64 {
    const TYPE: SchemaType = SchemaType::new(BasicType::Float);
}

macro_rules! impl_well_known_int {
    ($($int:ty),+) => {
        $(
        impl WellKnown for $int {
            const TYPE: SchemaType = SchemaType::new(BasicType::Integer);
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
    const TYPE: SchemaType = T::TYPE;
}

impl<T: WellKnown> WellKnown for Vec<T> {
    const TYPE: SchemaType = SchemaType::new(BasicType::Array);
}

impl<T: WellKnown, const N: usize> WellKnown for [T; N]
where
    [T; N]: DeserializeOwned, // `serde` implements `Deserialize` for separate lengths rather for generics
{
    const TYPE: SchemaType = SchemaType::new(BasicType::Array);
}

// Heterogeneous tuples don't look like a good idea to mark as well-known because they wouldn't look well-structured
// (it'd be better to define either multiple params or a struct param).

impl<T: WellKnown + Eq + Hash> WellKnown for HashSet<T> {
    const TYPE: SchemaType = SchemaType::new(BasicType::Array).with_qualifier("set");
}

impl<T: WellKnown + Eq + Ord> WellKnown for BTreeSet<T> {
    const TYPE: SchemaType = SchemaType::new(BasicType::Array).with_qualifier("set");
}

/// Keys are intentionally restricted by [`FromStr`] in order to prevent runtime errors when dealing with keys
/// that do not serialize to strings.
impl<K, V> WellKnown for HashMap<K, V>
where
    K: 'static + DeserializeOwned + Eq + Hash + FromStr,
    V: WellKnown,
{
    const TYPE: SchemaType = SchemaType::new(BasicType::Object).with_qualifier("map");
}

impl<K, V> WellKnown for BTreeMap<K, V>
where
    K: 'static + DeserializeOwned + Eq + Ord + FromStr,
    V: WellKnown,
{
    const TYPE: SchemaType = SchemaType::new(BasicType::Object).with_qualifier("map");
}

/// Default [`DeserializeParam`] implementation used unless it is explicitly overwritten via `#[config(with = _)]`
/// attribute.
pub struct DefaultDeserializer<T>(PhantomData<fn(T)>);

#[allow(clippy::new_without_default)] // won't make much sense, since it cannot be used in const contexts
impl<T: 'static> DefaultDeserializer<T> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: 'static> fmt::Debug for DefaultDeserializer<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DefaultDeserializer")
            .field("type", &any::type_name::<T>())
            .finish()
    }
}

impl<T: WellKnown> DeserializeParam<T> for DefaultDeserializer<T> {
    fn expecting(&self) -> SchemaType {
        T::TYPE
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        T::deserialize(ctx.current_value_deserializer(param.name)?)
    }
}

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
            .finish()
    }
}

impl<T: 'static, D: DeserializeParam<T>> WithDefault<T, D> {
    pub const fn new(inner: D, default: fn() -> T) -> Self {
        Self { inner, default }
    }
}

impl<T: 'static, D: DeserializeParam<T>> DeserializeParam<T> for WithDefault<T, D> {
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
            String::deserialize(ValueDeserializer::new(current_value))?
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
pub trait ObjectSafeDeserializer: 'static + fmt::Debug + Send + Sync {
    fn expecting(&self) -> SchemaType;

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Box<dyn any::Any>, ErrorWithOrigin>;
}

#[doc(hidden)]
pub struct DeserializerWrapper<T, D> {
    inner: D,
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

impl<T: 'static, D: DeserializeParam<T>> DeserializerWrapper<T, D> {
    pub const fn new(inner: D) -> Self {
        Self {
            inner,
            _ty: PhantomData,
        }
    }
}

impl<T: 'static, D: DeserializeParam<T>> ObjectSafeDeserializer for DeserializerWrapper<T, D> {
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
