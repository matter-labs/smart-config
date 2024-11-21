//! Schema-guided parsing of configurations.

use std::{collections::HashMap, iter::empty, sync::Arc};

use serde::{
    de::{
        self,
        value::{MapDeserializer, SeqDeserializer},
        DeserializeOwned, DeserializeSeed, Error as DeError, IntoDeserializer,
    },
    Deserialize,
};

pub use crate::error::ParseError;
use crate::{
    error::LocationInConfig,
    metadata::ConfigMetadata,
    value::{Pointer, Value, ValueOrigin, WithOrigin},
    DescribeConfig, DeserializeConfig,
};

macro_rules! parse_int_value {
    ($($ty:ident => $method:ident,)*) => {
        $(
        fn $method<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
            match self.value() {
                Value::String(s) => {
                    match s.parse::<$ty>() {
                        Ok(val) => val.into_deserializer().$method(visitor),
                        Err(err) => {
                            let err = ParseError::custom(format_args!("{err} while parsing {} value '{s}'", stringify!($ty)));
                            Err(self.enrich_err(err))
                        }
                    }
                }
                Value::Number(number) => number.deserialize_any(visitor).map_err(|err| self.enrich_err(err.into())),
                _ => Err(self.invalid_type(&format!("{} number", stringify!($ty))))
            }
        }
        )*
    }
}

fn parse_array<'de, V: de::Visitor<'de>>(
    parent_path: Pointer<'_>,
    array: &[WithOrigin],
    visitor: V,
    origin: Option<&Arc<ValueOrigin>>,
) -> Result<V::Value, ParseError> {
    let mut deserializer = SeqDeserializer::new(array.iter().enumerate().map(|(i, value)| {
        let path = parent_path.join(&i.to_string());
        ValueDeserializer::new(value, path)
    }));
    let seq = visitor
        .visit_seq(&mut deserializer)
        .map_err(|err| err.with_origin(origin))?;
    deserializer.end().map_err(|err| err.with_origin(origin))?;
    Ok(seq)
}

fn parse_object<'de, V: de::Visitor<'de>>(
    parent_path: Pointer<'_>,
    object: &HashMap<String, WithOrigin>,
    visitor: V,
    origin: Option<&Arc<ValueOrigin>>,
) -> Result<V::Value, ParseError> {
    let mut deserializer = MapDeserializer::new(object.iter().map(|(key, value)| {
        let path = parent_path.join(key);
        (key.as_str(), ValueDeserializer::new(value, path))
    }));
    let map = visitor
        .visit_map(&mut deserializer)
        .map_err(|err| err.with_origin(origin))?;
    deserializer.end().map_err(|err| err.with_origin(origin))?;
    Ok(map)
}

#[derive(Debug, Clone)]
pub struct ValueDeserializer<'a> {
    value: Option<&'a WithOrigin>,
    /// Absolute path that `value` corresponds to.
    path: String,
    /// Metadata of the config currently being deserialized. Set by derived `DeserializeConfig` impls.
    config: Option<&'static ConfigMetadata>,
    // TODO: options, e.g. mapping enum variants?
}

impl<'a> ValueDeserializer<'a> {
    pub(crate) fn new(value: &'a WithOrigin, path: String) -> Self {
        Self {
            value: Some(value),
            path,
            config: None,
        }
    }

    pub(crate) fn missing(path: String) -> Self {
        Self {
            value: None,
            path,
            config: None,
        }
    }

    fn value(&self) -> &'a Value {
        self.value.map_or(&Value::Null, |val| &val.inner)
    }

    fn path(&self) -> Pointer<'_> {
        Pointer(&self.path)
    }

    fn origin(&self) -> Option<&Arc<ValueOrigin>> {
        self.value.map(|val| &val.origin)
    }

    #[cold]
    fn invalid_type(&self, expected: &str) -> ParseError {
        let actual = match self.value() {
            Value::Null => de::Unexpected::Unit,
            Value::Bool(value) => de::Unexpected::Bool(*value),
            Value::Number(value) => {
                if let Some(value) = value.as_u64() {
                    de::Unexpected::Unsigned(value)
                } else if let Some(value) = value.as_i64() {
                    de::Unexpected::Signed(value)
                } else if let Some(value) = value.as_f64() {
                    de::Unexpected::Float(value)
                } else {
                    de::Unexpected::Other("number")
                }
            }
            Value::String(s) => de::Unexpected::Str(s),
            Value::Array(_) => de::Unexpected::Seq,
            Value::Object(_) => de::Unexpected::Map,
        };
        self.enrich_err(ParseError::invalid_type(actual, &expected))
    }

    #[cold]
    fn enrich_err(&self, err: ParseError) -> ParseError {
        err.with_origin(self.origin())
            .with_path(&self.path)
            .for_config(self.config)
    }

    /// Gets a deserializer for a child at the specified path.
    fn child_deserializer(&self, child_path: &str) -> Self {
        let path = Pointer(&self.path).join(child_path);
        let child = if let Value::Object(object) = self.value() {
            object.get(child_path)
        } else {
            None
        };
        Self {
            value: child,
            path,
            config: None,
        }
    }
}

