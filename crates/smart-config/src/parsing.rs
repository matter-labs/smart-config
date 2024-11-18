//! Schema-guided parsing of configurations.

use std::{collections::HashMap, fmt, iter, iter::empty};

use anyhow::Context as _;
use serde::{
    de::{
        self,
        value::{MapDeserializer, SeqDeserializer},
        DeserializeSeed, Error as DeError, IntoDeserializer,
    },
    Deserialize,
};

use crate::{
    metadata::{ConfigMetadata, DescribeConfig},
    value::{Map, Value, ValueOrigin, ValueWithOrigin},
};

#[derive(Debug)]
pub struct Environment {
    map: ValueWithOrigin,
}

impl Environment {
    pub fn prefixed(prefix: &str, env: impl IntoIterator<Item = (String, String)>) -> Self {
        let map = env.into_iter().filter_map(|(name, value)| {
            let retained_name = name.strip_prefix(prefix)?.to_lowercase();
            Some((
                retained_name,
                ValueWithOrigin {
                    inner: Value::String(value),
                    origin: ValueOrigin::env_var(&name),
                },
            ))
        });
        let map = ValueWithOrigin {
            inner: Value::Object(map.collect()),
            origin: ValueOrigin("global configuration".into()),
        };
        Self { map }
    }

    pub fn parse<'de, C: DescribeConfig + Deserialize<'de>>(mut self) -> anyhow::Result<C> {
        self.map.inner.nest(C::describe_config())?;
        let original = self.map.clone();
        self.map
            .inner
            .merge_params(&original, C::describe_config())?;
        Ok(C::deserialize(self.map)?)
    }
}

impl Value {
    fn nest(&mut self, config: &ConfigMetadata) -> anyhow::Result<()> {
        let Self::Object(map) = self else {
            anyhow::bail!("expected object");
        };

        for nested in &*config.nested_configs {
            let name = nested.name;
            if name.is_empty() {
                continue;
            }

            if !map.contains_key(name) {
                let matching_keys = map.iter().filter_map(|(key, value)| {
                    if key.starts_with(name) && key.as_bytes().get(name.len()) == Some(&b'_') {
                        return Some((key[name.len() + 1..].to_owned(), value.clone()));
                    }
                    None
                });
                map.insert(
                    name.to_owned(),
                    ValueWithOrigin {
                        inner: Value::Object(matching_keys.collect()),
                        origin: ValueOrigin::group(nested),
                    },
                );
            }

            // `unwrap` is safe: the value has just been inserted
            let nested_value = &mut map.get_mut(name).unwrap().inner;
            nested_value
                .nest(nested.meta)
                .with_context(|| format!("nesting {}", nested.name))?;
        }
        Ok(())
    }

    fn merge_params(
        &mut self,
        original: &ValueWithOrigin,
        config: &ConfigMetadata,
    ) -> anyhow::Result<()> {
        let Self::Object(map) = self else {
            anyhow::bail!("expected object");
        };

        for param in &*config.params {
            if param.merge_from.is_empty() {
                continue; // Skip computations in the common case.
            }

            let all_param_names = iter::once(param.name).chain(param.aliases.iter().copied());
            let value_is_set = all_param_names.clone().any(|name| map.contains_key(name));
            if value_is_set {
                continue;
            }

            for &pointer in param.merge_from {
                for name in all_param_names.clone() {
                    if let Some(value) = original.pointer(pointer, name) {
                        map.insert(param.name.to_owned(), value.clone());
                        break;
                    }
                }
            }
        }

        // Recurse into nested configs
        for nested in &*config.nested_configs {
            let name = nested.name;
            let nested_value = if name.is_empty() {
                &mut *self
            } else {
                let Self::Object(map) = self else {
                    unreachable!()
                };
                if !map.contains_key(name) {
                    map.insert(
                        name.to_owned(),
                        ValueWithOrigin {
                            inner: Value::Object(Map::new()),
                            origin: ValueOrigin::group(nested),
                        },
                    );
                }
                &mut map.get_mut(name).unwrap().inner
            };
            nested_value
                .merge_params(original, nested.meta)
                .with_context(|| format!("merging params as {}", nested.name))?;
        }

        Ok(())
    }
}

impl ValueWithOrigin {
    fn invalid_type(&self, expected: &str) -> ParseError {
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
            Value::String(s) => de::Unexpected::Str(s),
            Value::Array(_) => de::Unexpected::Seq,
            Value::Object(_) => de::Unexpected::Map,
        };
        ParseError::invalid_type(actual, &expected)
    }
}

#[derive(Debug)]
pub struct ParseError {
    inner: serde_json::Error,
    origin: Option<ValueOrigin>,
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
    fn with_origin(mut self, origin: &ValueOrigin) -> Self {
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
            match &self.inner {
                Value::String(s) => {
                    match s.parse::<$ty>() {
                        Ok(val) => val.into_deserializer().$method(visitor),
                        Err(err) => {
                            let err = ParseError::custom(format_args!("{err} while parsing {} value '{s}'", stringify!($ty)));
                            Err(err.with_origin(&self.origin))
                        }
                    }
                }
                Value::Number(number) => number.deserialize_any(visitor).map_err(|err| ParseError {
                    inner: err,
                    origin: Some(self.origin.clone()),
                }),
                _ => Err(self.invalid_type(&format!("{} number", stringify!($ty))))
            }
        }
        )*
    }
}

