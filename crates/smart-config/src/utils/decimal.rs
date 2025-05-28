use std::{cmp, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, de};

/// Ad-hoc non-negative decimal value with `u64` precision.
///
/// # Why not use `f64`?
///
/// - Additional precision when parsing from ints and strings; the latter supports `i16` decimal exponents (vs -308..=308 for `f64`).
/// - Lossless conversion to integers; error on overflow and imprecise conversion.
#[derive(Clone, Copy)]
pub(crate) struct Decimal {
    mantissa: u64,
    exponent: i16,
}

impl fmt::Debug for Decimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for Decimal {
    #[allow(clippy::cast_sign_loss)] // doesn't happen due to checks
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: scientific format
        if self.exponent >= 0 {
            fmt::Display::fmt(&self.mantissa, formatter)?;
            // Pad with trailing zeros
            write!(
                formatter,
                "{:0>exponent$}",
                "",
                exponent = self.exponent as usize
            )
        } else {
            let mantissa_str = self.mantissa.to_string();
            let fraction_len = (-self.exponent) as usize;
            let (mut int, fraction) =
                mantissa_str.split_at(mantissa_str.len().saturating_sub(fraction_len));
            if int.is_empty() {
                int = "0";
            }
            write!(formatter, "{int}.{fraction:0>fraction_len$}")
        }
    }
}

impl PartialEq for Decimal {
    fn eq(&self, other: &Self) -> bool {
        if self.mantissa == 0 && other.mantissa == 0 {
            return true; // in this case we don't need to unify scales, which could lead to an overflow and a false negative
        }

        let (lesser_scaled, greater_scaled) = match self.exponent.cmp(&other.exponent) {
            cmp::Ordering::Less | cmp::Ordering::Equal => (self, other),
            cmp::Ordering::Greater => (other, self),
        };
        let exp_diff = greater_scaled.exponent.abs_diff(lesser_scaled.exponent);
        let Some(pow10) = 10_u64.checked_pow(exp_diff.into()) else {
            return false;
        };
        greater_scaled.mantissa.checked_mul(pow10) == Some(lesser_scaled.mantissa)
    }
}

impl From<u64> for Decimal {
    fn from(value: u64) -> Self {
        Self {
            mantissa: value,
            exponent: 0,
        }
        .reduced()
    }
}

impl Decimal {
    const EXPECTING: &'static str = "decimal fraction like 1.5";

    const ZERO: Self = Self {
        mantissa: 0,
        exponent: 0,
    };

    fn to_int(self) -> Option<u64> {
        let exp = u32::try_from(self.exponent).ok()?;
        self.mantissa.checked_mul(10_u64.checked_pow(exp)?)
    }

    /// Multiplies this number by `10^scale` and returns the integer result.
    pub(crate) fn scale(self, scale: i16) -> Result<u64, serde_json::Error> {
        let scaled = Self {
            mantissa: self.mantissa,
            exponent: self.exponent.checked_add(scale).ok_or_else(|| {
                de::Error::custom(format!("exponent overflow multiplying {self} by 1e{scale}"))
            })?,
        }
        .reduced();

        if scaled.exponent < 0 {
            return Err(de::Error::custom(format!(
                "{self} * 1e{scale} = {scaled} is not integer"
            )));
        }
        scaled.to_int().ok_or_else(|| {
            de::Error::custom(format!("{self} * 1e{scale} = {scaled} overflows u64"))
        })
    }

    fn from_f64<E: de::Error>(value: f64) -> Result<Self, E> {
        if value < 0.0 || value.is_infinite() || value.is_nan() || value.is_subnormal() {
            return Err(de::Error::invalid_value(
                de::Unexpected::Float(value),
                &Self::EXPECTING,
            ));
        }
        if value == 0.0 {
            return Ok(Self::ZERO);
        }

        Self::from_normal_positive_f64(value).ok_or_else(|| {
            let msg = format!(
                "precision lost converting value {value} to a decimal; quote the value to avoid precision loss"
            );
            de::Error::custom(msg)
        })
    }

    fn from_normal_positive_f64(value: f64) -> Option<Self> {
        #[allow(clippy::cast_precision_loss)] // doesn't happen
        const MAX_SAFE_INT: f64 = ((1_u64 << 53) - 1) as f64;

        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        // ^ doesn't happen; `f64::DIGITS` is small and `value.log10().floor()` is in [-308, 308].
        let exponent = value.log10().floor() as i32 - f64::DIGITS as i32 + 1;
        let exponent = i16::try_from(exponent).ok()?;

        let mantissa = (value * Self::pow10(-exponent)?).round();
        if mantissa > MAX_SAFE_INT {
            return None;
        }

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // ^ doesn't happen due to the checks above
        let this = Self {
            mantissa: mantissa as u64,
            exponent,
        };
        Some(this.reduced())
    }

