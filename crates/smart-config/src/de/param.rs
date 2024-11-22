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
};

use serde::{
    de::{DeserializeOwned, Error as DeError},
    Deserialize,
};

use crate::{
    de::{deserializer::ValueDeserializer, DeserializeContext},
    error::ErrorWithOrigin,
    metadata::{BasicType, ParamMetadata, SchemaType},
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
    const TYPE: SchemaType = SchemaType::new(BasicType::String);
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
    const TYPE: SchemaType = SchemaType::new(BasicType::Array);
}

impl<T: WellKnown + Eq + Ord> WellKnown for BTreeSet<T> {
    const TYPE: SchemaType = SchemaType::new(BasicType::Array);
}

impl<K, V> WellKnown for HashMap<K, V>
where
    K: 'static + DeserializeOwned + Eq + Hash + FromStr,
    V: WellKnown,
{
    const TYPE: SchemaType = SchemaType::new(BasicType::Object);
}

impl<K, V> WellKnown for BTreeMap<K, V>
where
    K: 'static + DeserializeOwned + Eq + Ord + FromStr,
    V: WellKnown,
{
    const TYPE: SchemaType = SchemaType::new(BasicType::Object);
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
            .debug_struct("WellKnown")
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
