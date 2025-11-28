use std::{cmp, fmt, ops, str::FromStr};

use serde::{Deserialize, Deserializer, de};

use super::FromStrStart;

#[derive(Debug, Clone, Copy)]
struct DecimalStr<'a> {
    input: &'a str,
    mantissa: &'a str,
    exponent: Option<&'a str>,
}

impl<'a> DecimalStr<'a> {
    fn new(input: &'a str) -> (Self, &'a str) {
        #[derive(Debug, Clone, Copy)]
        enum DecimalPart {
            Mantissa { has_dot: bool },
            Exponent { is_empty: bool },
        }

        let mut part = DecimalPart::Mantissa { has_dot: false };
        let mut mantissa_len = None;
        let mut exponent_len = None;
        for (i, ch) in input.char_indices() {
            match part {
                DecimalPart::Mantissa { has_dot } => {
                    if ch.is_ascii_digit() || ch == '_' {
                        // Integer continues...
                    } else if ch == '.' && !has_dot {
                        part = DecimalPart::Mantissa { has_dot: true };
                    } else if ch == 'e' || ch == 'E' {
                        mantissa_len = Some(i);
                        part = DecimalPart::Exponent { is_empty: true };
                    } else {
                        mantissa_len = Some(i);
                        break;
                    }
                }
                DecimalPart::Exponent { is_empty } => {
                    if ch.is_ascii_digit() || (is_empty && (ch == '+' || ch == '-')) {
                        // Exponent continues...
                    } else {
                        exponent_len = Some(i);
                        break;
                    }
                    part = DecimalPart::Exponent { is_empty: false };
                }
            }
        }

        let mantissa_len = mantissa_len.unwrap_or(input.len());
        if matches!(part, DecimalPart::Exponent { .. }) && exponent_len.is_none() {
            exponent_len = Some(input.len());
        }

        let exponent = exponent_len.and_then(|len| {
            let exponent = &input[mantissa_len + 1..len];
            if exponent.is_empty() {
                exponent_len = None; // Reset for correctly computing `total_len` below
                None
            } else {
                Some(exponent)
            }
        });

        let total_len = exponent_len.unwrap_or(mantissa_len);
        let (parsed, remainder) = input.split_at(total_len);
        let this = Self {
            input: parsed,
            mantissa: &input[..mantissa_len],
            exponent,
        };
        (this, remainder)
    }
}

/// Print format for a [`Decimal`] value.
#[derive(Debug, Clone, Copy)]
enum PrintFormat {
    /// Decimal format, e.g. `100.5` or `0.00123`.
    Decimal,
    /// Exponential / scientific format, e.g. `1.005e2` or `1.23e-3`.
    Exponential,
}

/// Ad-hoc non-negative decimal value with `u64` precision.
///
/// # Why not use `f64`?
///
/// - Additional precision when parsing from ints and strings; the latter supports `i16` decimal exponents (vs -308..=308 for `f64`).
/// - Lossless conversion to integers; error on overflow and imprecise conversion.
#[derive(Clone, Copy, Default)]
pub(crate) struct Decimal {
    mantissa: u64,
    exponent: i16,
}

/// Will print small or large values in the scientific (aka exponential) format, e.g. `1.234e-9`.
/// The output never loses precision.
impl fmt::Debug for Decimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.format_generic(formatter, None)
    }
}

/// Unlike `Display` for `f64`, will print small or large values in the scientific (aka exponential) format,
/// e.g. `1.234e-9`. To always use the decimal format, use the alternate specifier (`{:#}`). The output never loses precision.
impl fmt::Display for Decimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let format = formatter.alternate().then_some(PrintFormat::Decimal);
        self.format_generic(formatter, format)
    }
}

/// Will print value in the scientific (aka exponential) format, e.g. `1.234e-9`.
/// The output never loses precision.
impl fmt::LowerExp for Decimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.format_generic(formatter, Some(PrintFormat::Exponential))
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
        Self::new(value, 0)
    }
}

impl Decimal {
    const EXPECTING: &'static str = "decimal fraction like 1.5";

    const ZERO: Self = Self {
        mantissa: 0,
        exponent: 0,
    };

