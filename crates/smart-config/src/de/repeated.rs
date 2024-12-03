//! `Repeated` deserializer for arrays / objects.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    hash::{BuildHasher, Hash},
    str::FromStr,
};

use serde::de::Error as DeError;

use crate::{
    de::{DeserializeContext, DeserializeParam, WellKnown},
    error::{ErrorWithOrigin, LowLevelError},
    metadata::{BasicTypes, ParamMetadata, TypeQualifiers},
    value::Value,
};

/// Deserializer from JSON arrays and objects.
///
/// Supports the following param types:
///
/// - [`Vec`], arrays, [`HashSet`], [`BTreeSet`] when deserializing from an array
/// - [`HashMap`] and [`BTreeMap`] when deserializing from an object
#[derive(Debug)]
pub struct Repeated<De>(pub De);

impl<De> Repeated<De> {
    fn deserialize_array<T, C>(
        &self,
        mut ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
        expected_len: Option<usize>,
    ) -> Result<C, ErrorWithOrigin>
    where
        De: DeserializeParam<T>,
        C: FromIterator<T>,
    {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        let Value::Array(items) = deserializer.value() else {
            return Err(deserializer.invalid_type("array"));
        };

        if let Some(expected_len) = expected_len {
            if items.len() != expected_len {
                let err = DeError::invalid_length(items.len(), &expected_len.to_string().as_str());
                return Err(deserializer.enrich_err(err));
            }
        }

        let mut has_errors = false;
        let items = items.iter().enumerate().filter_map(|(i, item)| {
            let coerced = item.coerce_value_type(De::EXPECTING);
            let mut child_ctx = ctx.child(&i.to_string(), ctx.location_in_config);
            let mut child_ctx = child_ctx.patched(coerced.as_ref().unwrap_or(item));
            match self.0.deserialize_param(child_ctx.borrow(), param) {
                Ok(val) if !has_errors => Some(val),
                Ok(_) => None, // Drop the value since it won't be needed anyway
                Err(err) => {
                    has_errors = true;
                    child_ctx.push_error(err);
                    None
                }
            }
        });
        let items: C = items.collect();

        if has_errors {
            let origin = deserializer.origin().clone();
            Err(ErrorWithOrigin::new(LowLevelError::InvalidArray, origin))
        } else {
            Ok(items)
        }
    }

    fn deserialize_map<K, V, C>(
        &self,
        mut ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<C, ErrorWithOrigin>
    where
        K: FromStr<Err: fmt::Display>,
        De: DeserializeParam<V>,
        C: FromIterator<(K, V)>,
    {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        let Value::Object(items) = deserializer.value() else {
            return Err(deserializer.invalid_type("object"));
        };

        let mut has_errors = false;
        let items = items.iter().filter_map(|(key, value)| {
            let coerced = value.coerce_value_type(De::EXPECTING);
            let mut child_ctx = ctx.child(key, ctx.location_in_config);
            let mut child_ctx = child_ctx.patched(coerced.as_ref().unwrap_or(value));

            let key = match key.parse::<K>() {
                Ok(val) if !has_errors => Some(val),
                Ok(_) => None,
                Err(err) => {
                    has_errors = true;
                    child_ctx.push_error(DeError::custom(format!("cannot deserialize key: {err}")));
                    None
                }
            };
            let value = match self.0.deserialize_param(child_ctx.borrow(), param) {
                Ok(val) if !has_errors => Some(val),
                Ok(_) => None,
                Err(err) => {
                    has_errors = true;
                    child_ctx.push_error(err);
                    None
                }
            };
            Some((key?, value?))
        });
        let items: C = items.collect();

        if has_errors {
            let origin = deserializer.origin().clone();
            Err(ErrorWithOrigin::new(LowLevelError::InvalidObject, origin))
        } else {
            Ok(items)
        }
    }
}

impl<T, De> DeserializeParam<Vec<T>> for Repeated<De>
where
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Vec<T>, ErrorWithOrigin> {
        self.deserialize_array(ctx, param, None)
    }
}

impl<T, S, De> DeserializeParam<HashSet<T, S>> for Repeated<De>
where
    T: Eq + Hash,
    S: 'static + Default + BuildHasher,
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new("set")
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<HashSet<T, S>, ErrorWithOrigin> {
        self.deserialize_array(ctx, param, None)
    }
}

impl<T, De> DeserializeParam<BTreeSet<T>> for Repeated<De>
where
    T: Eq + Ord,
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new("set")
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<BTreeSet<T>, ErrorWithOrigin> {
        self.deserialize_array(ctx, param, None)
    }
}

impl<T, De, const N: usize> DeserializeParam<[T; N]> for Repeated<De>
where
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new("fixed-sized array")
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<[T; N], ErrorWithOrigin> {
        let items: Vec<_> = self.deserialize_array(ctx, param, Some(N))?;
        // `unwrap()` is safe due to the length check in `deserialize_inner()`
        Ok(items.try_into().ok().unwrap())
    }
}

impl<T: WellKnown> WellKnown for Vec<T> {
    type Deserializer = Repeated<T::Deserializer>;
    const DE: Self::Deserializer = Repeated(T::DE);
}

impl<T: WellKnown, const N: usize> WellKnown for [T; N] {
    type Deserializer = Repeated<T::Deserializer>;
    const DE: Self::Deserializer = Repeated(T::DE);
}

// Heterogeneous tuples don't look like a good idea to mark as well-known because they wouldn't look well-structured
// (it'd be better to define either multiple params or a struct param).

impl<T, S> WellKnown for HashSet<T, S>
where
    T: Eq + Hash + WellKnown,
    S: 'static + Default + BuildHasher,
{
    type Deserializer = Repeated<T::Deserializer>;
    const DE: Self::Deserializer = Repeated(T::DE);
}

impl<T> WellKnown for BTreeSet<T>
where
    T: Eq + Ord + WellKnown,
{
    type Deserializer = Repeated<T::Deserializer>;
    const DE: Self::Deserializer = Repeated(T::DE);
}

impl<K, V, S, De> DeserializeParam<HashMap<K, V, S>> for Repeated<De>
where
    K: 'static + Eq + Hash + FromStr<Err: fmt::Display>,
    S: 'static + Default + BuildHasher,
    De: DeserializeParam<V>,
{
    const EXPECTING: BasicTypes = BasicTypes::OBJECT;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new("map")
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<HashMap<K, V, S>, ErrorWithOrigin> {
        self.deserialize_map(ctx, param)
    }
}

impl<K, V, De> DeserializeParam<BTreeMap<K, V>> for Repeated<De>
where
    K: 'static + Eq + Ord + FromStr<Err: fmt::Display>,
    De: DeserializeParam<V>,
{
    const EXPECTING: BasicTypes = BasicTypes::OBJECT;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new("map")
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<BTreeMap<K, V>, ErrorWithOrigin> {
        self.deserialize_map(ctx, param)
    }
}

impl<K, V, S> WellKnown for HashMap<K, V, S>
where
    K: 'static + Eq + Hash + FromStr<Err: fmt::Display>,
    V: WellKnown,
    S: 'static + Default + BuildHasher,
{
    type Deserializer = Repeated<V::Deserializer>;
    const DE: Self::Deserializer = Repeated(V::DE);
}

impl<K, V> WellKnown for BTreeMap<K, V>
where
    K: 'static + Eq + Ord + FromStr<Err: fmt::Display>,
    V: WellKnown,
{
    type Deserializer = Repeated<V::Deserializer>;
    const DE: Self::Deserializer = Repeated(V::DE);
}
