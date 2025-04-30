//! Param deserializers based on units of measurement.

use std::{str::FromStr, time::Duration};

use serde::{
    de::{Error as DeError, Unexpected},
    Deserialize,
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

/// Raw `Duration` representation used by the `WithUnit` deserializer.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(rename_all = "lowercase")]
enum RawDuration {
    #[serde(alias = "ms", alias = "milliseconds")]
    Millis(u64),
    #[serde(alias = "second", alias = "s", alias = "sec", alias = "secs")]
    Seconds(u64),
    #[serde(alias = "minute", alias = "min", alias = "mins", alias = "m")]
    Minutes(u64),
    #[serde(alias = "hour", alias = "hr", alias = "h")]
    Hours(u64),
    #[serde(alias = "day", alias = "d")]
    Days(u64),
    #[serde(alias = "week", alias = "w")]
    Weeks(u64),
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
        Ok(match unit {
            "milliseconds" | "millis" | "ms" => Self::Millis(value),
            "seconds" | "second" | "secs" | "sec" | "s" => Self::Seconds(value),
            "minutes" | "minute" | "mins" | "min" | "m" => Self::Minutes(value),
            "hours" | "hour" | "hr" | "h" => Self::Hours(value),
            "days" | "day" | "d" => Self::Days(value),
            "weeks" | "week" | "w" => Self::Weeks(value),
            _ => {
                return Err(DeError::invalid_value(
                    Unexpected::Str(unit),
                    &"duration unit, like 'ms', up to 'weeks'",
                ))
            }
        })
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
}

impl WellKnown for Duration {
    type Deserializer = WithUnit;
    const DE: Self::Deserializer = WithUnit;
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(rename_all = "lowercase")]
enum RawByteSize {
    Bytes(u64),
    #[serde(alias = "kb", alias = "kib")]
    Kilobytes(u64),
    #[serde(alias = "mb", alias = "mib")]
    Megabytes(u64),
    #[serde(alias = "gb", alias = "gib")]
    Gigabytes(u64),
}

impl RawByteSize {
    const EXPECTING: &'static str = "value with unit, like '32 MB'";
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
        Ok(match unit.to_lowercase().as_str() {
            "bytes" | "b" => Self::Bytes(value),
            "kb" | "kib" => Self::Kilobytes(value),
            "mb" | "mib" => Self::Megabytes(value),
            "gb" | "gib" => Self::Gigabytes(value),
            _ => {
                return Err(DeError::invalid_value(
                    Unexpected::Str(unit),
                    &"duration unit, like 'KB', up to 'GB'",
                ))
            }
        })
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
        assert!(err.starts_with("invalid value"), "{err}");
        assert!(err.contains("duration unit"), "{err}");
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
}
