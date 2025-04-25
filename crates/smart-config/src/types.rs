use std::fmt;

use compile_fmt::{clip, compile_panic};

use crate::metadata::SizeUnit;

/// A wrapper providing a clear reminder that the wrapped value represents the number of bytes.
// TODO: make generic?
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
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

impl From<u64> for ByteSize {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl ByteSize {
    /// Creates a value with the specified unit of measurement checking for overflow.
    pub const fn checked(val: u64, unit: SizeUnit) -> Option<Self> {
        match val.checked_mul(unit.bytes_in_unit()) {
            Some(val) => Some(Self(val)),
            None => None,
        }
    }

    /// Creates a value with the specified unit of measurement.
    ///
    /// # Panics
    ///
    /// Panics on overflow.
    pub const fn new(val: u64, unit: SizeUnit) -> Self {
        if let Some(size) = Self::checked(val, unit) {
            size
        } else {
            compile_panic!(
                val => compile_fmt::fmt::<u64>(), " ", unit.plural() => clip(16, ""), " does not fit into a `u64` value"
            );
        }
    }
}
