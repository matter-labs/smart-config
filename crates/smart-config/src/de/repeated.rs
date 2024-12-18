//! `Repeated` deserializer for arrays / objects.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    hash::{BuildHasher, Hash},
    marker::PhantomData,
    sync::Arc,
};

use serde::de::{DeserializeOwned, Error as DeError};

use crate::{
    de::{DeserializeContext, DeserializeParam, WellKnown},
    error::{ErrorWithOrigin, LowLevelError},
    metadata::{BasicTypes, ParamMetadata, TypeDescription},
    utils::const_eq,
    value::{Map, StrValue, Value, ValueOrigin, WithOrigin},
};

/// Deserializer from JSON arrays.
///
/// Supports deserializing to [`Vec`], arrays, [`HashSet`], [`BTreeSet`].
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
}

impl<T: 'static, De> DeserializeParam<Vec<T>> for Repeated<De>
where
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;

    fn describe(&self, description: &mut TypeDescription) {
        description.set_items(&self.0);
    }

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
    T: 'static + Eq + Hash,
    S: 'static + Default + BuildHasher,
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;

    fn describe(&self, description: &mut TypeDescription) {
        description.set_details("set").set_items(&self.0);
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
    T: 'static + Eq + Ord,
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;

    fn describe(&self, description: &mut TypeDescription) {
        description.set_details("set").set_items(&self.0);
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<BTreeSet<T>, ErrorWithOrigin> {
        self.deserialize_array(ctx, param, None)
    }
}

impl<T: 'static, De, const N: usize> DeserializeParam<[T; N]> for Repeated<De>
where
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = BasicTypes::ARRAY;

    fn describe(&self, description: &mut TypeDescription) {
        description
            .set_details(format!("{N}-element array"))
            .set_items(&self.0);
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

/// Deserializer from JSON objects.
///
/// Supports deserializing to [`HashMap`] and [`BTreeMap`].
pub struct Entries<K, V, DeK, DeV> {
    keys: DeK,
    values: DeV,
    _kv: PhantomData<fn(K, V)>,
}

impl<K, V, DeK: fmt::Debug, DeV: fmt::Debug> fmt::Debug for Entries<K, V, DeK, DeV> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Entries")
            .field("keys", &self.keys)
            .field("values", &self.values)
            .finish()
    }
}

impl<K: WellKnown, V: WellKnown> Entries<K, V, K::Deserializer, V::Deserializer> {
    /// `Entries` instance using the [`WellKnown`] deserializers for keys and values.
    pub const WELL_KNOWN: Self = Self::new(K::DE, V::DE);
}

impl<K, V, DeK, DeV> Entries<K, V, DeK, DeV>
where
    DeK: DeserializeParam<K>,
    DeV: DeserializeParam<V>,
{
    /// Creates a new deserializer instance with provided key and value deserializers.
    pub const fn new(keys: DeK, values: DeV) -> Self {
        Self {
            keys,
            values,
            _kv: PhantomData,
        }
    }

    /// Converts this to a [`NamedEntries`] instance.
    pub const fn named(
        self,
        keys_name: &'static str,
        values_name: &'static str,
    ) -> NamedEntries<K, V, DeK, DeV> {
        assert!(!keys_name.is_empty());
        assert!(!values_name.is_empty());
        assert!(
            !const_eq(keys_name.as_bytes(), values_name.as_bytes()),
            "Keys and values fields must not coincide"
        );

        NamedEntries {
            inner: self,
            keys_name,
            values_name,
        }
    }

    fn deserialize_map<C: FromIterator<(K, V)>>(
        &self,
        mut ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
        map: &Map,
        map_origin: &Arc<ValueOrigin>,
    ) -> Result<C, ErrorWithOrigin> {
        let mut has_errors = false;
        let items = map.iter().filter_map(|(key, value)| {
            let key_as_value = WithOrigin::new(
                Value::String(StrValue::Plain(key.clone())),
                Arc::new(ValueOrigin::Synthetic {
                    source: map_origin.clone(),
                    transform: "string key".into(),
                }),
            );
            let parsed_key =
                parse_key_or_value::<K, _>(&mut ctx, param, key, &self.keys, &key_as_value);
            let parsed_value =
                parse_key_or_value::<V, _>(&mut ctx, param, key, &self.values, value);

            has_errors |= parsed_key.is_none() || parsed_value.is_none();
            Some((parsed_key?, parsed_value?)).filter(|_| !has_errors)
        });
        let items: C = items.collect();

        if has_errors {
            let origin = map_origin.clone();
            Err(ErrorWithOrigin::new(LowLevelError::InvalidObject, origin))
        } else {
            Ok(items)
        }
    }
}

