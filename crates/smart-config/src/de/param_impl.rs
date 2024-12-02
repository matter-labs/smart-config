//! Implementations of parameter deserializers.

use std::{str::FromStr, time::Duration};

use serde::{
    de::{Error as DeError, Unexpected},
    Deserialize,
};

use crate::{
    de::{DeserializeContext, DeserializeParam, WellKnown},
    error::ErrorWithOrigin,
    metadata::{BasicTypes, ParamMetadata, SizeUnit, TimeUnit, TypeQualifiers},
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
        const SECONDS_IN_MINUTE: u64 = 60;
        const SECONDS_IN_HOUR: u64 = 3_600;
        const SECONDS_IN_DAY: u64 = 86_400;

        Ok(match self {
            Self::Millis => Duration::from_millis(raw_value),
            Self::Seconds => Duration::from_secs(raw_value),
            Self::Minutes => {
                let val = raw_value
                    .checked_mul(SECONDS_IN_MINUTE)
                    .ok_or_else(|| self.overflow_err(raw_value))?;
                Duration::from_secs(val)
            }
            Self::Hours => {
                let val = raw_value
                    .checked_mul(SECONDS_IN_HOUR)
                    .ok_or_else(|| self.overflow_err(raw_value))?;
                Duration::from_secs(val)
            }
            Self::Days => {
                let val = raw_value
                    .checked_mul(SECONDS_IN_DAY)
                    .ok_or_else(|| self.overflow_err(raw_value))?;
                Duration::from_secs(val)
            }
        })
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

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new("time duration").with_unit((*self).into())
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

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new("byte size").with_unit((*self).into())
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

/// Default deserializer for [`Duration`]s.
#[derive(Debug, Clone, Copy)]
pub struct WithUnit;

/// Raw `Duration` representation used by the `WithUnit` deserializer.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(rename_all = "lowercase")]
enum RawDuration {
    #[serde(alias = "ms", alias = "milliseconds")]
    Millis(u64),
    #[serde(alias = "sec", alias = "secs")]
    Seconds(u64),
    #[serde(alias = "min", alias = "mins")]
    Minutes(u64),
    #[serde(alias = "hr")]
    Hours(u64),
    #[serde(alias = "d")]
    Days(u64),
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
            "millis" | "ms" | "milliseconds" => Self::Millis(value),
            "seconds" | "sec" | "secs" => Self::Seconds(value),
            "minutes" | "min" | "mins" => Self::Minutes(value),
            "hours" | "hr" => Self::Hours(value),
            "days" | "d" => Self::Days(value),
            _ => {
                return Err(DeError::invalid_value(
                    Unexpected::Str(unit),
                    &"duration unit, like 'ms', up to 'days'",
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
        };
        unit.into_duration(raw_value)
    }
}

impl DeserializeParam<Duration> for WithUnit {
    const EXPECTING: BasicTypes = BasicTypes::STRING.or(BasicTypes::OBJECT);

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::new("duration with unit, or object with single unit key")
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<Duration, ErrorWithOrigin> {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        let raw = if let Value::String(s) = deserializer.value() {
            s.parse::<RawDuration>()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_time_string() {
        let duration: RawDuration = "10ms".parse().unwrap();
        assert_eq!(duration, RawDuration::Millis(10));
        let duration: RawDuration = "50    seconds".parse().unwrap();
        assert_eq!(duration, RawDuration::Seconds(50));
        let duration: RawDuration = "10 min".parse().unwrap();
        assert_eq!(duration, RawDuration::Minutes(10));
        let duration: RawDuration = "12 hours".parse().unwrap();
        assert_eq!(duration, RawDuration::Hours(12));
        let duration: RawDuration = "30d".parse().unwrap();
        assert_eq!(duration, RawDuration::Days(30));
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
}