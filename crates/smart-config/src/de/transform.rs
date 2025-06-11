//! Transforms for [`Optional`](super::Optional) value deserializer.
//!
//! See `Optional` and [`KnownOptionTransform`](super::KnownOptionTransform) docs for details and examples of usage.

use crate::utils::Sealed;

/// Marker trait for [`Optional`](super::Optional) transforms. Sealed; cannot be implemented for external types.
pub trait OptionalTransform: Sealed {}

/// Default [`Optional`](super::Optional) transform. Functionally similar to [`Option::map()`],
/// requiring the delegated deserializer to return a non-optional value.
#[derive(Debug)]
pub struct Map(());

impl Sealed for Map {}
impl OptionalTransform for Map {}

/// [`Optional`](super::Optional) transform functionally similar to [`Option::and_then()`].
/// Requires the delegated deserializer to return an optional value.
#[derive(Debug)]
pub struct AndThen(());

impl Sealed for AndThen {}
impl OptionalTransform for AndThen {}