/// Methods used in proc macros. Not a part of public API.
#[doc(hidden)]
impl<'a> ValueDeserializer<'a> {
    pub fn for_config<T: DescribeConfig>(self) -> Self {
        Self {
            config: Some(T::describe_config()),
            ..self
        }
    }

    pub fn deserialize_param<T: DeserializeOwned>(
        &self,
        index: usize,
        path: &'static str,
        default_fn: Option<fn() -> T>,
    ) -> Result<T, ParseError> {
        self.deserialize_param_inner(index, path, default_fn)
    }

    fn deserialize_param_inner<T: DeserializeOwned>(
        &self,
        index: usize,
        path: &'static str,
        default_fn: Option<impl FnOnce() -> T>,
    ) -> Result<T, ParseError> {
        let location = LocationInConfig::Param(index);
        let child_deserializer = self.child_deserializer(path);
        if child_deserializer.value.is_none() {
            return if let Some(default_fn) = default_fn {
                Ok(default_fn())
            } else {
                let err = DeError::missing_field(path);
                Err(self.enrich_err(err).with_location(self.config, location))
            };
        }

        T::deserialize(child_deserializer)
            .map_err(|err| self.enrich_err(err).with_location(self.config, location))
    }

    pub fn deserialize_tag(
        &self,
        index: usize,
        path: &'static str,
        expected: &'static [&'static str],
        default: Option<&'static str>,
    ) -> Result<&'a str, ParseError> {
        let tag_value: String = self.deserialize_param_inner(
            index,
            path,
            default.map(|default| || default.to_owned()),
        )?;
        let matching_tag = expected
            .iter()
            .copied()
            .find(|&variant| variant == tag_value);
        matching_tag.ok_or_else(|| {
            self.enrich_err(DeError::unknown_variant(&tag_value, expected))
                .with_location(self.config, LocationInConfig::Param(index))
        })
    }

    pub fn deserialize_nested_config<T: DeserializeConfig>(
        &self,
        index: usize,
        path: &'static str,
        default_fn: Option<fn() -> T>,
    ) -> Result<T, ParseError> {
        let location = LocationInConfig::Nested(index);
        let child_deserializer = self.child_deserializer(path);

        let child_value = if let Some(value) = child_deserializer.value {
            value
        } else if let Some(default_fn) = default_fn {
            return Ok(default_fn());
        } else {
            let err = DeError::missing_field(path);
            return Err(self.enrich_err(err).with_location(self.config, location));
        };
        if !matches!(&child_value.inner, Value::Object(_)) {
            return Err(self
                .invalid_type("configuration object")
                .with_location(self.config, location));
        }
        T::deserialize_config(child_deserializer)
    }

    pub fn for_flattened_config(&self) -> Self {
        self.clone()
    }
}

