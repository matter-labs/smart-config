use std::{fmt, ops};

use compile_fmt::{clip, compile_panic};

use crate::metadata::{EtherUnit, SizeUnit};

/// A wrapper providing a clear reminder that the wrapped value represents the number of bytes.
///
/// # Examples
///
/// In non-const context, the most idiomatic way to produce a size is to multiply [`SizeUnit`] by `u64`:
///
/// ```
/// # use smart_config::{metadata::SizeUnit, ByteSize};
/// let size = 128 * SizeUnit::MiB;
/// assert_eq!(size, ByteSize(128 << 20));
/// ```
///
/// In const context, [`Self::new()`] may be used instead:
///
/// ```
/// # use smart_config::{metadata::SizeUnit, ByteSize};
/// const SIZE: ByteSize = ByteSize::new(128, SizeUnit::MiB);
/// assert_eq!(SIZE, ByteSize(128 << 20));
/// ```
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteSize(pub u64);

impl fmt::Debug for ByteSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for ByteSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            formatter.write_str("0 B")
        } else if self.0 % (1 << 30) == 0 {
            write!(formatter, "{} GiB", self.0 >> 30)
        } else if self.0 % (1 << 20) == 0 {
            write!(formatter, "{} MiB", self.0 >> 20)
        } else if self.0 % (1 << 10) == 0 {
            write!(formatter, "{} KiB", self.0 >> 10)
        } else {
            write!(formatter, "{} B", self.0)
        }
    }
}

macro_rules! impl_unit_conversions {
    ($ty:ident, $unit:ident) => {
        impl From<u64> for $ty {
            fn from(value: u64) -> Self {
                Self(value)
            }
        }

        impl $ty {
            /// Creates a value with the specified unit of measurement checking for overflow.
            pub const fn checked(val: u64, unit: $unit) -> Option<Self> {
                match val.checked_mul(unit.value_in_unit()) {
                    Some(val) => Some(Self(val)),
                    None => None,
                }
            }

            /// Creates a value with the specified unit of measurement.
            ///
            /// # Panics
            ///
            /// Panics on overflow.
            pub const fn new(val: u64, unit: $unit) -> Self {
                if let Some(size) = Self::checked(val, unit) {
                    size
                } else {
                    compile_panic!(
                        val => compile_fmt::fmt::<u64>(), " ", unit.as_str() => clip(16, ""), " does not fit into a `u64` value"
                    );
                }
            }

            /// Adds two byte sizes.
            pub const fn checked_add(self, rhs: Self) -> Option<Self> {
                match self.0.checked_add(rhs.0) {
                    Some(val) => Some(Self(val)),
                    None => None,
                }
            }

            /// Multiplies this size by the given factor.
            pub const fn checked_mul(self, factor: u64) -> Option<Self> {
                match self.0.checked_mul(factor) {
                    Some(val) => Some(Self(val)),
                    None => None,
                }
            }
        }

        impl From<$unit> for $ty {
            fn from(unit: $unit) -> Self {
                Self(unit.value_in_unit())
            }
        }

        /// Panics on overflow.
        impl ops::Mul<u64> for $unit {
            type Output = $ty;

            fn mul(self, rhs: u64) -> Self::Output {
                $ty::from(self)
                    .checked_mul(rhs)
                    .unwrap_or_else(|| panic!("Integer overflow getting {rhs} * {self}"))
            }
        }

        /// Panics on overflow.
        impl ops::Mul<$unit> for u64 {
            type Output = $ty;

            fn mul(self, rhs: $unit) -> Self::Output {
                $ty::from(rhs)
                    .checked_mul(self)
                    .unwrap_or_else(|| panic!("Integer overflow getting {self} * {rhs}"))
            }
        }

        /// Panics on overflow.
        impl ops::Add for $ty {
            type Output = Self;

            fn add(self, rhs: Self) -> Self::Output {
                self.checked_add(rhs)
                    .unwrap_or_else(|| panic!("Integer overflow getting {self} + {rhs}"))
            }
        }

        /// Panics on overflow.
        impl ops::Mul<u64> for $ty {
            type Output = Self;

            fn mul(self, rhs: u64) -> Self::Output {
                self.checked_mul(rhs)
                    .unwrap_or_else(|| panic!("Integer overflow getting {self} * {rhs}"))
            }
        }
    };
}

impl_unit_conversions!(ByteSize, SizeUnit);

/// A wrapper for ether amounts.
///
/// # Examples
///
/// In non-const context, the most idiomatic way to produce a size is to multiply [`EtherUnit`] by `u64`:
///
/// ```
/// # use smart_config::{metadata::EtherUnit, EtherAmount};
/// let size = 100 * EtherUnit::Gwei;
/// assert_eq!(size, EtherAmount(100_000_000_000));
/// ```
///
/// In const context, [`Self::new()`] may be used instead:
///
/// ```
/// # use smart_config::{metadata::EtherUnit, EtherAmount};
/// const SIZE: EtherAmount = EtherAmount::new(100, EtherUnit::Gwei);
/// assert_eq!(SIZE, EtherAmount(100_000_000_000));
/// ```
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EtherAmount(pub u64);

impl fmt::Debug for EtherAmount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for EtherAmount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 % EtherUnit::Ether.value_in_unit() == 0 {
            write!(
                formatter,
                "{} ether",
                self.0 / EtherUnit::Ether.value_in_unit()
            )
        } else if self.0 % EtherUnit::Gwei.value_in_unit() == 0 {
            write!(
                formatter,
                "{} gwei",
                self.0 / EtherUnit::Gwei.value_in_unit()
            )
        } else {
            write!(formatter, "{} wei", self.0)
        }
    }
}

impl_unit_conversions!(EtherAmount, EtherUnit);
