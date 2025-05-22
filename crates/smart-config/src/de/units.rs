//! Param deserializers based on units of measurement.

use std::{fmt, marker::PhantomData, str::FromStr, time::Duration};

use serde::{
    de::{self, EnumAccess, Error as DeError, Unexpected, VariantAccess},
    Deserialize, Deserializer,
};

use crate::{
    de::{DeserializeContext, DeserializeParam, WellKnown},
    error::ErrorWithOrigin,
    metadata::{BasicTypes, ParamMetadata, SizeUnit, TimeUnit, TypeDescription},
    value::Value,
    ByteSize,
};

impl TimeUnit {
    fn overflow_err(self, raw_val: u64) -> serde_json::Error {
        let plural = self.plural();
        DeError::custom(format!(
            "{raw_val} {plural} does not fit into `u64` when converted to seconds"
        ))
    }

    fn into_duration(self, raw_value: u64) -> Result<Duration, serde_json::Error> {
        self.checked_mul(raw_value)
            .ok_or_else(|| self.overflow_err(raw_value))
    }
}

/// Supports deserializing a [`Duration`] from a number, with `self` being the unit of measurement.
///
/// # Examples
///
/// ```
/// # use std::time::Duration;
/// # use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};
/// use smart_config::testing;
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(with = TimeUnit::Millis)]
///     time_ms: Duration,
/// }
///
/// let source = smart_config::config!("time_ms": 100);
/// let config = testing::test::<TestConfig>(source)?;
/// assert_eq!(config.time_ms, Duration::from_millis(100));
/// # anyhow::Ok(())
/// ```
impl DeserializeParam<Duration> for TimeUnit {
    const EXPECTING: BasicTypes = BasicTypes::INTEGER;

    fn describe(&self, description: &mut TypeDescription) {
        description
            .set_details("time duration")
            .set_unit((*self).into());
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Duration, ErrorWithOrigin> {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        let raw_value = u64::deserialize(deserializer)?;
        self.into_duration(raw_value)
            .map_err(|err| deserializer.enrich_err(err))
    }

    fn serialize_param(&self, param: &Duration) -> serde_json::Value {
        match self {
            Self::Millis => serde_json::to_value(param.as_millis()).unwrap(),
            Self::Seconds => param.as_secs().into(),
            Self::Minutes => (param.as_secs() / 60).into(),
            Self::Hours => (param.as_secs() / 3_600).into(),
            Self::Days => (param.as_secs() / 86_400).into(),
            Self::Weeks => (param.as_secs() / 86_400 / 7).into(),
        }
    }
}

/// Supports deserializing a [`ByteSize`] from a number, with `self` being the unit of measurement.
///
/// # Examples
///
/// ```
/// # use std::time::Duration;
/// # use smart_config::{metadata::SizeUnit, DescribeConfig, DeserializeConfig, ByteSize};
/// use smart_config::testing;
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(with = SizeUnit::MiB)]
///     size_mb: ByteSize,
/// }
///
/// let source = smart_config::config!("size_mb": 4);
/// let config = testing::test::<TestConfig>(source)?;
/// assert_eq!(config.size_mb, ByteSize(4 << 20));
/// # anyhow::Ok(())
/// ```
impl DeserializeParam<ByteSize> for SizeUnit {
    const EXPECTING: BasicTypes = BasicTypes::INTEGER;

