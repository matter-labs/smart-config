/// Constructor of [`Serde`](struct@crate::de::Serde) types / instances.
///
/// The macro accepts a comma-separated list of expected basic types from the following set: `bool`, `int`,
/// `float`, `str`, `array`, `object`. As a shortcut, `Serde![*]` signals to accept any input.
///
/// # Examples
///
/// ```
/// # use serde::{Deserialize, Deserializer};
/// # use smart_config::{de::Serde, DescribeConfig, DeserializeConfig};
/// struct ComplexType {
///     // ...
/// }
///
/// impl<'de> Deserialize<'de> for ComplexType {
///     // Complex deserialization logic...
/// # fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
/// #     unreachable!()
/// # }
/// }
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     /// Will try to deserialize any integer, string or object delegating
///     /// to the `Deserialize` impl. Will error on other inputs (e.g., arrays).
///     #[config(with = Serde![int, str, object])]
///     complex_param: ComplexType,
///     #[config(with = Serde![*])]
///     anything: serde_json::Value,
/// }
/// ```
#[macro_export]
#[allow(non_snake_case)]
macro_rules! Serde {
    (*) => {
        $crate::de::Serde::<{ $crate::metadata::BasicTypes::ANY.raw() }>
    };
    ($($expecting:tt),+ $(,)?) => {
        $crate::de::Serde::<{ $crate::basic_types!($($expecting)+) }>
    };
}

/// FIXME
#[macro_export]
#[allow(non_snake_case)]
macro_rules! Custom {
    (*) => {
        $crate::de::Custom::<_, { $crate::metadata::BasicTypes::ANY.raw() }>
    };
    ($($expecting:tt),+ $(,)?) => {
        $crate::de::Custom::<_, { $crate::basic_types!($($expecting)+) }>
    };
}

/// FIXME
#[macro_export]
macro_rules! basic_types {
    (@expand bool $($tail:tt)+) => {
        $crate::metadata::BasicTypes::BOOL.or($crate::basic_types!($($tail)+))
    };
    (@expand int $($tail:tt)+) => {
        $crate::metadata::BasicTypes::INTEGER.or($crate::basic_types!(@expand $($tail)+))
    };
    (@expand float $($tail:tt)+) => {
        $crate::metadata::BasicTypes::FLOAT.or($crate::basic_types!(@expand $($tail)+))
    };
    (@expand str $($tail:tt)+) => {
        $crate::metadata::BasicTypes::STRING.or($crate::basic_types!(@expand $($tail)+))
    };
    (@expand array $($tail:tt)+) => {
        $crate::metadata::BasicTypes::ARRAY.or($crate::basic_types!(@expand $($tail)+))
    };
    (@expand object $($tail:tt)+) => {
        $crate::metadata::BasicTypes::OBJECT.or($crate::basic_types!(@expand $($tail)+))
    };

    (@expand bool) => {
        $crate::metadata::BasicTypes::BOOL
    };
    (@expand int) => {
        $crate::metadata::BasicTypes::INTEGER
    };
    (@expand float) => {
        $crate::metadata::BasicTypes::FLOAT
    };
    (@expand str) => {
        $crate::metadata::BasicTypes::STRING
    };
    (@expand array) => {
        $crate::metadata::BasicTypes::ARRAY
    };
    (@expand object) => {
        $crate::metadata::BasicTypes::OBJECT
    };

    (*) => {
         $crate::metadata::BasicTypes::ANY.raw()
    };
    ($($expecting:tt),+ $(,)?) => {
        $crate::metadata::BasicTypes::raw($crate::basic_types!(@expand $($expecting)+))
    };
}

pub use Custom;
pub use Serde;