fn parse_array<'de, V: de::Visitor<'de>>(
    array: Vec<ValueWithOrigin>,
    visitor: V,
    origin: &ValueOrigin,
) -> Result<V::Value, ParseError> {
    let mut deserializer = SeqDeserializer::new(array.into_iter());
    let seq = visitor
        .visit_seq(&mut deserializer)
        .map_err(|err| err.with_origin(origin))?;
    deserializer.end().map_err(|err| err.with_origin(origin))?;
    Ok(seq)
}

fn parse_object<'de, V: de::Visitor<'de>>(
    object: HashMap<String, ValueWithOrigin>,
    visitor: V,
    origin: &ValueOrigin,
) -> Result<V::Value, ParseError> {
    let mut deserializer = MapDeserializer::new(object.into_iter());
    let map = visitor
        .visit_map(&mut deserializer)
        .map_err(|err| err.with_origin(origin))?;
    deserializer.end().map_err(|err| err.with_origin(origin))?;
    Ok(map)
}

impl<'de> de::Deserializer<'de> for ValueWithOrigin {
    type Error = ParseError;

    fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.inner {
            Value::Null => visitor.visit_none(),
            Value::Bool(value) => visitor.visit_bool(value),
            Value::Number(value) => value.deserialize_any(visitor).map_err(|err| ParseError {
                inner: err,
                origin: Some(self.origin.clone()),
            }),
            Value::String(value) => visitor.visit_string(value),
            Value::Array(array) => parse_array(array, visitor, &self.origin),
            Value::Object(object) => parse_object(object, visitor, &self.origin),
        }
    }

    fn deserialize_option<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.inner {
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
        match self.inner {
            Value::String(s) => {
                if s.is_empty() {
                    SeqDeserializer::new(empty::<Self>()).deserialize_seq(visitor)
                } else {
                    let items = s.split(',').map(|item| Self {
                        inner: Value::String(item.to_owned()),
                        origin: self.origin.clone(),
                    });
                    SeqDeserializer::new(items).deserialize_seq(visitor)
                }
            }
            Value::Array(array) => parse_array(array, visitor, &self.origin),
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
        match self.inner {
            Value::Object(object) => parse_object(object, visitor, &self.origin),
            _ => Err(self.invalid_type("object")),
        }
    }

    fn deserialize_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.inner {
            Value::Array(array) => parse_array(array, visitor, &self.origin),
            Value::Object(object) => parse_object(object, visitor, &self.origin),
            _ => Err(self.invalid_type("array or object")),
        }
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let (variant, value) = match self.inner {
            Value::Object(object) if object.len() == 1 => {
                let (variant, value) = object.into_iter().next().unwrap();
                (variant, Some(value))
            }
            Value::String(s) => (s, None),
            _ => return Err(self.invalid_type("string or object with single key")),
        };
        visitor.visit_enum(EnumDeserializer { variant, value })
    }

    // Primitive values

    fn deserialize_bool<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.inner {
            Value::Bool(value) => visitor.visit_bool(value),
            Value::String(s) => match s.parse::<bool>() {
                Ok(val) => visitor.visit_bool(val),
                Err(err) => Err(de::Error::custom(format_args!(
                    "{err} while parsing value '{s}' at {:?}",
                    self.origin
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
        match self.inner {
            Value::String(s) => visitor.visit_string(s),
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
        match self.inner {
            Value::String(s) => visitor.visit_string(s),
            Value::Array(array) => parse_array(array, visitor, &self.origin),
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
        match self.inner {
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
        drop(self);
        visitor.visit_unit()
    }
}

impl<'de> IntoDeserializer<'de, ParseError> for ValueWithOrigin {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

#[derive(Debug)]
struct EnumDeserializer {
    variant: String,
    value: Option<ValueWithOrigin>,
}

impl<'de> de::EnumAccess<'de> for EnumDeserializer {
    type Error = ParseError;
    type Variant = VariantDeserializer;

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
struct VariantDeserializer(Option<ValueWithOrigin>);

impl<'de> de::VariantAccess<'de> for VariantDeserializer {
    type Error = ParseError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.0 {
            Some(value) => de::Deserialize::deserialize(value),
            None => Ok(()),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.0 {
            Some(value) => seed.deserialize(value),
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
            Some(value) => de::Deserializer::deserialize_seq(value, visitor),
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
            Some(value) => de::Deserializer::deserialize_map(value, visitor),
            None => Err(de::Error::invalid_type(
                de::Unexpected::UnitVariant,
                &"struct variant",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::metadata::EmptyConfig;

    #[derive(Debug, PartialEq, Eq, Hash, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum SimpleEnum {
        First,
        Second,
    }

    #[derive(Debug, Deserialize, DescribeConfig)]
    #[config(crate = crate)]
    struct NestedConfig {
        #[serde(rename = "renamed")]
        simple_enum: SimpleEnum,
        #[config(merge_from("/deprecated"))]
        #[serde(default = "NestedConfig::default_other_int")]
        other_int: u32,
    }

    impl NestedConfig {
        const fn default_other_int() -> u32 {
            42
        }
    }

    #[derive(Debug, Deserialize)]
    struct TestConfig {
        int: u64,
        bool: bool,
        string: String,
        optional: Option<i64>,
        array: Vec<u32>,
        repeated: HashSet<SimpleEnum>,
        #[serde(flatten)]
        nested: NestedConfig,
    }

    #[test]
    fn parsing() {
        let env = Environment::prefixed(
            "",
            [
                ("int".to_owned(), "1".to_owned()),
                ("bool".to_owned(), "true".to_owned()),
                ("string".to_owned(), "??".to_owned()),
                ("array".to_owned(), "1,2,3".to_owned()),
                ("renamed".to_owned(), "first".to_owned()),
                ("repeated".to_owned(), "second,first".to_owned()),
            ],
        );

        let config = TestConfig::deserialize(env.map).unwrap();
        assert_eq!(config.int, 1);
        assert_eq!(config.optional, None);
        assert!(config.bool);
        assert_eq!(config.string, "??");
        assert_eq!(config.array, [1, 2, 3]);
        assert_eq!(
            config.repeated,
            HashSet::from([SimpleEnum::First, SimpleEnum::Second])
        );
        assert_eq!(config.nested.simple_enum, SimpleEnum::First);
        assert_eq!(config.nested.other_int, 42);
    }

    #[test]
    fn parsing_errors() {
        let env = Environment::prefixed(
            "",
            [
                ("renamed".to_owned(), "first".to_owned()),
                ("other_int".to_owned(), "what".to_owned()),
            ],
        );
        let err = NestedConfig::deserialize(env.map).unwrap_err();

        assert!(err.inner.to_string().contains("u32 value 'what'"), "{err}");
        assert!(
            err.origin.as_ref().unwrap().0.contains("other_int"),
            "{err}"
        );
    }

    #[derive(Debug, Deserialize, DescribeConfig)]
    #[config(crate = crate, merge_from("/deprecated"))]
    struct ConfigWithNesting {
        value: u32,
        #[config(merge_from())]
        #[serde(default)]
        not_merged: String,
        #[config(nested)]
        nested: NestedConfig,

        #[config(nested)]
        #[serde(rename = "deprecated")]
        _deprecated: EmptyConfig,
    }

    #[test]
    fn nesting_json() {
        let env = Environment::prefixed(
            "",
            [
                ("value".to_owned(), "123".to_owned()),
                ("nested_renamed".to_owned(), "first".to_owned()),
                ("nested_other_int".to_owned(), "321".to_owned()),
            ],
        );
        let mut map = env.map;
        map.inner
            .nest(ConfigWithNesting::describe_config())
            .unwrap();
        assert_eq!(
            map.pointer("/", "value").unwrap().inner,
            Value::String("123".to_owned())
        );
        assert_eq!(
            map.pointer("/nested", "renamed").unwrap().inner,
            Value::String("first".to_owned())
        );
        assert_eq!(
            map.pointer("/nested/", "other_int").unwrap().inner,
            Value::String("321".to_owned())
        );

        let Value::Object(global) = &map.inner else {
            panic!("unexpected map: {map:#?}");
        };
        let nested = &global["nested"];
        let Value::Object(nested) = &nested.inner else {
            panic!("unexpected nested value: {nested:#?}");
        };

        assert_eq!(nested["renamed"].inner, Value::String("first".into()));
        assert_eq!(nested["other_int"].inner, Value::String("321".into()));

        let config = ConfigWithNesting::deserialize(map).unwrap();
        assert_eq!(config.value, 123);
        assert_eq!(config.nested.simple_enum, SimpleEnum::First);
        assert_eq!(config.nested.other_int, 321);
    }

    #[test]
    fn merging_config_parts() {
        let env = Environment::prefixed(
            "",
            [
                ("deprecated_value".to_owned(), "4".to_owned()),
                ("nested_renamed".to_owned(), "first".to_owned()),
            ],
        );

        let config: ConfigWithNesting = env.parse().unwrap();
        assert_eq!(config.value, 4);
        assert_eq!(config.nested.simple_enum, SimpleEnum::First);
        assert_eq!(config.nested.other_int, 42);

        let env = Environment::prefixed(
            "",
            [
                ("value".to_owned(), "123".to_owned()),
                ("deprecated_value".to_owned(), "4".to_owned()),
                ("nested_renamed".to_owned(), "first".to_owned()),
                ("deprecated_other_int".to_owned(), "321".to_owned()),
                ("deprecated_not_merged".to_owned(), "!".to_owned()),
            ],
        );

        let config: ConfigWithNesting = env.parse().unwrap();
        assert_eq!(config.value, 123);
        assert_eq!(config.not_merged, "");
        assert_eq!(config.nested.simple_enum, SimpleEnum::First);
        assert_eq!(config.nested.other_int, 321);
    }
}