    fn describe(&self, description: &mut TypeDescription) {
        description
            .set_details("byte size")
            .set_unit((*self).into());
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<ByteSize, ErrorWithOrigin> {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        let raw_value = u64::deserialize(deserializer)?;
        ByteSize::checked(raw_value, *self).ok_or_else(|| {
            let err = DeError::custom(format!(
                "{raw_value} {unit} does not fit into `u64`",
                unit = self.plural()
            ));
            deserializer.enrich_err(err)
        })
    }

    fn serialize_param(&self, param: &ByteSize) -> serde_json::Value {
        match self {
            Self::Bytes => param.0.into(),
            Self::KiB => (param.0 >> 10).into(),
            Self::MiB => (param.0 >> 20).into(),
            Self::GiB => (param.0 >> 30).into(),
        }
    }
}

/// Default deserializer for [`Duration`]s and [`ByteSize`]s.
///
/// Values can be deserialized from 2 formats:
///
/// - String consisting of an integer, optional whitespace and a unit, such as "30 secs" or "500ms" (for `Duration`) /
///   "4 MiB" (for `ByteSize`). The unit must correspond to a [`TimeUnit`] / [`SizeUnit`].
/// - Object with a single key and an integer value, such as `{ "hours": 3 }` (for `Duration`) / `{ "kb": 512 }` (for `SizeUnit`).
///
/// Thanks to nesting of object params, the second approach automatically means that a duration can be parsed
/// from a param name suffixed with a unit. For example, a value `latency_ms: 500` for parameter `latency`
/// will be recognized as 500 ms.
///
/// # Examples
///
/// ```
/// # use std::time::Duration;
/// # use smart_config::{testing, ByteSize, Environment, DescribeConfig, DeserializeConfig};
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     latency: Duration,
///     disk: ByteSize,
/// }
///
/// // Parsing from a string
/// let source = smart_config::config!("latency": "30 secs", "disk": "256 MiB");
/// let config: TestConfig = testing::test(source)?;
/// assert_eq!(config.latency, Duration::from_secs(30));
/// assert_eq!(config.disk, ByteSize(256 << 20));
///
/// // Parsing from an object
/// let source = smart_config::config!(
///     "latency": serde_json::json!({ "hours": 3 }),
///     "disk": serde_json::json!({ "gigabytes": 2 }),
/// );
/// let config: TestConfig = testing::test(source)?;
/// assert_eq!(config.latency, Duration::from_secs(3 * 3_600));
/// assert_eq!(config.disk, ByteSize(2 << 30));
///
/// // Parsing from a suffixed parameter name
/// let source = Environment::from_iter("", [("LATENCY_SEC", "15"), ("DISK_GB", "10")]);
/// let config: TestConfig = testing::test(source)?;
/// assert_eq!(config.latency, Duration::from_secs(15));
/// assert_eq!(config.disk, ByteSize(10 << 30));
/// # anyhow::Ok(())
/// ```
#[derive(Debug, Clone, Copy)]
pub struct WithUnit;

/// Helper trait allowing to unify enum parsing for durations and byte sizes.
trait EnumWithUnit: Sized {
    const VARIANTS: &'static [&'static str];

    fn extract_variant(unit: &str) -> Option<fn(u64) -> Self>;

    fn parse<E: de::Error>(unit: &str, value: u64) -> Result<Self, E> {
        let variant_mapper = Self::extract_variant(unit)
            .ok_or_else(|| DeError::unknown_variant(unit, Self::VARIANTS))?;
        Ok(variant_mapper(value))
    }
}

#[derive(Debug)]
struct EnumVisitor<T>(PhantomData<T>);

impl<'v, T: EnumWithUnit> de::Visitor<'v> for EnumVisitor<T> {
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "enum with one of {:?} variants",
            RawDuration::VARIANTS
        )
    }

    fn visit_enum<A: EnumAccess<'v>>(self, data: A) -> Result<Self::Value, A::Error> {
        let (tag, payload) = data.variant::<String>()?;
        let value = payload.newtype_variant::<u64>()?;
        let unit = tag.strip_prefix("in_").unwrap_or(&tag);
        T::parse(unit, value)
    }
}

/// Raw `Duration` representation used by the `WithUnit` deserializer.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
enum RawDuration {
    Millis(u64),
    Seconds(u64),
    Minutes(u64),
    Hours(u64),
    Days(u64),
    Weeks(u64),
}

macro_rules! impl_enum_with_unit {
    ($($($name:tt)|+ => $func:expr,)+) => {
        const VARIANTS: &'static [&'static str] = &[$($($name,)+)+];

        fn extract_variant(unit: &str) -> Option<fn(u64) -> Self> {
            Some(match unit {
                $($($name )|+ => $func,)+
                _ => return None,
            })
        }
    };
}