    pub(crate) const fn new(mantissa: u64, exponent: i16) -> Self {
        Self { mantissa, exponent }.reduced()
    }

    /// Adjusts the exponent so that the mantissa is in 1.0..10.0, e.g. 9876e3 = 9.876e6.
    fn adjusted_exponent(self) -> i32 {
        let (mut pow10, mut digit_count) = (1_u128, 0_i32);
        while pow10 <= u128::from(self.mantissa) {
            pow10 *= 10;
            digit_count += 1;
        }
        digit_count = digit_count.max(1); // for `mantissa == 0`

        i32::from(self.exponent) + digit_count - 1
    }

    fn format_generic(
        self,
        formatter: &mut fmt::Formatter<'_>,
        format: Option<PrintFormat>,
    ) -> fmt::Result {
        /// Adjusted exponents (i.e., ones that would appear in the scientific / exponential notation `$.$$$..e$$`)
        /// that are formatted as decimals. Other exponents use the scientific format. The range is the same as for `f64`.
        const DECIMAL_EXPONENTS: ops::RangeInclusive<i32> = -4..=15;

        let reduced = self.reduced();
        match format {
            Some(PrintFormat::Decimal) => reduced.format_decimal(formatter),
            Some(PrintFormat::Exponential) | None => {
                let adjusted_exponent = reduced.adjusted_exponent();
                let use_exp_format =
                    format.is_some() || !DECIMAL_EXPONENTS.contains(&adjusted_exponent);
                if use_exp_format {
                    // Use the exponential / scientific format, e.g., `1.23e15` or `7.5e-9`.
                    Self::format_exp(formatter, reduced.mantissa, adjusted_exponent)
                } else {
                    // Use the decimal format, e.g. `123000` or `0.0123`.
                    reduced.format_decimal(formatter)
                }
            }
        }
    }

