//! `serde`-compatible deserializer based on a value with origin.

use std::sync::Arc;

use serde::{
    Deserialize, Deserializer,
    de::{
        self, DeserializeSeed, Error as DeError, IntoDeserializer,
        value::{MapDeserializer, SeqDeserializer},
    },
};

use crate::{
    error::ErrorWithOrigin,
    utils::EnumVariant,
    value::{Map, StrValue, Value, ValueOrigin, WithOrigin},
};

/// Available deserialization options.
#[derive(Debug, Clone, Default)]
pub struct DeserializerOptions {
    /// Enables coercion of variant names between cases, e.g. from `SHOUTING_CASE` to `shouting_case`.
    pub coerce_variant_names: bool,
}

impl WithOrigin {
    #[cold]
    pub(crate) fn invalid_type(&self, expected: &str) -> ErrorWithOrigin {
        let actual = match &self.inner {
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
            Value::String(StrValue::Plain(s)) => de::Unexpected::Str(s),
            Value::String(StrValue::Secret(_)) => de::Unexpected::Other("secret"),
            Value::Array(_) => de::Unexpected::Seq,
            Value::Object(_) => de::Unexpected::Map,
        };
        ErrorWithOrigin::json(
            DeError::invalid_type(actual, &expected),
            self.origin.clone(),
        )
    }
}

macro_rules! parse_int_value {
    ($($ty:ident => $method:ident,)*) => {
        $(
        fn $method<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
            let result = match self.value() {
                Value::String(s) => {
                    match s.expose().parse::<$ty>() {
                        Ok(val) => val.into_deserializer().$method(visitor),
                        Err(err) => {
                            let err = DeError::custom(format_args!("{err} while parsing {} value '{s}'", stringify!($ty)));
                            return Err(self.enrich_err(err));
                        }
                    }
                }
                Value::Number(number) => number.deserialize_any(visitor).map_err(|err| self.enrich_err(err.into())),
                _ => return Err(self.invalid_type(&format!("{} number", stringify!($ty)))),
            };
            result.map_err(|err| err.set_origin_if_unset(&self.value.origin))
        }
        )*
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ValueDeserializer<'a> {
    value: &'a WithOrigin,
    options: &'a DeserializerOptions,
}

impl<'a> ValueDeserializer<'a> {
    pub(super) fn new(value: &'a WithOrigin, options: &'a DeserializerOptions) -> Self {
        Self { value, options }
    }

    pub(super) fn value(&self) -> &'a Value {
        &self.value.inner
    }

    pub(super) fn origin(&self) -> &Arc<ValueOrigin> {
        &self.value.origin
    }

    pub(super) fn enrich_err(&self, err: serde_json::Error) -> ErrorWithOrigin {
        ErrorWithOrigin::json(err, self.value.origin.clone())
    }

    pub(super) fn invalid_type(&self, expected: &str) -> ErrorWithOrigin {
        self.value.invalid_type(expected)
    }

    fn parse_array<'de, V: de::Visitor<'de>>(
        &self,
        array: &[WithOrigin],
        visitor: V,
    ) -> Result<V::Value, ErrorWithOrigin> {
        let mut deserializer = SeqDeserializer::new(
            array
                .iter()
                .map(|val| ValueDeserializer::new(val, self.options)),
        );
        let seq = visitor.visit_seq(&mut deserializer)?;
        deserializer.end()?;
        Ok(seq)
    }

    fn parse_object<'de, V: de::Visitor<'de>>(
        &self,
        object: &Map,
        visitor: V,
    ) -> Result<V::Value, ErrorWithOrigin> {
        let mut deserializer = MapDeserializer::new(
            object
                .iter()
                .map(|(key, value)| (key.as_str(), ValueDeserializer::new(value, self.options))),
        );
        let map = visitor.visit_map(&mut deserializer)?;
        deserializer.end()?;
        Ok(map)
    }
}