impl EnumWithUnit for RawDuration {
    impl_enum_with_unit!(
        "milliseconds" | "millis" | "ms" => Self::Millis,
        "seconds" | "second" | "secs" | "sec" | "s" => Self::Seconds,
        "minutes" | "minute" | "mins" | "min" | "m" => Self::Minutes,
        "hours" | "hour" | "hr" | "h" => Self::Hours,
        "days" | "day" | "d" => Self::Days,
        "weeks" | "week" | "w" => Self::Weeks,
    );
}

impl<'de> Deserialize<'de> for RawDuration {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_enum("RawDuration", Self::VARIANTS, EnumVisitor(PhantomData))
    }
}

impl RawDuration {
    const EXPECTING: &'static str = "value with unit, like '10 ms'";
}

impl FromStr for RawDuration {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let unit_start = s
            .find(|ch: char| !ch.is_ascii_digit())
            .ok_or_else(|| DeError::invalid_type(Unexpected::Str(s), &Self::EXPECTING))?;
        if unit_start == 0 {
            return Err(DeError::invalid_type(Unexpected::Str(s), &Self::EXPECTING));
        }

        let value: u64 = s[..unit_start].parse().map_err(DeError::custom)?;
        let unit = s[unit_start..].trim();
        Self::parse(unit, value)
    }
}

impl TryFrom<RawDuration> for Duration {
    type Error = serde_json::Error;

    fn try_from(value: RawDuration) -> Result<Self, Self::Error> {
        let (unit, raw_value) = match value {
            RawDuration::Millis(val) => (TimeUnit::Millis, val),
            RawDuration::Seconds(val) => (TimeUnit::Seconds, val),
            RawDuration::Minutes(val) => (TimeUnit::Minutes, val),
            RawDuration::Hours(val) => (TimeUnit::Hours, val),
            RawDuration::Days(val) => (TimeUnit::Days, val),
            RawDuration::Weeks(val) => (TimeUnit::Weeks, val),
        };
        unit.into_duration(raw_value)
    }
}

impl DeserializeParam<Duration> for WithUnit {
    const EXPECTING: BasicTypes = BasicTypes::STRING.or(BasicTypes::OBJECT);

    fn describe(&self, description: &mut TypeDescription) {
        description.set_details("duration with unit, or object with single unit key");
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Duration, ErrorWithOrigin> {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        let raw = if let Value::String(s) = deserializer.value() {
            s.expose()
                .parse::<RawDuration>()
                .map_err(|err| deserializer.enrich_err(err))?
        } else {
            RawDuration::deserialize(deserializer)?
        };
        raw.try_into().map_err(|err| deserializer.enrich_err(err))
    }

    fn serialize_param(&self, param: &Duration) -> serde_json::Value {
        if param.is_zero() {
            // Special case to produce more "expected" string.
            return "0s".into();
        }

        let duration_string = if param.subsec_millis() != 0 {
            format!("{}ms", param.as_millis())
        } else {
            let seconds = param.as_secs();
            if seconds % 60 != 0 {
                format!("{seconds}s")
            } else if seconds % 3_600 != 0 {
                format!("{}min", seconds / 60)
            } else if seconds % 86_400 != 0 {
                format!("{}h", seconds / 3_600)
            } else if seconds % (86_400 * 7) != 0 {
                format!("{}d", seconds / 86_400)
            } else {
                format!("{}w", seconds / (86_400 * 7))
            }
        };
        duration_string.into()
    }
}

impl WellKnown for Duration {
    type Deserializer = WithUnit;
    const DE: Self::Deserializer = WithUnit;
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
enum RawByteSize {
    Bytes(u64),
    Kilobytes(u64),
    Megabytes(u64),
    Gigabytes(u64),
}

impl EnumWithUnit for RawByteSize {
    impl_enum_with_unit!(
        "bytes" | "b" => Self::Bytes,
        "kilobytes" | "kb" | "kib" => Self::Kilobytes,
        "megabytes" | "mb" | "mib" => Self::Megabytes,
        "gigabytes" | "gb" | "gib" => Self::Gigabytes,
    );
}

impl RawByteSize {
    const EXPECTING: &'static str = "value with unit, like '32 MB'";
}

impl<'de> Deserialize<'de> for RawByteSize {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_enum("RawByteSize", Self::VARIANTS, EnumVisitor(PhantomData))
    }
}

impl FromStr for RawByteSize {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let unit_start = s
            .find(|ch: char| !ch.is_ascii_digit())
            .ok_or_else(|| DeError::invalid_type(Unexpected::Str(s), &Self::EXPECTING))?;
        if unit_start == 0 {
            return Err(DeError::invalid_type(Unexpected::Str(s), &Self::EXPECTING));
        }

