use compile_fmt::{clip, compile_panic, fmt};

use crate::metadata::SizeUnit;

/// A wrapper providing a clear reminder that the wrapped value represents the number of bytes.
// TODO: make generic?
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ByteSize(pub u64);

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
                val => fmt::<u64>(), " ", unit.plural() => clip(16, ""), " does not fit into a `u64` value"
            );
        }
    }
}
