//! Parameter deserializers.

use std::{
    any,
    collections::{HashMap, HashSet},
    fmt,
    hash::Hash,
    marker::PhantomData,
    num::{
        NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroIsize, NonZeroU16, NonZeroU32,
        NonZeroU64, NonZeroU8, NonZeroUsize,
    },
    path::PathBuf,
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
/// Usually makes sense if the param type is not [`WellKnown`].
#[derive(Debug)]
pub struct Serde(pub BasicType);

impl<T: DeserializeOwned> DeserializeParam<T> for Serde {
    fn expecting(&self) -> SchemaType {
        SchemaType::new(self.0)
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        T::deserialize(ctx.current_value_deserializer(param.name)?)
    }
}

/// Default [`DeserializeParam`] implementation used unless it is explicitly overwritten via `#[config(with = _)]`
/// attribute.
pub struct WellKnown<T>(PhantomData<fn(T)>);

#[allow(clippy::new_without_default)] // won't make much sense, since it cannot be used in const contexts
impl<T: 'static> WellKnown<T> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: 'static> fmt::Debug for WellKnown<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WellKnown")
            .field("type", &any::type_name::<T>())
            .finish()
    }
}

macro_rules! impl_well_known_deserialize {
    ($ty:ty, $expecting:expr) => {
        impl DeserializeParam<$ty> for WellKnown<$ty> {
            fn expecting(&self) -> SchemaType {
                $expecting
            }

            fn deserialize_param(
                &self,
                ctx: DeserializeContext<'_>,
                param: &'static ParamMetadata,
            ) -> Result<$ty, ErrorWithOrigin> {
                <$ty as Deserialize>::deserialize(ctx.current_value_deserializer(param.name)?)
            }
        }
    };
}

impl_well_known_deserialize!(bool, SchemaType::new(BasicType::Bool));

impl_well_known_deserialize!(u8, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(i8, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(u16, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(i16, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(u32, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(i32, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(u64, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(i64, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(u128, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(i128, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(usize, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(isize, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(NonZeroU8, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(NonZeroI8, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(NonZeroU16, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(NonZeroI16, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(NonZeroU32, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(NonZeroI32, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(NonZeroU64, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(NonZeroI64, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(NonZeroUsize, SchemaType::new(BasicType::Integer));
impl_well_known_deserialize!(NonZeroIsize, SchemaType::new(BasicType::Integer));

impl_well_known_deserialize!(f32, SchemaType::new(BasicType::Float));
impl_well_known_deserialize!(f64, SchemaType::new(BasicType::Float));

impl_well_known_deserialize!(String, SchemaType::new(BasicType::String));
impl_well_known_deserialize!(PathBuf, SchemaType::new(BasicType::String));

impl<T> DeserializeParam<Option<T>> for WellKnown<Option<T>>
where
    T: 'static + DeserializeOwned,
    WellKnown<T>: DeserializeParam<T>,
{
    fn expecting(&self) -> SchemaType {
        WellKnown::<T>::new().expecting()
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Option<T>, ErrorWithOrigin> {
        let Ok(deserializer) = ctx.current_value_deserializer(param.name) else {
            return Ok(None);
        };
        <Option<T>>::deserialize(deserializer)
    }
}

impl<T: 'static + DeserializeOwned> DeserializeParam<Vec<T>> for WellKnown<Vec<T>> {
    fn expecting(&self) -> SchemaType {
        SchemaType::new(BasicType::Array)
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Vec<T>, ErrorWithOrigin> {
        <Vec<T>>::deserialize(ctx.current_value_deserializer(param.name)?)
    }
}

impl<T> DeserializeParam<HashSet<T>> for WellKnown<HashSet<T>>
where
    T: 'static + Eq + Hash + DeserializeOwned,
{
    fn expecting(&self) -> SchemaType {
        SchemaType::new(BasicType::Array)
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<HashSet<T>, ErrorWithOrigin> {
        <HashSet<T>>::deserialize(ctx.current_value_deserializer(param.name)?)
    }
}

impl<K, V> DeserializeParam<HashMap<K, V>> for WellKnown<HashMap<K, V>>
where
    K: 'static + Eq + Hash + DeserializeOwned,
    V: 'static + DeserializeOwned,
{
    fn expecting(&self) -> SchemaType {
        SchemaType::new(BasicType::Object)
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<HashMap<K, V>, ErrorWithOrigin> {
        <HashMap<K, V>>::deserialize(ctx.current_value_deserializer(param.name)?)
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