impl<'de> de::Deserializer<'de> for ValueDeserializer<'_> {
    type Error = ParseError;

    fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value() {
            Value::Null => visitor.visit_none(),
            Value::Bool(value) => visitor.visit_bool(*value),
            Value::Number(value) => value
                .deserialize_any(visitor)
                .map_err(|err| self.enrich_err(err.into())),
            Value::String(value) => visitor.visit_str(value),
            Value::Array(array) => parse_array(self.path(), array, visitor, self.origin()),
            Value::Object(object) => parse_object(self.path(), object, visitor, self.origin()),
        }
    }

    fn deserialize_option<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value() {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_newtype_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let parent_path = self.path();
        match self.value() {
            Value::String(s) => {
                if s.is_empty() {
                    SeqDeserializer::new(empty::<Self>()).deserialize_seq(visitor)
                } else {
                    let items = s.split(',').map(|item| WithOrigin {
                        inner: Value::String(item.to_owned()),
                        origin: self.origin().cloned().unwrap_or_default(),
                    });
                    let items: Vec<_> = items.collect();
                    let items = items.iter().enumerate().map(|(i, value)| {
                        let path = parent_path.join(&i.to_string());
                        ValueDeserializer::new(value, path)
                    });
                    SeqDeserializer::new(items).deserialize_seq(visitor)
                }
            }
            Value::Array(array) => parse_array(parent_path, array, visitor, self.origin()),
            _ => Err(self.invalid_type("array or comma-separated string")),
        }
    }

    fn deserialize_tuple<V: de::Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value() {
            Value::Object(object) => parse_object(self.path(), object, visitor, self.origin()),
            _ => Err(self.invalid_type("object")),
        }
    }

    fn deserialize_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value() {
            Value::Array(array) => parse_array(self.path(), array, visitor, self.origin()),
            Value::Object(object) => parse_object(self.path(), object, visitor, self.origin()),
            _ => Err(self.invalid_type("array or object")),
        }
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let (variant, value, path) = match self.value() {
            Value::Object(object) if object.len() == 1 => {
                let (variant, value) = object.iter().next().unwrap();
                (variant, Some(value), self.path().join(variant))
            }
            Value::String(s) => (s, None, self.path),
            _ => return Err(self.invalid_type("string or object with single key")),
        };
        visitor.visit_enum(EnumDeserializer {
            variant,
            inner: Self {
                value,
                path,
                config: None,
            },
        })
    }

    // Primitive values

    fn deserialize_bool<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value() {
            Value::Bool(value) => visitor.visit_bool(*value),
            Value::String(s) => match s.parse::<bool>() {
                Ok(val) => visitor.visit_bool(val),
                Err(err) => Err(ParseError::custom(format_args!(
                    "{err} while parsing value '{s}' as boolean"
                ))
                .with_origin(self.origin())
                .with_path(&self.path)),
            },
            _ => Err(self.invalid_type("boolean or boolean-like string")),
        }
    }

    parse_int_value! {
        u8 => deserialize_u8,
        u16 => deserialize_u16,
        u32 => deserialize_u32,
        u64 => deserialize_u64,
        i8 => deserialize_i8,
        i16 => deserialize_i16,
        i32 => deserialize_i32,
        i64 => deserialize_i64,
        f32 => deserialize_f32,
        f64 => deserialize_f64,
    }

    fn deserialize_string<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value() {
            Value::String(s) => visitor.visit_str(s),
            Value::Null => visitor.visit_string("null".to_string()),
            Value::Bool(value) => visitor.visit_string(value.to_string()),
            Value::Number(value) => visitor.visit_string(value.to_string()),
            _ => Err(self.invalid_type("string or other primitive type")),
        }
    }

    fn deserialize_char<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_string(visitor)
    }

    fn deserialize_str<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_string(visitor)
    }

    fn deserialize_byte_buf<V: de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value() {
            Value::String(s) => visitor.visit_str(s),
            Value::Array(array) => parse_array(self.path(), array, visitor, self.origin()),
            _ => Err(self.invalid_type("string or array")),
        }
    }

    fn deserialize_bytes<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_byte_buf(visitor)
    }

    fn deserialize_identifier<V: de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_string(visitor)
    }

    fn deserialize_unit<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value() {
            Value::Null => visitor.visit_unit(),
            _ => Err(self.invalid_type("null")),
        }
    }

    fn deserialize_unit_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_ignored_any<V: de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_unit()
    }
}

impl<'de> IntoDeserializer<'de, ParseError> for ValueDeserializer<'_> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

#[derive(Debug)]
struct EnumDeserializer<'a> {
    variant: &'a str,
    inner: ValueDeserializer<'a>,
}

impl<'a, 'de> de::EnumAccess<'de> for EnumDeserializer<'a> {
    type Error = ParseError;
    type Variant = ValueDeserializer<'a>;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Self::Error> {
        let variant = self.variant.into_deserializer();
        seed.deserialize(variant).map(|v| (v, self.inner))
    }
}

impl<'de> de::VariantAccess<'de> for ValueDeserializer<'_> {
    type Error = ParseError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        if self.value.is_some() {
            Deserialize::deserialize(self)
        } else {
            Ok(())
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if self.value.is_some() {
            seed.deserialize(self)
        } else {
            Err(self.invalid_type("newtype variant"))
        }
    }

    fn tuple_variant<V: de::Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        if self.value.is_some() {
            de::Deserializer::deserialize_seq(self, visitor)
        } else {
            Err(self.invalid_type("tuple variant"))
        }
    }

    fn struct_variant<V: de::Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        if self.value.is_some() {
            de::Deserializer::deserialize_map(self, visitor)
        } else {
            Err(self.invalid_type("struct variant"))
        }
    }
}
