//! `serde`-compatible deserializer based on a value with origin.

use std::{collections::HashMap, iter::empty, sync::Arc};

use serde::{
    de::{
        self,
        value::{MapDeserializer, SeqDeserializer},
        DeserializeSeed, Error as DeError, IntoDeserializer,
    },
    Deserialize,
};

use crate::{
    error::ErrorWithOrigin,
    value::{Value, ValueOrigin, WithOrigin},
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
                            let err = DeError::custom(format_args!("{err} while parsing {} value '{s}'", stringify!($ty)));
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
    array: &[WithOrigin],
    visitor: V,
) -> Result<V::Value, ErrorWithOrigin> {
    let mut deserializer = SeqDeserializer::new(array.iter().map(ValueDeserializer::new));
    let seq = visitor.visit_seq(&mut deserializer)?;
    deserializer.end()?;
    Ok(seq)
}

fn parse_object<'de, V: de::Visitor<'de>>(
    object: &HashMap<String, WithOrigin>,
    visitor: V,
) -> Result<V::Value, ErrorWithOrigin> {
    let mut deserializer = MapDeserializer::new(
        object
            .iter()
            .map(|(key, value)| (key.as_str(), ValueDeserializer::new(value))),
    );
    let map = visitor.visit_map(&mut deserializer)?;
    deserializer.end()?;
    Ok(map)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ValueDeserializer<'a> {
    value: &'a WithOrigin,
    // TODO: options, e.g. mapping enum variants?
}

impl<'a> ValueDeserializer<'a> {
    pub(crate) fn new(value: &'a WithOrigin) -> Self {
        Self { value }
    }

    fn value(&self) -> &'a Value {
        &self.value.inner
    }

    fn enrich_err(&self, err: serde_json::Error) -> ErrorWithOrigin {
        ErrorWithOrigin::new(err, self.value.origin.clone())
    }

    #[cold]
    fn invalid_type(&self, expected: &str) -> ErrorWithOrigin {
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
        DeError::invalid_type(actual, &expected)
    }
}

impl<'de> de::Deserializer<'de> for ValueDeserializer<'_> {
    type Error = ErrorWithOrigin;

    fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value() {
            Value::Null => visitor.visit_none(),
            Value::Bool(value) => visitor.visit_bool(*value),
            Value::Number(value) => value
                .deserialize_any(visitor)
                .map_err(|err| self.enrich_err(err)),
            Value::String(value) => visitor.visit_str(value),
            Value::Array(array) => parse_array(array, visitor),
            Value::Object(object) => parse_object(object, visitor),
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
        match self.value() {
            Value::String(s) => {
                if s.is_empty() {
                    SeqDeserializer::new(empty::<Self>()).deserialize_seq(visitor)
                } else {
                    let origin = &self.value.origin;
                    let items = s.split(',').map(|item| WithOrigin {
                        inner: Value::String(item.to_owned()),
                        origin: origin.clone(),
                    });
                    let items: Vec<_> = items.collect();
                    let items = items.iter().map(ValueDeserializer::new);
                    SeqDeserializer::new(items).deserialize_seq(visitor)
                }
            }
            Value::Array(array) => parse_array(array, visitor),
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
            Value::Object(object) => parse_object(object, visitor),
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
            Value::Array(array) => parse_array(array, visitor),
            Value::Object(object) => parse_object(object, visitor),
            _ => Err(self.invalid_type("array or object")),
        }
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let (variant, value) = match self.value() {
            Value::Object(object) if object.len() == 1 => {
                let (variant, value) = object.iter().next().unwrap();
                (variant, Some(value))
            }
            Value::String(s) => (s, None),
            _ => return Err(self.invalid_type("string or object with single key")),
        };
        visitor.visit_enum(EnumDeserializer {
            variant,
            inner: VariantDeserializer {
                value,
                parent_origin: self.value.origin.clone(),
            },
        })
    }

    // Primitive values

    fn deserialize_bool<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value() {
            Value::Bool(value) => visitor.visit_bool(*value),
            Value::String(s) => match s.parse::<bool>() {
                Ok(val) => visitor.visit_bool(val),
                Err(err) => {
                    let err =
                        DeError::custom(format_args!("{err} while parsing value '{s}' as boolean"));
                    Err(self.enrich_err(err))
                }
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
            Value::Array(array) => parse_array(array, visitor),
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

impl<'de> IntoDeserializer<'de, ErrorWithOrigin> for ValueDeserializer<'_> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

#[derive(Debug)]
struct EnumDeserializer<'a> {
    variant: &'a str,
    inner: VariantDeserializer<'a>,
}

impl<'a, 'de> de::EnumAccess<'de> for EnumDeserializer<'a> {
    type Error = ErrorWithOrigin;
    type Variant = VariantDeserializer<'a>;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Self::Error> {
        let variant = self.variant.into_deserializer();
        seed.deserialize(variant).map(|v| (v, self.inner))
    }
}

#[derive(Debug)]
struct VariantDeserializer<'a> {
    value: Option<&'a WithOrigin>,
    parent_origin: Arc<ValueOrigin>,
}

impl<'de> de::VariantAccess<'de> for VariantDeserializer<'_> {
    type Error = ErrorWithOrigin;

    fn unit_variant(self) -> Result<(), Self::Error> {
        if let Some(value) = self.value {
            Deserialize::deserialize(ValueDeserializer::new(value))
        } else {
            Ok(())
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if let Some(value) = self.value {
            seed.deserialize(ValueDeserializer::new(value))
        } else {
            let err = DeError::invalid_type(de::Unexpected::Unit, &"newtype variant");
            Err(ErrorWithOrigin::new(err, self.parent_origin))
        }
    }

    fn tuple_variant<V: de::Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        if let Some(value) = self.value {
            de::Deserializer::deserialize_seq(ValueDeserializer::new(value), visitor)
        } else {
            let err = DeError::invalid_type(de::Unexpected::Unit, &"tuple variant");
            Err(ErrorWithOrigin::new(err, self.parent_origin))
        }
    }

    fn struct_variant<V: de::Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        if let Some(value) = self.value {
            de::Deserializer::deserialize_map(ValueDeserializer::new(value), visitor)
        } else {
            let err = DeError::invalid_type(de::Unexpected::Unit, &"struct variant");
            Err(ErrorWithOrigin::new(err, self.parent_origin))
        }
    }
}