        let value: u64 = s[..unit_start].parse().map_err(DeError::custom)?;
        let unit = s[unit_start..].trim();
        Self::parse(&unit.to_lowercase(), value)
    }
}

impl TryFrom<RawByteSize> for ByteSize {
    type Error = serde_json::Error;

    fn try_from(value: RawByteSize) -> Result<Self, Self::Error> {
        let (unit, raw_value) = match value {
            RawByteSize::Bytes(val) => (SizeUnit::Bytes, val),
            RawByteSize::Kilobytes(val) => (SizeUnit::KiB, val),
            RawByteSize::Megabytes(val) => (SizeUnit::MiB, val),
            RawByteSize::Gigabytes(val) => (SizeUnit::GiB, val),
        };
        ByteSize::checked(raw_value, unit).ok_or_else(|| {
            DeError::custom(format!(
                "{raw_value} {unit} does not fit into `u64`",
                unit = unit.plural()
            ))
        })
    }
}

impl DeserializeParam<ByteSize> for WithUnit {
    const EXPECTING: BasicTypes = BasicTypes::STRING.or(BasicTypes::OBJECT);

    fn describe(&self, description: &mut TypeDescription) {
        description.set_details("size with unit, or object with single unit key");
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<ByteSize, ErrorWithOrigin> {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        let raw = if let Value::String(s) = deserializer.value() {
            s.expose()
                .parse::<RawByteSize>()
                .map_err(|err| deserializer.enrich_err(err))?
        } else {
            RawByteSize::deserialize(deserializer)?
        };
        raw.try_into().map_err(|err| deserializer.enrich_err(err))
    }

    fn serialize_param(&self, param: &ByteSize) -> serde_json::Value {
        param.to_string().into()
    }
}

impl WellKnown for ByteSize {
    type Deserializer = WithUnit;
    const DE: Self::Deserializer = WithUnit;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_time_string() {
        let duration: RawDuration = "10ms".parse().unwrap();
        assert_eq!(duration, RawDuration::Millis(10));
        let duration: RawDuration = "50    seconds".parse().unwrap();
        assert_eq!(duration, RawDuration::Seconds(50));
        let duration: RawDuration = "40s".parse().unwrap();
        assert_eq!(duration, RawDuration::Seconds(40));
        let duration: RawDuration = "10 min".parse().unwrap();
        assert_eq!(duration, RawDuration::Minutes(10));
        let duration: RawDuration = "10m".parse().unwrap();
        assert_eq!(duration, RawDuration::Minutes(10));
        let duration: RawDuration = "12 hours".parse().unwrap();
        assert_eq!(duration, RawDuration::Hours(12));
        let duration: RawDuration = "12h".parse().unwrap();
        assert_eq!(duration, RawDuration::Hours(12));
        let duration: RawDuration = "30d".parse().unwrap();
        assert_eq!(duration, RawDuration::Days(30));
        let duration: RawDuration = "1 day".parse().unwrap();
        assert_eq!(duration, RawDuration::Days(1));
        let duration: RawDuration = "2 weeks".parse().unwrap();
        assert_eq!(duration, RawDuration::Weeks(2));
        let duration: RawDuration = "3w".parse().unwrap();
        assert_eq!(duration, RawDuration::Weeks(3));
    }