    // We use a lookup table because `10.0_f64.powi(exp)` loses precision for `exp >= 33`, and
    // something like `format!("1e{exp}").parse().unwrap()` looks weird / slow.
    fn pow10(exp: i16) -> Option<f64> {
        // `1e308` is the maximum representable power of 10 for `f64`.
        #[rustfmt::skip]
        const LOOKUP: &[f64] = &[
            1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
            1e17, 1e18, 1e19, 1e20, 1e21, 1e22, 1e23, 1e24, 1e25, 1e26, 1e27, 1e28, 1e29, 1e30, 1e31,
            1e32, 1e33, 1e34, 1e35, 1e36, 1e37, 1e38, 1e39, 1e40, 1e41, 1e42, 1e43, 1e44, 1e45, 1e46,
            1e47, 1e48, 1e49, 1e50, 1e51, 1e52, 1e53, 1e54, 1e55, 1e56, 1e57, 1e58, 1e59, 1e60, 1e61,
            1e62, 1e63, 1e64, 1e65, 1e66, 1e67, 1e68, 1e69, 1e70, 1e71, 1e72, 1e73, 1e74, 1e75, 1e76,
            1e77, 1e78, 1e79, 1e80, 1e81, 1e82, 1e83, 1e84, 1e85, 1e86, 1e87, 1e88, 1e89, 1e90, 1e91,
            1e92, 1e93, 1e94, 1e95, 1e96, 1e97, 1e98, 1e99, 1e100, 1e101, 1e102, 1e103, 1e104, 1e105,
            1e106, 1e107, 1e108, 1e109, 1e110, 1e111, 1e112, 1e113, 1e114, 1e115, 1e116, 1e117, 1e118,
            1e119, 1e120, 1e121, 1e122, 1e123, 1e124, 1e125, 1e126, 1e127, 1e128, 1e129, 1e130, 1e131,
            1e132, 1e133, 1e134, 1e135, 1e136, 1e137, 1e138, 1e139, 1e140, 1e141, 1e142, 1e143, 1e144,
            1e145, 1e146, 1e147, 1e148, 1e149, 1e150, 1e151, 1e152, 1e153, 1e154, 1e155, 1e156, 1e157,
            1e158, 1e159, 1e160, 1e161, 1e162, 1e163, 1e164, 1e165, 1e166, 1e167, 1e168, 1e169, 1e170,
            1e171, 1e172, 1e173, 1e174, 1e175, 1e176, 1e177, 1e178, 1e179, 1e180, 1e181, 1e182, 1e183,
            1e184, 1e185, 1e186, 1e187, 1e188, 1e189, 1e190, 1e191, 1e192, 1e193, 1e194, 1e195, 1e196,
            1e197, 1e198, 1e199, 1e200, 1e201, 1e202, 1e203, 1e204, 1e205, 1e206, 1e207, 1e208, 1e209,
            1e210, 1e211, 1e212, 1e213, 1e214, 1e215, 1e216, 1e217, 1e218, 1e219, 1e220, 1e221, 1e222,
            1e223, 1e224, 1e225, 1e226, 1e227, 1e228, 1e229, 1e230, 1e231, 1e232, 1e233, 1e234, 1e235,
            1e236, 1e237, 1e238, 1e239, 1e240, 1e241, 1e242, 1e243, 1e244, 1e245, 1e246, 1e247, 1e248,
            1e249, 1e250, 1e251, 1e252, 1e253, 1e254, 1e255, 1e256, 1e257, 1e258, 1e259, 1e260, 1e261,
            1e262, 1e263, 1e264, 1e265, 1e266, 1e267, 1e268, 1e269, 1e270, 1e271, 1e272, 1e273, 1e274,
            1e275, 1e276, 1e277, 1e278, 1e279, 1e280, 1e281, 1e282, 1e283, 1e284, 1e285, 1e286, 1e287,
            1e288, 1e289, 1e290, 1e291, 1e292, 1e293, 1e294, 1e295, 1e296, 1e297, 1e298, 1e299, 1e300,
            1e301, 1e302, 1e303, 1e304, 1e305, 1e306, 1e307, 1e308,
        ];

        let pow10 = LOOKUP.get(usize::from(exp.unsigned_abs())).copied()?;
        Some(if exp < 0 { pow10.recip() } else { pow10 })
    }

    #[must_use]
    fn reduced(mut self) -> Self {
        if self.mantissa == 0 {
            self.exponent = 0;
            return self;
        }

        while self.mantissa % 10 == 0 {
            self.exponent += 1;
            self.mantissa /= 10;
        }
        self
    }
}