fn parse_key_or_value<T, De: DeserializeParam<T>>(
    ctx: &mut DeserializeContext<'_>,
    param: &'static ParamMetadata,
    key_path: &str,
    de: &De,
    val: &WithOrigin,
) -> Option<T> {
    let coerced = val.coerce_value_type(De::EXPECTING);
    let mut child_ctx = ctx.child(key_path, ctx.location_in_config);
    let mut child_ctx = child_ctx.patched(coerced.as_ref().unwrap_or(val));
    match de.deserialize_param(child_ctx.borrow(), param) {
        Ok(val) => Some(val),
        Err(err) => {
            child_ctx.push_error(err);
            None
        }
    }
}

impl<K, V, C, DeK, DeV> DeserializeParam<C> for Entries<K, V, DeK, DeV>
where
    K: 'static,
    V: 'static,
    DeK: DeserializeParam<K>,
    DeV: DeserializeParam<V>,
    C: FromIterator<(K, V)>,
{
    const EXPECTING: BasicTypes = {
        assert!(
            DeK::EXPECTING.contains(BasicTypes::STRING)
                || DeK::EXPECTING.contains(BasicTypes::INTEGER),
            "map keys must be deserializable from strings or ints"
        );
        BasicTypes::OBJECT
    };

    fn describe(&self, description: &mut TypeDescription) {
        description
            .set_details("map")
            .set_entries(&self.keys, &self.values);
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<C, ErrorWithOrigin> {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        let Value::Object(map) = deserializer.value() else {
            return Err(deserializer.invalid_type("object"));
        };
        self.deserialize_map(ctx, param, map, deserializer.origin())
    }
}

impl<K, V, S> WellKnown for HashMap<K, V, S>
where
    K: 'static + Eq + Hash + WellKnown,
    V: 'static + WellKnown,
    S: 'static + Default + BuildHasher,
{
    type Deserializer = Entries<K, V, K::Deserializer, V::Deserializer>;
    const DE: Self::Deserializer = Entries::new(K::DE, V::DE);
}

impl<K, V> WellKnown for BTreeMap<K, V>
where
    K: 'static + Eq + Ord + WellKnown,
    V: 'static + WellKnown,
{
    type Deserializer = Entries<K, V, K::Deserializer, V::Deserializer>;
    const DE: Self::Deserializer = Entries::new(K::DE, V::DE);
}

/// Deserializer that supports either an array of values, or a string in which values are delimited
/// by the specified separator.
///
/// # Examples
///
/// ```
/// use std::{collections::HashSet, path::PathBuf};
/// use smart_config::{de, testing, DescribeConfig, DeserializeConfig};
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(default, with = de::Delimited(","))]
///     strings: Vec<String>,
///     // More complex types are supported as well
///     #[config(with = de::Delimited(":"))]
///     paths: Vec<PathBuf>,
///     // ...and more complex collections (here together with string -> number coercion)
///     #[config(with = de::Delimited(";"))]
///     ints: HashSet<u64>,
/// }
///
/// let sample = smart_config::config!(
///     "strings": ["test", "string"], // standard array value is still supported
///     "paths": "/usr/bin:/usr/local/bin",
///     "ints": "12;34;12",
/// );
/// let config: TestConfig = testing::test(sample)?;
/// assert_eq!(config.strings.len(), 2);
/// assert_eq!(config.strings[0], "test");
/// assert_eq!(config.paths.len(), 2);
/// assert_eq!(config.paths[1].as_os_str(), "/usr/local/bin");
/// assert_eq!(config.ints, HashSet::from([12, 34]));
/// # anyhow::Ok(())
/// ```
///
/// The wrapping logic is smart enough to catch in compile time an attempt to apply `Delimited` to a type
/// that cannot be deserialized from an array:
///
/// ```compile_fail
/// use smart_config::{de, DescribeConfig, DeserializeConfig};
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct Fail {
///     // will fail with "evaluation of `<Delimited as DeserializeParam<u64>>::EXPECTING` failed"
///     #[config(default, with = de::Delimited(","))]
///     test: u64,
/// }
/// ```
#[derive(Debug)]
pub struct Delimited(pub &'static str);

impl<T: DeserializeOwned + WellKnown> DeserializeParam<T> for Delimited {
    const EXPECTING: BasicTypes = {
        let base = <T::Deserializer as DeserializeParam<T>>::EXPECTING;
        assert!(
            base.contains(BasicTypes::ARRAY),
            "can only apply `Delimited` to types that support deserialization from array"
        );
        base.or(BasicTypes::STRING)
    };

    fn describe(&self, description: &mut TypeDescription) {
        T::DE.describe(description);
        let details = if let Some(details) = description.details() {
            format!("{details}; using {:?} delimiter", self.0)
        } else {
            format!("using {:?} delimiter", self.0)
        };
        description.set_details(details);
    }

    fn deserialize_param(
        &self,
        mut ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        let Some(WithOrigin {
            inner: Value::String(s),
            origin,
        }) = ctx.current_value()
        else {
            return T::DE.deserialize_param(ctx, param);
        };

        let array_origin = Arc::new(ValueOrigin::Synthetic {
            source: origin.clone(),
            transform: format!("{:?}-delimited string", self.0),
        });
        let array_items = s.expose().split(self.0).enumerate().map(|(i, part)| {
            let item_origin = ValueOrigin::Path {
                source: array_origin.clone(),
                path: i.to_string(),
            };
            let part = if s.is_secret() {
                StrValue::Secret(part.into())
            } else {
                StrValue::Plain(part.into())
            };
            WithOrigin::new(Value::String(part), Arc::new(item_origin))
        });
        let array = WithOrigin::new(Value::Array(array_items.collect()), array_origin);
        T::DE.deserialize_param(ctx.patched(&array), param)
    }
}

/// Deserializer that supports either a map or an array of `{ key: _, value: _ }` tuples (with customizable
/// key / value names). Created using [`Entries::named()`].
///
/// Unlike [`Entries`], [`NamedEntries`] doesn't require keys to be deserializable from strings (although
/// if they don't, map inputs will not work).
///
/// # Examples
///
/// ```
/// use std::{collections::HashMap, time::Duration};
/// # use smart_config::{de::Entries, testing, DescribeConfig, DeserializeConfig};
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(with = Entries::WELL_KNOWN.named("num", "value"))]
///     entries: HashMap<u64, String>,
///     /// Can also be used with "linear" containers with tuple items.
///     #[config(with = Entries::WELL_KNOWN.named("method", "timeout"))]
///     tuples: Vec<(String, Duration)>,
/// }
///
/// // Parsing from maps:
/// let map_input = smart_config::config!(
///     "entries": serde_json::json!({ "2": "two", "3": "three" }),
///     "tuples": serde_json::json!({ "getLogs": "2s" }),
/// );
/// let config: TestConfig = testing::test(map_input)?;
/// assert_eq!(
///     config.entries,
///     HashMap::from([(2, "two".to_owned()), (3, "three".to_owned())])
/// );
/// assert_eq!(
///     config.tuples,
///    [("getLogs".to_owned(), Duration::from_secs(2))]
/// );
///
/// // The equivalent input as named tuples:
/// let tuples_input = smart_config::config!(
///     "entries": serde_json::json!([
///         { "num": 2, "value": "two" },
///         { "num": 3, "value": "three" },
///     ]),
///     "tuples": serde_json::json!([
///         { "method": "getLogs", "timeout": "2s" },
///     ]),
/// );
/// let config: TestConfig = testing::test(tuples_input)?;
/// # assert_eq!(
/// #     config.entries,
/// #     HashMap::from([(2, "two".to_owned()), (3, "three".to_owned())])
/// # );
/// # assert_eq!(
/// #     config.tuples,
/// #    [("getLogs".to_owned(), Duration::from_secs(2))]
/// # );
/// # anyhow::Ok(())
/// ```
pub struct NamedEntries<K, V, DeK, DeV> {
    inner: Entries<K, V, DeK, DeV>,
    keys_name: &'static str,
    values_name: &'static str,
}

impl<K, V, DeK: fmt::Debug, DeV: fmt::Debug> fmt::Debug for NamedEntries<K, V, DeK, DeV> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NamedEntries")
            .field("inner", &self.inner)
            .field("keys_name", &self.keys_name)
            .field("values_name", &self.values_name)
            .finish()
    }
}