    #[allow(clippy::cast_sign_loss)] // doesn't happen due to checks
    fn format_decimal(self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
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
            let fraction_digits = (-self.exponent) as usize;
            let mantissa_str = self.mantissa.to_string();
            let (mut int, fraction) =
                mantissa_str.split_at(mantissa_str.len().saturating_sub(fraction_digits));
            if int.is_empty() {
                int = "0";
            }
            write!(formatter, "{int}.{fraction:0>fraction_digits$}")
        }
    }

    fn format_exp(
        formatter: &mut fmt::Formatter<'_>,
        mantissa: u64,
        adjusted_exponent: i32,
    ) -> fmt::Result {
        let mantissa_str = mantissa.to_string();
        let (int, fraction) = mantissa_str.split_at(1);
        let decimal_dot = if fraction.is_empty() { "" } else { "." };
        write!(
            formatter,
            "{int}{decimal_dot}{fraction}e{adjusted_exponent}"
        )
    }

    fn from_decimal_str<E: de::Error>(s: DecimalStr) -> Result<Self, E> {
        if s.mantissa.is_empty() {
            return Err(E::invalid_value(
                de::Unexpected::Str(s.input),
                &Self::EXPECTING,
            ));
        }

        let mut mantissa = 0_u64;
        let mut pow10 = Some(1_u64);
        let mut digit_count = 0;
        let mut trailing_zeros_count = 0;
        let mut has_significant_digits = false;
        let mut digits_after_dot = None::<i16>;

        for ch in s.mantissa.bytes().rev() {
            match ch {
                b'0'..=b'9' => {
                    mantissa += if ch == b'0' {
                        0
                    } else {
                        let pow10 = pow10.ok_or_else(|| E::custom("too many digits"))?;
                        u64::from(ch - b'0')
                            .checked_mul(pow10)
                            .ok_or_else(|| E::custom("too many digits"))?
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
                        return Err(E::invalid_value(
                            de::Unexpected::Str(s.input),
                            &Self::EXPECTING,
                        ));
                    }
                    digits_after_dot = Some(digit_count);
                }
                b'_' => { /* skip spacing */ }
                _ => {
                    return Err(E::invalid_value(
                        de::Unexpected::Str(s.input),
                        &Self::EXPECTING,
                    ));
                }
            }
        }

        let mut exponent = if let Some(s) = s.exponent {
            s.parse::<i16>()
                .map_err(|err| E::custom(format!("invalid exponent: {err}")))?
        } else {
            0
        };
        exponent += trailing_zeros_count;
        if let Some(digits_after_dot) = digits_after_dot {
            exponent -= digits_after_dot;
        }

        Ok(Self::new(mantissa, exponent))
    }

    /// Converts to `u64` performing rounding if necessary.
    #[allow(clippy::cast_sign_loss)] // Doesn't happen due to checks
    pub(crate) fn to_int(self) -> Option<u128> {
        if let Ok(exp) = u32::try_from(self.exponent) {
            u128::from(self.mantissa).checked_mul(10_u128.checked_pow(exp)?)
        } else {
            // `self.exponent` is negative.
            let exp = -self.exponent as u32;
            let Some(pow10) = 10_u128.checked_pow(exp) else {
                return Some(0); // The value is too small
            };

            let mut value = u128::from(self.mantissa) / pow10;
            let rem = u128::from(self.mantissa) % pow10;
            match rem.cmp(&(pow10 / 2)) {
                cmp::Ordering::Greater => value = value.checked_add(1)?,
                cmp::Ordering::Equal if value % 2 == 1 => value = value.checked_add(1)?,
                _ => { /* do nothing */ }
            }

            Some(value)
        }
    }

    /// Multiplies this number by `10^scale` and returns the integer result.
    pub(crate) fn scale(self, scale: i16) -> Result<u128, serde_json::Error> {
        let scaled = Self::new(
            self.mantissa,
            self.exponent.checked_add(scale).ok_or_else(|| {
                de::Error::custom(format!("exponent overflow multiplying {self} by 1e{scale}"))
            })?,
        );

        if scaled.exponent < 0 {
            return Err(de::Error::custom(format!(
                "{self} * 1e{scale} = {scaled} is not integer"
            )));
        }
        scaled.to_int().ok_or_else(|| {
            de::Error::custom(format!("{self} * 1e{scale} = {scaled} overflows u128"))
        })
    }

    pub(crate) fn checked_mul(self, rhs: Self) -> Option<Self> {
        const fn threshold(i: u32) -> (u128, u128) {
            assert!(i > 0 && i <= 19);
            let pow10 = 10_u128.pow(i);
            // 1 is subtracted because the last digit of `u64::MAX` (5) is odd, so it'll be rounded up on a tie
            (pow10, u64::MAX as u128 * pow10 + pow10 / 2 - 1)
        }

        #[rustfmt::skip]
        const POW10_THRESHOLDS: &[(u128, u128)] = &[
            threshold(1), threshold(2), threshold(3), threshold(4),
            threshold(5), threshold(6), threshold(7), threshold(8), threshold(9),
            threshold(10), threshold(11), threshold(12), threshold(13), threshold(14),
            threshold(15), threshold(16), threshold(17), threshold(18), threshold(19),
            (10_u128.pow(20), u128::MAX),
        ];

        // Do higher precision lossless computations first, then return `None` if they don't work out
        let mut mantissa = u128::from(self.mantissa) * u128::from(rhs.mantissa);
        let mut exp = i32::from(self.exponent) + i32::from(rhs.exponent);

        // Reduce without precision loss if possible.
        while mantissa > 0 && mantissa % 10 == 0 {
            mantissa /= 10;
            exp += 1;
        }

        if mantissa > u128::from(u64::MAX) {
            let idx = POW10_THRESHOLDS
                .binary_search_by_key(&mantissa, |(_, threshold)| *threshold)
                .unwrap_or_else(|idx| idx);
            let (pow10, _) = POW10_THRESHOLDS[idx];
            let rem = mantissa % pow10;
            mantissa /= pow10;

            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            // doesn't happen since `idx` is small
            {
                exp += idx as i32 + 1;
            }

            // Round to nearest, ties to even. There can be no overflow adding 1 to `mantissa` due to the threshold checks above.
            match rem.cmp(&(pow10 / 2)) {
                cmp::Ordering::Greater => mantissa += 1,
                cmp::Ordering::Equal if mantissa % 2 == 1 => mantissa += 1,
                _ => { /* do nothing */ }
            }
        }

        let exponent = i16::try_from(exp).ok()?;
        #[allow(clippy::cast_possible_truncation)] // Doesn't happen due to checks above
        let mantissa = mantissa as u64;
        Some(Self { mantissa, exponent })
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
        Some(Self::new(mantissa as u64, exponent))
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
    const fn reduced(mut self) -> Self {
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

impl FromStrStart for Decimal {
    fn from_str_start<E: de::Error>(input: &str) -> Result<(Option<Self>, &str), E> {
        let (dec, rem) = DecimalStr::new(input);
        let dec = if dec.input.is_empty() {
            None
        } else {
            Some(Self::from_decimal_str(dec)?)
        };
        Ok((dec, rem))
    }
}

impl FromStr for Decimal {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (dec, rem) = DecimalStr::new(s);
        if !rem.is_empty() {
            return Err(de::Error::invalid_value(
                de::Unexpected::Str(s),
                &Self::EXPECTING,
            ));
        }

        Self::from_decimal_str(dec)
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
    fn parsing_decimal_str() {
        let (dec, rem) = DecimalStr::new("1");
        assert_eq!(dec.mantissa, "1");
        assert_eq!(dec.exponent, None);
        assert_eq!(rem, "");

        let (dec, rem) = DecimalStr::new("1.");
        assert_eq!(dec.mantissa, "1.");
        assert_eq!(dec.exponent, None);
        assert_eq!(rem, "");

        let (dec, rem) = DecimalStr::new("1.03");
        assert_eq!(dec.mantissa, "1.03");
        assert_eq!(dec.exponent, None);
        assert_eq!(rem, "");

        let (dec, rem) = DecimalStr::new("1_030");
        assert_eq!(dec.mantissa, "1_030");
        assert_eq!(dec.exponent, None);
        assert_eq!(rem, "");

        let (dec, rem) = DecimalStr::new("1.030e5");
        assert_eq!(dec.mantissa, "1.030");
        assert_eq!(dec.exponent, Some("5"));
        assert_eq!(rem, "");

        let (dec, rem) = DecimalStr::new("1.030e+5");
        assert_eq!(dec.mantissa, "1.030");
        assert_eq!(dec.exponent, Some("+5"));
        assert_eq!(rem, "");

        let (dec, rem) = DecimalStr::new("1.030E-5");
        assert_eq!(dec.mantissa, "1.030");
        assert_eq!(dec.exponent, Some("-5"));
        assert_eq!(rem, "");

        let (dec, rem) = DecimalStr::new("1.030_22e-5eth");
        assert_eq!(dec.mantissa, "1.030_22");
        assert_eq!(dec.exponent, Some("-5"));
        assert_eq!(rem, "eth");

        let (dec, rem) = DecimalStr::new("1_030.0.3");
        assert_eq!(dec.mantissa, "1_030.0");
        assert_eq!(dec.exponent, None);
        assert_eq!(rem, ".3");

        let (dec, rem) = DecimalStr::new("1_030 ether");
        assert_eq!(dec.mantissa, "1_030");
        assert_eq!(dec.exponent, None);
        assert_eq!(rem, " ether");

        let (dec, rem) = DecimalStr::new("1_030ether");
        assert_eq!(dec.mantissa, "1_030");
        assert_eq!(dec.exponent, None);
        assert_eq!(rem, "ether");

        let (dec, rem) = DecimalStr::new("1_030.2eth");
        assert_eq!(dec.mantissa, "1_030.2");
        assert_eq!(dec.exponent, None);
        assert_eq!(rem, "eth");
    }

    #[test]
    fn parsing_decimals() {
        let dec: Decimal = "1".parse().unwrap();
        assert_eq!(dec.scale(0).unwrap(), 1);
        assert_eq!(dec.to_string(), "1");
        assert_eq!(format!("{dec:e}"), "1e0");

        let dec: Decimal = "1.5".parse().unwrap();
        assert_eq!(dec.scale(1).unwrap(), 15);
        assert_eq!(dec.to_string(), "1.5");
        assert_eq!(format!("{dec:e}"), "1.5e0");

        for input in ["1500", "1500.", "1500.0", "1_500.00"] {
            let dec: Decimal = input.parse().unwrap();
            assert_eq!(dec.scale(0).unwrap(), 1_500);
            assert_eq!(dec.to_string(), "1500");
            assert_eq!(format!("{dec:e}"), "1.5e3");
        }

        for input in [".15", "0.1500", "00.150", ".150_00"] {
            let dec: Decimal = input.parse().unwrap();
            assert_eq!(dec.scale(2).unwrap(), 15);
            assert_eq!(dec.to_string(), "0.15");
            assert_eq!(format!("{dec:e}"), "1.5e-1");
        }

        let dec: Decimal = "1.500".parse().unwrap();
        assert_eq!(dec.scale(1).unwrap(), 15);
        assert_eq!(dec.to_string(), "1.5");
        assert_eq!(format!("{dec:e}"), "1.5e0");

        let dec: Decimal = "1.5001".parse().unwrap();
        assert_eq!(dec.scale(6).unwrap(), 1_500_100);
        assert_eq!(dec.to_string(), "1.5001");
        assert_eq!(format!("{dec:e}"), "1.5001e0");

        let dec: Decimal = "1_001.500_1".parse().unwrap();
        assert_eq!(dec.scale(4).unwrap(), 10_015_001);
        assert_eq!(dec.to_string(), "1001.5001");
        assert_eq!(format!("{dec:e}"), "1.0015001e3");
    }

    #[test]
    fn displaying_exponential_decimals() {
        for input in ["1e20", "1.e20", "1.00e20", "100e18"] {
            let dec: Decimal = input.parse().unwrap();
            assert_eq!(dec, Decimal::new(1, 20));
            assert_eq!(dec.to_string(), "1e20");
            assert_eq!(format!("{dec:#}"), "100000000000000000000");
            assert_eq!(format!("{dec:e}"), "1e20");
        }

        for input in ["1.53e-6", "1.530e-6", "1530e-9", "0.00000153"] {
            let dec: Decimal = input.parse().unwrap();
            assert_eq!(dec, Decimal::new(153, -8));
            assert_eq!(dec.to_string(), "1.53e-6");
            assert_eq!(format!("{dec:#}"), "0.00000153");
            assert_eq!(format!("{dec:e}"), "1.53e-6");
        }
    }

    #[test]
    fn displaying_zero() {
        let dec = Decimal::default();
        assert_eq!(dec.to_string(), "0");
        assert_eq!(format!("{dec:#}"), "0");
        assert_eq!(format!("{dec:?}"), "0");
        assert_eq!(format!("{dec:e}"), "0e0");
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
        let expected_str = "9.83727295022633e-184";
        assert_eq!(dec.to_string(), expected_str);
        assert_eq!(format!("{dec:?}"), expected_str);
        assert_eq!(format!("{dec:e}"), expected_str);
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
        let expected_str = "9.83727295022633e204";
        assert_eq!(dec.to_string(), expected_str);
        assert_eq!(format!("{dec:?}"), expected_str);
        assert_eq!(format!("{dec:e}"), expected_str);

        let dec: Decimal = "98_372_729_502.263_3e194".parse().unwrap();
        assert_eq!(dec.mantissa, 983_727_295_022_633);
        assert_eq!(dec.exponent, 190);
    }

    #[test]
    fn multiplication_basics() {
        assert_eq!(
            Decimal::from(3).checked_mul(Decimal::from(5)),
            Some(Decimal::from(15))
        );
        assert_eq!(
            Decimal::from(3).checked_mul(Decimal::from(1)),
            Some(Decimal::from(3))
        );
        assert_eq!(
            Decimal::from(1).checked_mul(Decimal::from(3)),
            Some(Decimal::from(3))
        );
        assert_eq!(
            Decimal::from(0).checked_mul(Decimal::from(3)),
            Some(Decimal::from(0))
        );
        assert_eq!(
            Decimal::from(4).checked_mul(Decimal::from(0)),
            Some(Decimal::from(0))
        );

        assert_eq!(
            Decimal::new(3, -2).checked_mul(Decimal::from(5)),
            Some(Decimal::new(15, -2))
        );
        assert_eq!(
            Decimal::new(3, -2).checked_mul(Decimal::new(5, 4)),
            Some(Decimal::new(15, 2))
        );

        let x = Decimal::from(10_217_720_902_603_540_321);
        let y = Decimal::from(18_053_677_771_732_054_076);
        let product = x.checked_mul(y).unwrap();
        // The exact product is 184467440737095516153321225195018398396
        assert_eq!(product.mantissa, u64::MAX);
        assert_eq!(product.exponent, 19);

        let x = Decimal::from(2_u64.pow(19));
        let y = Decimal::from(5_u64.pow(19));
        let product = x.checked_mul(y).unwrap();
        assert_eq!(product.mantissa, 1);
        assert_eq!(product.exponent, 19);

        let x = Decimal::from(2_u64.pow(25));
        let y = Decimal::from(5_u64.pow(26) * 11);
        let product = x.checked_mul(y).unwrap();
        assert_eq!(product.mantissa, 55);
        assert_eq!(product.exponent, 25);
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

    fn test_decimal_str_roundtrip(dec: Decimal) -> Result<(), TestCaseError> {
        // All representations are lossless.
        let representations = [
            dec.to_string(),
            format!("{dec:#}"),
            format!("{dec:?}"),
            format!("{dec:e}"),
        ];
        for s in &representations {
            let parsed: Decimal = Decimal::from_str(s)?;
            prop_assert_eq!(parsed, dec, "repr: {}", s);
        }
        Ok(())
    }

    proptest! {
        #[test]
        fn decimal_from_int_yaml(x: u64) {
            let val: Decimal = serde_yaml::from_str(&format!("{x}")).unwrap();
            prop_assert_eq!(val, Decimal::from(x));
            test_decimal_str_roundtrip(val)?;
        }

        #[test]
        fn decimal_from_f64_yaml((s, expected) in f64_string(15)) {
            let val: Decimal = serde_yaml::from_str(&s).unwrap();
            prop_assert_eq!(val, expected);
            prop_assert!(val.exponent == 0 || val.mantissa % 10 != 0);
            test_decimal_str_roundtrip(val)?;
        }

        // 290 (max exponent) + 15 (digits) = 305 is roughly the `f64` precision limit
        #[test]
        fn decimal_from_small_f64_yaml((s, expected) in f64_scientific_string(15, 290)) {
            let val: Decimal = serde_yaml::from_str(&s).unwrap();
            prop_assert_eq!(val, expected);
            prop_assert!(val.exponent == 0 || val.mantissa % 10 != 0);
            test_decimal_str_roundtrip(val)?;
        }

        #[test]
        fn decimal_from_string_yaml((s, expected) in f64_string(15)) {
            let val: Decimal = serde_yaml::from_str(&format!("{s:?}")).unwrap();
            prop_assert_eq!(val, expected);
            prop_assert!(val.exponent == 0 || val.mantissa % 10 != 0);
            test_decimal_str_roundtrip(val)?;
        }

        #[test]
        fn decimal_to_string((_, x) in f64_string(15)) {
            let s = x.to_string();
            prop_assert_eq!(s.parse::<Decimal>()?, x);
        }

        #[test]
        fn lossless_multiplication(x: u32, y: u32) {
            let (x, y) = (u64::from(x), u64::from(y));
            let x_dec = Decimal::from(x);
            let y_dec = Decimal::from(y);
            prop_assert_eq!(x_dec.checked_mul(y_dec), Some(Decimal::from(x * y)));
        }

        #[test]
        fn u64_multiplication(x: u64, y: u64) {
            let x_dec = Decimal::from(x);
            let y_dec = Decimal::from(y);
            let product = x_dec.checked_mul(y_dec).unwrap();
            prop_assert!(product.exponent >= 0);

            let pow10 = 10_u128.pow(product.exponent.try_into().unwrap());
            let actual = u128::from(product.mantissa) * pow10;
            let expected = u128::from(x) * u128::from(y);
            prop_assert!(actual.abs_diff(expected) <= pow10 / 2, "{actual}, {expected}");
        }
    }
}