    #[test]
    fn parsing_time_string_errors() {
        let err = "".parse::<RawDuration>().unwrap_err().to_string();
        assert!(err.starts_with("invalid type"), "{err}");
        let err = "???".parse::<RawDuration>().unwrap_err().to_string();
        assert!(err.starts_with("invalid type"), "{err}");
        let err = "10".parse::<RawDuration>().unwrap_err().to_string();
        assert!(err.starts_with("invalid type"), "{err}");
        let err = "hours".parse::<RawDuration>().unwrap_err().to_string();
        assert!(err.starts_with("invalid type"), "{err}");

        let err = "111111111111111111111111111111111111111111s"
            .parse::<RawDuration>()
            .unwrap_err()
            .to_string();
        assert!(err.contains("too large"), "{err}");

        let err = "10 months".parse::<RawDuration>().unwrap_err().to_string();
        assert!(err.starts_with("unknown variant"), "{err}");
    }

    #[test]
    fn parsing_byte_size_string() {
        let size: RawByteSize = "16bytes".parse().unwrap();
        assert_eq!(size, RawByteSize::Bytes(16));
        let size: RawByteSize = "128    KiB".parse().unwrap();
        assert_eq!(size, RawByteSize::Kilobytes(128));
        let size: RawByteSize = "16 kb".parse().unwrap();
        assert_eq!(size, RawByteSize::Kilobytes(16));
        let size: RawByteSize = "4MB".parse().unwrap();
        assert_eq!(size, RawByteSize::Megabytes(4));
        let size: RawByteSize = "1 GB".parse().unwrap();
        assert_eq!(size, RawByteSize::Gigabytes(1));
    }

    #[test]
    fn serializing_with_time_unit() {
        let val = TimeUnit::Millis.serialize_param(&Duration::from_millis(10));
        assert_eq!(val, 10_u32);
        let val = TimeUnit::Millis.serialize_param(&Duration::from_secs(10));
        assert_eq!(val, 10_000_u32);
        let val = TimeUnit::Seconds.serialize_param(&Duration::from_secs(10));
        assert_eq!(val, 10_u32);
        let val = TimeUnit::Minutes.serialize_param(&Duration::from_secs(10));
        assert_eq!(val, 0_u32);
        let val = TimeUnit::Minutes.serialize_param(&Duration::from_secs(120));
        assert_eq!(val, 2_u32);
    }

    #[test]
    fn serializing_with_size_unit() {
        let val = SizeUnit::Bytes.serialize_param(&ByteSize(128));
        assert_eq!(val, 128_u32);
        let val = SizeUnit::Bytes.serialize_param(&ByteSize(1 << 16));
        assert_eq!(val, 1_u32 << 16);
        let val = SizeUnit::KiB.serialize_param(&ByteSize(1 << 16));
        assert_eq!(val, 1_u32 << 6);
        let val = SizeUnit::MiB.serialize_param(&ByteSize(1 << 16));
        assert_eq!(val, 0_u32);
        let val = SizeUnit::MiB.serialize_param(&ByteSize::new(3, SizeUnit::MiB));
        assert_eq!(val, 3_u32);
    }

    #[test]
    fn serializing_with_duration() {
        let val = WithUnit.serialize_param(&Duration::ZERO);
        assert_eq!(val, "0s");
        let val = WithUnit.serialize_param(&Duration::from_millis(10));
        assert_eq!(val, "10ms");
        let val = WithUnit.serialize_param(&Duration::from_secs(5));
        assert_eq!(val, "5s");
        let val = WithUnit.serialize_param(&Duration::from_millis(5_050));
        assert_eq!(val, "5050ms");
        let val = WithUnit.serialize_param(&Duration::from_secs(300));
        assert_eq!(val, "5min");
        let val = WithUnit.serialize_param(&Duration::from_secs(7_200));
        assert_eq!(val, "2h");
        let val = WithUnit.serialize_param(&Duration::from_secs(86_400));
        assert_eq!(val, "1d");
    }

    #[test]
    fn serializing_with_byte_size() {
        let val = WithUnit.serialize_param(&ByteSize(0));
        assert_eq!(val, "0 B");
        let val = WithUnit.serialize_param(&ByteSize(128));
        assert_eq!(val, "128 B");
        let val = WithUnit.serialize_param(&ByteSize(32 << 10));
        assert_eq!(val, "32 KiB");
        let val = WithUnit.serialize_param(&ByteSize(3 << 20));
        assert_eq!(val, "3 MiB");
    }
}
