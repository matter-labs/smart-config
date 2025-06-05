//! Parameter deserializers.

use std::{
    any, fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    num::{
        NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroIsize, NonZeroU16, NonZeroU32,
        NonZeroU64, NonZeroU8, NonZeroUsize,
    },
    path::PathBuf,
    str::FromStr,
};

use serde::{
    de::{DeserializeOwned, Error as DeError},
    Serialize,
};

use crate::{
    de::{deserializer::ValueDeserializer, DeserializeContext},
    error::ErrorWithOrigin,
    metadata::{BasicTypes, ParamMetadata, TypeDescription},
    value::{Value, WithOrigin},
};

/// Deserializes a parameter of the specified type.
///
/// # Implementations
///
/// ## Basic implementations
///
/// - [`Serde`] allows deserializing any type implementing [`serde::Deserialize`].
/// - [`TimeUnit`](crate::metadata::TimeUnit) deserializes [`Duration`](std::time::Duration)
///   from a numeric value that has the specified unit of measurement
/// - [`SizeUnit`](crate::metadata::SizeUnit) similarly deserializes [`ByteSize`](crate::ByteSize)
/// - [`WithUnit`](super::WithUnit) deserializes `Duration`s / `ByteSize`s as an integer + unit of measurement
///   (either in a string or object form).
///
/// ## Decorators
///
/// - [`Optional`] decorates a deserializer for `T` turning it into a deserializer for `Option<T>`
/// - [`WithDefault`] adds a default value used if the input is missing
/// - [`Delimited`](super::Delimited) allows deserializing arrays from a delimited string (e.g., comma-delimited)
/// - [`OrString`] allows to switch between structured and string deserialization
pub trait DeserializeParam<T>: fmt::Debug + Send + Sync + 'static {
    /// Describes which parameter this deserializer is expecting.
    const EXPECTING: BasicTypes;

    /// Additional info about the deserialized type, e.g., extended description.
    #[allow(unused)]
    fn describe(&self, description: &mut TypeDescription) {
        // Do nothing
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

    /// Serializes the provided parameter to the JSON model.
    ///
    /// Serialization is considered infallible (`serde_json` serialization may fail on recursive or very deeply nested data types;
    /// please don't use such data types for config params).
    fn serialize_param(&self, param: &T) -> serde_json::Value;
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
/// These types use [`Serde`] deserializer.
///
/// `WellKnown` is also implemented for more complex types:
///
/// | Rust type | Deserializer | Expected JSON |
/// |:-----------|:-------------|:----------------|
/// | [`Duration`](std::time::Duration) | [`WithUnit`](super::WithUnit) | string or object |
/// | [`ByteSize`](crate::ByteSize) | [`WithUnit`](super::WithUnit) | string or object |
/// | [`Option`] | [`Optional`] | value, or `null`, or nothing |
/// | [`Vec`], `[_; N]`, [`HashSet`](std::collections::HashSet), [`BTreeSet`](std::collections::BTreeSet) | [`Repeated`](super::Repeated) | array |
/// | [`HashMap`](std::collections::HashMap), [`BTreeMap`](std::collections::BTreeSet) | [`RepeatedEntries`](super::Entries) | object |
#[diagnostic::on_unimplemented(
    message = "`{Self}` param cannot be deserialized",
    note = "Add #[config(with = _)] attribute to specify deserializer to use",
    note = "If `{Self}` is a config, add #[config(nest)] or #[config(flatten)]"
)]
pub trait WellKnown: 'static + Sized {
    /// Type of the deserializer used for this type.
    type Deserializer: DeserializeParam<Self>;
    /// Deserializer instance.
    const DE: Self::Deserializer;
}

impl<T: WellKnown> DeserializeParam<T> for () {
    const EXPECTING: BasicTypes = <T::Deserializer as DeserializeParam<T>>::EXPECTING;

    fn describe(&self, description: &mut TypeDescription) {
        T::DE.describe(description);
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        T::DE.deserialize_param(ctx, param)
    }

