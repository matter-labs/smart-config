use std::{cmp, fmt, str::FromStr};

use serde::{de, Deserialize, Deserializer};

/// Ad-hoc decimal with `u64` precision.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Decimal {
    unscaled_value: u64,
    scale: u8,
}

impl fmt::Display for Decimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scale == 0 {
            fmt::Display::fmt(&self.unscaled_value, formatter)
        } else {
            let value_str = self.unscaled_value.to_string();
            let (int, fraction) = value_str.split_at(value_str.len() - usize::from(self.scale));
            write!(formatter, "{int}.{fraction}")
        }
    }
}

impl PartialEq for Decimal {
    fn eq(&self, other: &Self) -> bool {
        let (lesser_scaled, greater_scaled) = match self.scale.cmp(&other.scale) {
            cmp::Ordering::Less | cmp::Ordering::Equal => (self, other),
            cmp::Ordering::Greater => (other, self),
        };
        let scale_diff = greater_scaled.scale - lesser_scaled.scale;
        let Some(multiplier) = 10_u64.checked_pow(scale_diff.into()) else {
            return false;
        };
        lesser_scaled.unscaled_value.checked_mul(multiplier) == Some(greater_scaled.unscaled_value)
    }
}

impl Decimal {
    const EXPECTING: &'static str = "decimal fraction like 1.5";

    const ZERO: Self = Self {
        unscaled_value: 0,
        scale: 0,
    };

    #[allow(dead_code)] // FIXME
    pub(crate) fn scale(self, scale: u8) -> Result<u64, serde_json::Error> {
        let scale_diff = scale.checked_sub(self.scale).ok_or_else(|| {
            de::Error::custom(format!(
                "{self} has greater precision ({}) than allowed ({scale})",
                self.scale
            ))
        })?;
        let multiplier = 10_u64.checked_pow(scale_diff.into()).ok_or_else(|| {
            de::Error::custom(format!("overflow converting {self} to precision {scale}"))
        })?;
        self.unscaled_value.checked_mul(multiplier).ok_or_else(|| {
            de::Error::custom(format!("overflow converting {self} to precision {scale}"))
        })
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn from_f64<E: de::Error>(value: f64) -> Result<Self, E> {
        // `f64` provides ~15 accurate decimal digits
        const MAX_SAFE_INT: f64 = 1e15;
        const SCALES: &[f64] = &[
            1.0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15,
        ];

        if value < 0.0 || value.is_infinite() || value.is_nan() || value.is_subnormal() {
            return Err(de::Error::invalid_value(
                de::Unexpected::Float(value),
                &Self::EXPECTING,
            ));
        }
        if value == 0.0 {
            return Ok(Self::ZERO);
        }

        let maybe_scale_data = SCALES.iter().zip(0_u8..).find_map(|(&scale, i)| {
            let unscaled_value = value * scale;
            let rounded_value = unscaled_value.round();
            if rounded_value > MAX_SAFE_INT {
                return None;
            }

            // No division by zero since `value > 0.0` (checked above).
            let round_err = (unscaled_value - rounded_value).abs() / unscaled_value;
            (round_err < f64::EPSILON).then_some((i, rounded_value))
        });

        // FIXME: maybe it makes more sense to return an approximation here?
        let (mut scale, unscaled_value) = maybe_scale_data.ok_or_else(|| {
            de::Error::custom(format!(
                "precision lost converting value {value} to a decimal; quote the value to avoid precision loss"
            ))
        })?;

        let mut unscaled_value = unscaled_value as u64;
        // Reduce the value scale.
        while scale > 0 && unscaled_value % 10 == 0 {
            scale -= 1;
            unscaled_value /= 10;
        }

        Ok(Self {
            unscaled_value,
            scale,
        })
    }
}

impl FromStr for Decimal {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut unscaled_value = 0;
        let mut scale = None;
        let mut exponent = 1_u64;
        let mut digit_count = 0;
        for ch in s.bytes().rev() {
            match ch {
                b'0' if scale.is_none() && unscaled_value == 0 => {
                    // skip trailing zeros (e.g., in `1.500`) to not blow up the scale unnecessarily
                }
                b'0'..=b'9' => {
                    unscaled_value += u64::from(ch - b'0') * exponent;
                    digit_count += 1;
                    exponent = exponent
                        .checked_mul(10)
                        .ok_or_else(|| de::Error::custom("too many digits"))?;
                }
                b'.' => {
                    if scale.is_some() {
                        return Err(de::Error::invalid_value(
                            de::Unexpected::Str(s),
                            &Self::EXPECTING,
                        ));
                    }
                    scale = Some(digit_count);
                }
                b'_' => { /* skip spacing */ }
                _ => {
                    return Err(de::Error::invalid_value(
                        de::Unexpected::Str(s),
                        &Self::EXPECTING,
                    ))
                }
            }
        }