impl<K, V, DeK, DeV> NamedEntries<K, V, DeK, DeV>
where
    DeK: DeserializeParam<K>,
    DeV: DeserializeParam<V>,
{
    fn deserialize_entries<C: FromIterator<(K, V)>>(
        &self,
        mut ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
        array: &[WithOrigin],
        array_origin: &Arc<ValueOrigin>,
    ) -> Result<C, ErrorWithOrigin> {
        let mut has_errors = false;
        let items = array.iter().enumerate().filter_map(|(i, entry)| {
            let idx_str = i.to_string();
            let (key, value) = match self.parse_entry(entry) {
                Ok(entry) => entry,
                Err(err) => {
                    ctx.child(&idx_str, ctx.location_in_config).push_error(err);
                    has_errors = true;
                    return None;
                }
            };

            let parsed_key =
                parse_key_or_value::<K, _>(&mut ctx, param, &idx_str, &self.inner.keys, key);
            let parsed_value =
                parse_key_or_value::<V, _>(&mut ctx, param, &idx_str, &self.inner.values, value);
            has_errors |= parsed_key.is_none() || parsed_value.is_none();
            Some((parsed_key?, parsed_value?)).filter(|_| !has_errors)
        });
        let items: C = items.collect();

        if has_errors {
            let origin = array_origin.clone();
            Err(ErrorWithOrigin::new(LowLevelError::InvalidArray, origin))
        } else {
            Ok(items)
        }
    }

    fn parse_entry<'a>(
        &self,
        entry: &'a WithOrigin,
    ) -> Result<(&'a WithOrigin, &'a WithOrigin), ErrorWithOrigin> {
        let Value::Object(obj) = &entry.inner else {
            let expected = format!(
                "{{ {:?}: _, {:?}: _ }} tuple",
                self.keys_name, self.values_name
            );
            return Err(entry.invalid_type(&expected));
        };

        let key = obj.get(self.keys_name).ok_or_else(|| {
            let err = DeError::missing_field(self.keys_name);
            ErrorWithOrigin::json(err, entry.origin.clone())
        })?;
        let value = obj.get(self.values_name).ok_or_else(|| {
            let err = DeError::missing_field(self.values_name);
            ErrorWithOrigin::json(err, entry.origin.clone())
        })?;

        if obj.len() > 2 {
            let expected = format!(
                "{{ {:?}: _, {:?}: _ }} tuple",
                self.keys_name, self.values_name
            );
            return Err(entry.invalid_type(&expected));
        }
        Ok((key, value))
    }
}

impl<K, V, DeK, DeV, C> DeserializeParam<C> for NamedEntries<K, V, DeK, DeV>
where
    K: 'static,
    V: 'static,
    DeK: DeserializeParam<K>,
    DeV: DeserializeParam<V>,
    C: FromIterator<(K, V)>,
{
    const EXPECTING: BasicTypes = BasicTypes::OBJECT.or(BasicTypes::ARRAY);

    fn describe(&self, description: &mut TypeDescription) {
        let details = format!(
            "map or array of {{ {:?}: _, {:?}: _ }} tuples",
            self.keys_name, self.values_name
        );
        description
            .set_details(details)
            .set_entries(&self.inner.keys, &self.inner.values);
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<C, ErrorWithOrigin> {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        match deserializer.value() {
            Value::Object(map) => {
                self.inner
                    .deserialize_map(ctx, param, map, deserializer.origin())
            }
            Value::Array(array) => {
                self.deserialize_entries(ctx, param, array, deserializer.origin())
            }
            _ => Err(deserializer.invalid_type("object or array")),
        }
    }
}