impl FromStr for Decimal {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (mantissa_str, exponent_str) = s
            .split_once(['e', 'E'])
            .map_or((s, None), |(mantissa, exponent)| (mantissa, Some(exponent)));
        if mantissa_str.is_empty() {
            return Err(de::Error::invalid_value(
                de::Unexpected::Str(s),
                &Self::EXPECTING,
            ));
        }

        let mut mantissa = 0_u64;
        let mut pow10 = Some(1_u64);
        let mut digit_count = 0;
        let mut trailing_zeros_count = 0;
        let mut has_significant_digits = false;
        let mut digits_after_dot = None::<i16>;

        for ch in mantissa_str.bytes().rev() {
            match ch {
                b'0'..=b'9' => {
                    mantissa += if ch == b'0' {
                        0
                    } else {
                        let pow10 = pow10.ok_or_else(|| de::Error::custom("too many digits"))?;
                        u64::from(ch - b'0')
                            .checked_mul(pow10)
                            .ok_or_else(|| de::Error::custom("too many digits"))?
                    };

                    digit_count += 1;
                    has_significant_digits = has_significant_digits || ch != b'0';
                    if has_significant_digits {
                        pow10 = pow10.and_then(|e| e.checked_mul(10));
                    } else {
                        trailing_zeros_count += 1;
                    }
                }
                b'.' => {
                    if digits_after_dot.is_some() {
                        return Err(de::Error::invalid_value(
                            de::Unexpected::Str(s),
                            &Self::EXPECTING,
                        ));
                    }
                    digits_after_dot = Some(digit_count);
                }
                b'_' => { /* skip spacing */ }
                _ => {
                    return Err(de::Error::invalid_value(
                        de::Unexpected::Str(s),
                        &Self::EXPECTING,
                    ));
                }
            }
        }

        let mut exponent = if let Some(s) = exponent_str {
            s.parse::<i16>()
                .map_err(|err| de::Error::custom(format!("invalid exponent: {err}")))?
        } else {
            0
        };
        exponent += trailing_zeros_count;
        if let Some(digits_after_dot) = digits_after_dot {
            exponent -= digits_after_dot;
        }

        let this = Self { mantissa, exponent };
        Ok(this.reduced())
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
                Ok(Decimal::from(val))
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

        for input in ["1500", "1500.", "1500.0", "1_500.00"] {
            let dec: Decimal = input.parse().unwrap();
            assert_eq!(dec.scale(0).unwrap(), 1_500);
            assert_eq!(dec.to_string(), "1500");
        }

        for input in [".15", "0.1500", "00.150", ".150_00"] {
            let dec: Decimal = input.parse().unwrap();
            assert_eq!(dec.scale(2).unwrap(), 15);
            assert_eq!(dec.to_string(), "0.15");
        }

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
    fn parsing_errors() {
        // No mantissa
        "e".parse::<Decimal>().unwrap_err();
        // Empty exponent
        "1e".parse::<Decimal>().unwrap_err();
        // Invalid exponent
        "1e?".parse::<Decimal>().unwrap_err();
        // Too large exponent
        "1e123456".parse::<Decimal>().unwrap_err();
        "1e-123456".parse::<Decimal>().unwrap_err();
        // Too many digits
        "123456789012345678901".parse::<Decimal>().unwrap_err();
        ".123456789012345678901".parse::<Decimal>().unwrap_err();
        "0.123456789012345678901".parse::<Decimal>().unwrap_err();
        "1.23456789012345678901e30".parse::<Decimal>().unwrap_err();
        "1.23456789012345678901e-30".parse::<Decimal>().unwrap_err();
    }

    #[test]
    fn small_decimals() {
        let dec = Decimal::from_f64::<serde_json::Error>(1.3e-30).unwrap();
        assert_eq!(dec.mantissa, 13);
        assert_eq!(dec.exponent, -31);

        let dec: Decimal = "1.3e-30".parse().unwrap();
        assert_eq!(dec.mantissa, 13);
        assert_eq!(dec.exponent, -31);

        let dec = Decimal::from_f64::<serde_json::Error>(98_372_729_502.263_3e-194).unwrap();
        assert_eq!(dec.mantissa, 983_727_295_022_633);
        assert_eq!(dec.exponent, -198);

        let dec: Decimal = "98_372_729_502.263_3e-194".parse().unwrap();
        assert_eq!(dec.mantissa, 983_727_295_022_633);
        assert_eq!(dec.exponent, -198);
    }

