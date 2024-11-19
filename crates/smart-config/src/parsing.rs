//! Schema-guided parsing of configurations.

use std::{collections::HashMap, fmt, iter::empty, sync::Arc};

use serde::de::{
    self,
    value::{MapDeserializer, SeqDeserializer},
    DeserializeSeed, Error as DeError, IntoDeserializer,
};

use crate::value::{Value, ValueOrigin, ValueWithOrigin};

#[derive(Debug)]
pub(crate) struct ParseError {
    pub inner: serde_json::Error,
    pub origin: Option<Arc<ValueOrigin>>,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(origin) = &self.origin {
            write!(
                formatter,
                "error parsing configuration value from {origin}: {}",
                self.inner
            )
        } else {
            fmt::Display::fmt(&self.inner, formatter)
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
    }
}

impl DeError for ParseError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self {
            inner: DeError::custom(msg),
            origin: None,
        }
    }
}

impl ParseError {
    fn with_origin(mut self, origin: &Arc<ValueOrigin>) -> Self {
        if self.origin.is_none() {
            self.origin = Some(origin.clone());
        }
        self
    }
}

macro_rules! parse_int_value {
    ($($ty:ident => $method:ident,)*) => {
        $(
        fn $method<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
            let origin = &self.value.origin;
            match &self.value.inner {
                Value::String(s) => {
                    match s.parse::<$ty>() {
                        Ok(val) => val.into_deserializer().$method(visitor),
                        Err(err) => {
                            let err = ParseError::custom(format_args!("{err} while parsing {} value '{s}'", stringify!($ty)));
                            Err(err.with_origin(origin))
                        }
                    }
                }
                Value::Number(number) => number.deserialize_any(visitor).map_err(|err| ParseError {
                    inner: err,
                    origin: Some(origin.clone()),
                }),
                _ => Err(self.invalid_type(&format!("{} number", stringify!($ty))))
            }
        }
        )*
    }
}

fn parse_array<'de, V: de::Visitor<'de>>(
    array: &[ValueWithOrigin],
    visitor: V,
    origin: &Arc<ValueOrigin>,
) -> Result<V::Value, ParseError> {
    let mut deserializer =
        SeqDeserializer::new(array.iter().map(|value| ValueDeserializer { value }));
    let seq = visitor
        .visit_seq(&mut deserializer)
        .map_err(|err| err.with_origin(origin))?;
    deserializer.end().map_err(|err| err.with_origin(origin))?;
    Ok(seq)
}

fn parse_object<'de, V: de::Visitor<'de>>(
    object: &HashMap<String, ValueWithOrigin>,
    visitor: V,
    origin: &Arc<ValueOrigin>,
) -> Result<V::Value, ParseError> {
    let mut deserializer = MapDeserializer::new(
        object
            .iter()
            .map(|(key, value)| (key.as_str(), ValueDeserializer { value })),
    );
    let map = visitor
        .visit_map(&mut deserializer)
        .map_err(|err| err.with_origin(origin))?;
    deserializer.end().map_err(|err| err.with_origin(origin))?;
    Ok(map)
}

#[derive(Debug)]
pub(crate) struct ValueDeserializer<'a> {
    value: &'a ValueWithOrigin,
    // TODO: options, e.g. mapping variants?
}

impl<'a> ValueDeserializer<'a> {
    pub fn new(value: &'a ValueWithOrigin) -> Self {
        Self { value }
    }

    fn invalid_type(&self, expected: &str) -> ParseError {
        let actual = match &self.value.inner {
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
        ParseError::invalid_type(actual, &expected)
    }
}

impl<'de> de::Deserializer<'de> for ValueDeserializer<'_> {
    type Error = ParseError;

    fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let origin = &self.value.origin;
        match &self.value.inner {
            Value::Null => visitor.visit_none(),
            Value::Bool(value) => visitor.visit_bool(*value),
            Value::Number(value) => value.deserialize_any(visitor).map_err(|err| ParseError {
                inner: err,
                origin: Some(self.value.origin.clone()),
            }),
            Value::String(value) => visitor.visit_str(value),
            Value::Array(array) => parse_array(array, visitor, origin),
            Value::Object(object) => parse_object(object, visitor, origin),
        }
    }

    fn deserialize_option<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match &self.value.inner {
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
        let origin = &self.value.origin;
        match &self.value.inner {
            Value::String(s) => {
                if s.is_empty() {
                    SeqDeserializer::new(empty::<Self>()).deserialize_seq(visitor)
                } else {
                    let items = s.split(',').map(|item| ValueWithOrigin {
                        inner: Value::String(item.to_owned()),
                        origin: origin.clone(),
                    });
                    let items: Vec<_> = items.collect();
                    let items = items.iter().map(|value| ValueDeserializer { value });
                    SeqDeserializer::new(items).deserialize_seq(visitor)
                }
            }
            Value::Array(array) => parse_array(array, visitor, origin),
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
        match &self.value.inner {
            Value::Object(object) => parse_object(object, visitor, &self.value.origin),
            _ => Err(self.invalid_type("object")),
        }
    }

    fn deserialize_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let origin = &self.value.origin;
        match &self.value.inner {
            Value::Array(array) => parse_array(array, visitor, origin),
            Value::Object(object) => parse_object(object, visitor, origin),
            _ => Err(self.invalid_type("array or object")),
        }
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let (variant, value) = match &self.value.inner {
            Value::Object(object) if object.len() == 1 => {
                let (variant, value) = object.iter().next().unwrap();
                (variant, Some(value))
            }
            Value::String(s) => (s, None),
            _ => return Err(self.invalid_type("string or object with single key")),
        };
        visitor.visit_enum(EnumDeserializer { variant, value })
    }

    // Primitive values

    fn deserialize_bool<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match &self.value.inner {
            Value::Bool(value) => visitor.visit_bool(*value),
            Value::String(s) => match s.parse::<bool>() {
                Ok(val) => visitor.visit_bool(val),
                Err(err) => Err(de::Error::custom(format_args!(
                    "{err} while parsing value '{s}' at {:?}",
                    self.value.origin
                ))),
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
        match &self.value.inner {
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
        match &self.value.inner {
            Value::String(s) => visitor.visit_str(s),
            Value::Array(array) => parse_array(array, visitor, &self.value.origin),
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
        match self.value.inner {
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
    value: Option<&'a ValueWithOrigin>,
}

impl<'a, 'de> de::EnumAccess<'de> for EnumDeserializer<'a> {
    type Error = ParseError;
    type Variant = VariantDeserializer<'a>;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Self::Error> {
        let variant = self.variant.into_deserializer();
        let visitor = VariantDeserializer(self.value);
        seed.deserialize(variant).map(|v| (v, visitor))
    }
}

#[derive(Debug)]
struct VariantDeserializer<'a>(Option<&'a ValueWithOrigin>);

impl<'de> de::VariantAccess<'de> for VariantDeserializer<'_> {
    type Error = ParseError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.0 {
            Some(value) => de::Deserialize::deserialize(ValueDeserializer { value }),
            None => Ok(()),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.0 {
            Some(value) => seed.deserialize(ValueDeserializer { value }),
            None => Err(de::Error::invalid_type(
                de::Unexpected::UnitVariant,
                &"newtype variant",
            )),
        }
    }

    fn tuple_variant<V: de::Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.0 {
            Some(value) => de::Deserializer::deserialize_seq(ValueDeserializer { value }, visitor),
            None => Err(de::Error::invalid_type(
                de::Unexpected::UnitVariant,
                &"tuple variant",
            )),
        }
    }

    fn struct_variant<V: de::Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.0 {
            Some(value) => de::Deserializer::deserialize_map(ValueDeserializer { value }, visitor),
            None => Err(de::Error::invalid_type(
                de::Unexpected::UnitVariant,
                &"struct variant",
            )),
        }
    }
}