        let scale = scale.unwrap_or(0);
        Ok(Self {
            unscaled_value,
            scale,
        })
    }
}

impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DecimalVisitor;

        impl de::Visitor<'_> for DecimalVisitor {
            type Value = Decimal;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(Decimal::EXPECTING)
            }

            fn visit_u64<E: de::Error>(self, val: u64) -> Result<Self::Value, E> {
                Ok(Decimal {
                    unscaled_value: val,
                    scale: 0,
                })
            }

            fn visit_f64<E: de::Error>(self, val: f64) -> Result<Self::Value, E> {
                Decimal::from_f64(val)
            }

            fn visit_str<E: de::Error>(self, val: &str) -> Result<Self::Value, E> {
                val.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_any(DecimalVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_decimals() {
        let dec: Decimal = "1".parse().unwrap();
        assert_eq!(dec.scale(0).unwrap(), 1);
        assert_eq!(dec.to_string(), "1");

        let dec: Decimal = "1.5".parse().unwrap();
        assert_eq!(dec.scale(1).unwrap(), 15);
        assert_eq!(dec.to_string(), "1.5");

        let dec: Decimal = "1.500".parse().unwrap();
        assert_eq!(dec.scale(1).unwrap(), 15);
        assert_eq!(dec.to_string(), "1.5");

        let dec: Decimal = "1.5001".parse().unwrap();
        assert_eq!(dec.scale(6).unwrap(), 1_500_100);
        assert_eq!(dec.to_string(), "1.5001");

        let dec: Decimal = "1_001.500_1".parse().unwrap();
        assert_eq!(dec.scale(4).unwrap(), 10_015_001);
        assert_eq!(dec.to_string(), "1001.5001");
    }

    #[test]
    fn converting_decimals_from_f64() {
        let dec = Decimal::from_f64::<serde_json::Error>(0.0).unwrap();
        assert_eq!(dec.unscaled_value, 0);
        assert_eq!(dec.scale, 0);

        let dec = Decimal::from_f64::<serde_json::Error>(123.0).unwrap();
        assert_eq!(dec.unscaled_value, 123);
        assert_eq!(dec.scale, 0);

        let dec = Decimal::from_f64::<serde_json::Error>(1.23).unwrap();
        assert_eq!(dec.unscaled_value, 123);
        assert_eq!(dec.scale, 2);

        let dec = Decimal::from_f64::<serde_json::Error>(1.123_456_789_123_45).unwrap();
        assert_eq!(dec.unscaled_value, 112_345_678_912_345);
        assert_eq!(dec.scale, 14);
    }
}

#[cfg(test)]
mod prop_tests {
    use proptest::prelude::*;

    use super::*;

    fn f64_string(digit_count: u8) -> impl Strategy<Value = (String, Decimal)> {
        let digits = proptest::collection::vec(b'0'..=b'9', usize::from(digit_count));
        (digits, 0..=digit_count).prop_map(move |(s, scale)| {
            let mut s = String::from_utf8(s).unwrap();
            let unscaled_value: u64 = s.parse().unwrap();
            s.insert((digit_count - scale).into(), '.');
            let expected = Decimal {
                unscaled_value,
                scale,
            };
            (s, expected)
        })
    }

    proptest! {
        #[test]
        fn decimal_from_int_yaml(x: u64) {
            let val: Decimal = serde_yaml::from_str(&format!("{x}")).unwrap();
            prop_assert_eq!(val.scale, 0);
            prop_assert_eq!(val.unscaled_value, x);
        }

        #[test]
        fn decimal_from_f64_yaml((s, expected) in f64_string(15)) {
            let val: Decimal = serde_yaml::from_str(&s).unwrap();
            prop_assert_eq!(val, expected);
            prop_assert!(val.scale == 0 || val.unscaled_value % 10 != 0);
        }

        #[test]
        fn decimal_from_string_yaml((s, expected) in f64_string(15)) {
            let val: Decimal = serde_yaml::from_str(&format!("{s:?}")).unwrap();
            prop_assert_eq!(val, expected);
            prop_assert!(val.scale == 0 || val.unscaled_value % 10 != 0);
        }
    }
}