impl<'de> Deserializer<'de> for ValueDeserializer<'_> {
    type Error = ErrorWithOrigin;

    fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let result = match self.value() {
            Value::Null => visitor.visit_none(),
            Value::Bool(value) => visitor.visit_bool(*value),
            Value::Number(value) => value
                .deserialize_any(visitor)
                .map_err(|err| self.enrich_err(err)),
            Value::String(value) => visitor.visit_str(value.expose()),
            Value::Array(array) => self.parse_array(array, visitor),
            Value::Object(object) => self.parse_object(object, visitor),
        };
        result.map_err(|err| err.set_origin_if_unset(&self.value.origin))
    }

    fn deserialize_option<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let result = match self.value() {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        };
        result.map_err(|err| err.set_origin_if_unset(&self.value.origin))
    }

    fn deserialize_newtype_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor
            .visit_newtype_struct(self)
            .map_err(|err| err.set_origin_if_unset(&self.value.origin))
    }

    fn deserialize_seq<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let result = match self.value() {
            Value::Array(array) => self.parse_array(array, visitor),
            _ => Err(self.invalid_type("array")),
        };
        result.map_err(|err| err.set_origin_if_unset(&self.value.origin))
    }

    fn deserialize_tuple<V: de::Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
            .map_err(|err| err.set_origin_if_unset(&self.value.origin))
    }

    fn deserialize_tuple_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
            .map_err(|err| err.set_origin_if_unset(&self.value.origin))
    }

    fn deserialize_map<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let result = match self.value() {
            Value::Object(object) => self.parse_object(object, visitor),
            _ => Err(self.invalid_type("object")),
        };
        result.map_err(|err| err.set_origin_if_unset(&self.value.origin))
    }

    fn deserialize_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let result = match self.value() {
            Value::Array(array) => self.parse_array(array, visitor),
            Value::Object(object) => self.parse_object(object, visitor),
            _ => Err(self.invalid_type("array or object")),
        };
        result.map_err(|err| err.set_origin_if_unset(&self.value.origin))
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let (mut variant, value) = match self.value() {
            Value::Object(object) if object.len() == 1 => {
                let (variant, value) = object.iter().next().unwrap();
                (variant.as_str(), Some(value))
            }
            Value::String(s) => (s.expose(), None),
            _ => return Err(self.invalid_type("string or object with single key")),
        };

        if self.options.coerce_variant_names
            && let Some(parsed) = EnumVariant::new(variant)
            && let Some(expected_variant) = parsed.try_match(variants)
        {
            variant = expected_variant;
        }

        visitor
            .visit_enum(EnumDeserializer {
                variant,
                inner: VariantDeserializer {
                    value,
                    options: self.options,
                    parent_origin: self.value.origin.clone(),
                },
            })
            .map_err(|err| err.set_origin_if_unset(&self.value.origin))
    }

    // Primitive values

    fn deserialize_bool<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let result = match self.value() {
            Value::Bool(value) => visitor.visit_bool(*value),
            Value::String(s) => match s.expose().parse::<bool>() {
                Ok(val) => visitor.visit_bool(val),
                Err(err) => {
                    let err =
                        DeError::custom(format_args!("{err} while parsing value '{s}' as boolean"));
                    return Err(self.enrich_err(err));
                }
            },
            _ => return Err(self.invalid_type("boolean or boolean-like string")),
        };
        result.map_err(|err: serde_json::Error| self.enrich_err(err))
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
        u128 => deserialize_u128,
        i128 => deserialize_i128,
        f32 => deserialize_f32,
        f64 => deserialize_f64,
    }

    fn deserialize_string<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let result = match self.value() {
            Value::String(s) => visitor.visit_str(s.expose()),
            Value::Null => visitor.visit_string("null".to_string()),
            Value::Bool(value) => visitor.visit_string(value.to_string()),
            Value::Number(value) => visitor.visit_string(value.to_string()),
            _ => Err(self.invalid_type("string or other primitive type")),
        };
        result.map_err(|err| err.set_origin_if_unset(&self.value.origin))
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
        let result = match self.value() {
            Value::String(s) => visitor.visit_str(s.expose()),
            Value::Array(array) => self.parse_array(array, visitor),
            _ => return Err(self.invalid_type("string or array")),
        };
        result.map_err(|err| err.set_origin_if_unset(&self.value.origin))
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
        let result = match self.value() {
            Value::Null => visitor.visit_unit(),
            _ => Err(self.invalid_type("null")),
        };
        result.map_err(|err| err.set_origin_if_unset(&self.value.origin))
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
        visitor
            .visit_unit()
            .map_err(|err: serde_json::Error| self.enrich_err(err))
    }
}

impl IntoDeserializer<'_, ErrorWithOrigin> for ValueDeserializer<'_> {
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
        match seed.deserialize(variant) {
            Ok(val) => Ok((val, self.inner)),
            Err(err) => Err(ErrorWithOrigin::json(err, self.inner.origin().clone())),
        }
    }
}

#[derive(Debug)]
struct VariantDeserializer<'a> {
    value: Option<&'a WithOrigin>,
    options: &'a DeserializerOptions,
    parent_origin: Arc<ValueOrigin>,
}

impl VariantDeserializer<'_> {
    fn origin(&self) -> &Arc<ValueOrigin> {
        self.value.map_or(&self.parent_origin, |val| &val.origin)
    }
}

impl<'de> de::VariantAccess<'de> for VariantDeserializer<'_> {
    type Error = ErrorWithOrigin;

    fn unit_variant(self) -> Result<(), Self::Error> {
        if let Some(value) = self.value {
            Deserialize::deserialize(ValueDeserializer::new(value, self.options))
        } else {
            Ok(())
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if let Some(value) = self.value {
            seed.deserialize(ValueDeserializer::new(value, self.options))
        } else {
            let err = DeError::invalid_type(de::Unexpected::Unit, &"newtype variant");
            Err(ErrorWithOrigin::json(err, self.parent_origin))
        }
    }

    fn tuple_variant<V: de::Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        if let Some(value) = self.value {
            de::Deserializer::deserialize_seq(ValueDeserializer::new(value, self.options), visitor)
        } else {
            let err = DeError::invalid_type(de::Unexpected::Unit, &"tuple variant");
            Err(ErrorWithOrigin::json(err, self.parent_origin))
        }
    }

    fn struct_variant<V: de::Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        if let Some(value) = self.value {
            de::Deserializer::deserialize_map(ValueDeserializer::new(value, self.options), visitor)
        } else {
            let err = DeError::invalid_type(de::Unexpected::Unit, &"struct variant");
            Err(ErrorWithOrigin::json(err, self.parent_origin))
        }
    }
}