    fn serialize_param(&self, param: &T) -> serde_json::Value {
        T::DE.serialize_param(param)
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

impl<T: Serialize + DeserializeOwned, const EXPECTING: u8> DeserializeParam<T>
    for Serde<EXPECTING>
{
    const EXPECTING: BasicTypes = BasicTypes::from_raw(EXPECTING);

    fn describe(&self, _description: &mut TypeDescription) {
        // Do nothing
    }

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

    fn serialize_param(&self, param: &T) -> serde_json::Value {
        serde_json::to_value(param).expect("failed serializing to JSON")
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

impl WellKnown for IpAddr {
    type Deserializer = Qualified<super::Serde![str]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![str], "IP address");
}

impl WellKnown for Ipv4Addr {
    type Deserializer = Qualified<super::Serde![str]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![str], "IPv4 address");
}

impl WellKnown for Ipv6Addr {
    type Deserializer = Qualified<super::Serde![str]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![str], "IPv6 address");
}

impl WellKnown for SocketAddr {
    type Deserializer = Qualified<super::Serde![str]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![str], "socket address");
}

impl WellKnown for SocketAddrV4 {
    type Deserializer = Qualified<super::Serde![str]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![str], "v4 socket address");
}

impl WellKnown for SocketAddrV6 {
    type Deserializer = Qualified<super::Serde![str]>;
    const DE: Self::Deserializer = Qualified::new(super::Serde![str], "v6 socket address");
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

macro_rules! impl_well_known_non_zero_int {
    ($($int:ty),+) => {
        $(
        impl WellKnown for $int {
            type Deserializer = Qualified<super::Serde![int]>;
            const DE: Self::Deserializer = Qualified::new(super::Serde![int], "non-zero");
        }
        )+
    };
}

impl_well_known_non_zero_int!(
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

/// [Deserializer](DeserializeParam) decorator that provides additional [details](TypeDescription)
/// for the deserialized type.
#[derive(Debug)]
pub struct Qualified<De> {
    inner: De,
    // Cannot use `TypeDescription` directly because it wouldn't allow to drop the type in const contexts.
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

    fn describe(&self, description: &mut TypeDescription) {
        description.set_details(self.description);
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        self.inner.deserialize_param(ctx, param)
    }

    fn serialize_param(&self, param: &T) -> serde_json::Value {
        self.inner.serialize_param(param)
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

    fn describe(&self, description: &mut TypeDescription) {
        self.inner.describe(description);
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

    fn serialize_param(&self, param: &T) -> serde_json::Value {
        self.inner.serialize_param(param)
    }
}

/// Deserializer decorator that wraps the output of the underlying decorator in `Some` and returns `None`
/// if the input for the param is missing.
///
/// # Encoding nulls
///
/// For env variables, specifying null values can be tricky since natively, all env variable values are strings.
/// There are the following was to avoid this issue:
///
/// - [JSON coercion](crate::Environment::coerce_json()) can be used to pass unambiguous JSON values (incl. `null`).
/// - If the original deserializer doesn't expect string values, an empty string or `"null"` will be coerced
///   to a null.
/// - [`filter` attribute](macro@crate::DescribeConfig#filter) can help filtering out empty strings etc. for types
///   that do expect string values.
#[derive(Debug)]
pub struct Optional<De>(pub De);

impl<T, De: DeserializeParam<T>> DeserializeParam<Option<T>> for Optional<De> {
    const EXPECTING: BasicTypes = De::EXPECTING;

    fn describe(&self, description: &mut TypeDescription) {
        self.0.describe(description);
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Option<T>, ErrorWithOrigin> {
        let current_value = ctx.current_value().map(|val| &val.inner);
        let Some(current_value) = current_value else {
            return Ok(None);
        };
        if matches!(current_value, Value::Null) {
            return Ok(None);
        }

        // Coerce string values representing `null`, provided that the original deserializer doesn't expect a string
        // (if it does, there would be an ambiguity doing this).
        if !De::EXPECTING.contains(BasicTypes::STRING) {
            if let Some(s) = current_value.as_plain_str() {
                if s.is_empty() || s == "null" {
                    return Ok(None);
                }
            }
        }

        self.0.deserialize_param(ctx, param).map(Some)
    }

    fn serialize_param(&self, param: &Option<T>) -> serde_json::Value {
        if let Some(param) = param {
            self.0.serialize_param(param)
        } else {
            serde_json::Value::Null
        }
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
/// # use serde::{Deserialize, Serialize};
/// use smart_config::{de, testing, DescribeConfig, DeserializeConfig};
///
/// #[derive(Debug, Serialize, Deserialize)]
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

    fn describe(&self, description: &mut TypeDescription) {
        self.0.describe(description);
    }

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

        T::from_str(s.expose()).map_err(|err| {
            let err = serde_json::Error::custom(err);
            ErrorWithOrigin::json(err, origin.clone())
        })
    }

    fn serialize_param(&self, param: &T) -> serde_json::Value {
        self.0.serialize_param(param)
    }
}