    #[test]
    fn large_decimals() {
        let dec = Decimal::from_f64::<serde_json::Error>(1.3e30).unwrap();
        assert_eq!(dec.mantissa, 13);
        assert_eq!(dec.exponent, 29);

        for input in ["1.3e30", "1.3e+30", "1.300e+30"] {
            let dec: Decimal = input.parse().unwrap();
            assert_eq!(dec.mantissa, 13);
            assert_eq!(dec.exponent, 29);
        }

        let dec = Decimal::from_f64::<serde_json::Error>(98_372_729_502.263_3e194).unwrap();
        assert_eq!(dec.mantissa, 983_727_295_022_633);
        assert_eq!(dec.exponent, 190);

        let dec: Decimal = "98_372_729_502.263_3e194".parse().unwrap();
        assert_eq!(dec.mantissa, 983_727_295_022_633);
        assert_eq!(dec.exponent, 190);
    }

    #[test]
    fn converting_decimals_from_f64() {
        let dec = Decimal::from_f64::<serde_json::Error>(0.0).unwrap();
        assert_eq!(dec.mantissa, 0);
        assert_eq!(dec.exponent, 0);

        let dec = Decimal::from_f64::<serde_json::Error>(123.0).unwrap();
        assert_eq!(dec.mantissa, 123);
        assert_eq!(dec.exponent, 0);

        let dec = Decimal::from_f64::<serde_json::Error>(1.23).unwrap();
        assert_eq!(dec.mantissa, 123);
        assert_eq!(dec.exponent, -2);

        let dec = Decimal::from_f64::<serde_json::Error>(1.123_456_789_123_45).unwrap();
        assert_eq!(dec.mantissa, 112_345_678_912_345);
        assert_eq!(dec.exponent, -14);
    }
}

#[cfg(test)]
mod prop_tests {
    use std::fmt::Write as _;

    use proptest::prelude::*;

    use super::*;

    #[allow(clippy::cast_possible_wrap)] // doesn't happen
    fn f64_string(digit_count: u16) -> impl Strategy<Value = (String, Decimal)> {
        let digits = proptest::collection::vec(b'0'..=b'9', usize::from(digit_count));
        (digits, 0..=digit_count).prop_map(move |(s, scale)| {
            let mut s = String::from_utf8(s).unwrap();
            let mantissa: u64 = s.parse().unwrap();
            s.insert((digit_count - scale).into(), '.');
            let expected = Decimal {
                mantissa,
                exponent: -(scale as i16),
            };
            (s, expected)
        })
    }

    fn f64_scientific_string(
        digit_count: u16,
        max_exp: u16,
    ) -> impl Strategy<Value = (String, Decimal)> {
        let first_digit = b'1'..=b'9';
        let other_digits = proptest::collection::vec(b'0'..=b'9', usize::from(digit_count - 1));
        let max_exp = i16::try_from(max_exp).unwrap();
        let digit_count = i16::try_from(digit_count).unwrap();

        (first_digit, other_digits, -max_exp..=max_exp).prop_map(
            move |(first_digit, mut digits, exp)| {
                digits.insert(0, first_digit);
                let mut s = String::from_utf8(digits).unwrap();
                let mantissa: u64 = s.parse().unwrap();
                s.insert(1, '.');
                write!(&mut s, "e{exp}").unwrap();

                let expected = Decimal {
                    mantissa,
                    exponent: exp - digit_count + 1,
                };
                (s, expected)
            },
        )
    }

    proptest! {
        #[test]
        fn decimal_from_int_yaml(x: u64) {
            let val: Decimal = serde_yaml::from_str(&format!("{x}")).unwrap();
            prop_assert_eq!(val, Decimal::from(x));
        }

        #[test]
        fn decimal_from_f64_yaml((s, expected) in f64_string(15)) {
            let val: Decimal = serde_yaml::from_str(&s).unwrap();
            prop_assert_eq!(val, expected);
            prop_assert!(val.exponent == 0 || val.mantissa % 10 != 0);
        }

        // 290 (max exponent) + 15 (digits) = 305 is roughly the `f64` precision limit
        #[test]
        fn decimal_from_small_f64_yaml((s, expected) in f64_scientific_string(15, 290)) {
            let val: Decimal = serde_yaml::from_str(&s).unwrap();
            prop_assert_eq!(val, expected);
            prop_assert!(val.exponent == 0 || val.mantissa % 10 != 0);
        }

        #[test]
        fn decimal_from_string_yaml((s, expected) in f64_string(15)) {
            let val: Decimal = serde_yaml::from_str(&format!("{s:?}")).unwrap();
            prop_assert_eq!(val, expected);
            prop_assert!(val.exponent == 0 || val.mantissa % 10 != 0);
        }

        #[test]
        fn decimal_to_string((_, x) in f64_string(15)) {
            let s = x.to_string();
            prop_assert_eq!(s.parse::<Decimal>()?, x);
        }
    }
}
