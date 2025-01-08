/// Creates [`Json`](crate::Json) configuration input based on the provided list of path–value tuples.
/// This is essentially a slightly fancier / more specialized version of [`json!`](serde_json::json!).
///
/// A path must be a string literal, with segments separated by dots `.`. A value can be anything
/// implementing [`Serialize`](serde::Serialize). Paths in a macro cannot coincide or be embedded into each other.
/// The value produced by macro has [`Json`](crate::Json) type.
///
/// # Examples
///
/// ## Basic usage
///
/// ```
/// let json = smart_config::config!(
///     "test.int": 1,
///     "test.str": "example",
///     "test.flag": true,
///     "test.structured.array": [1, 2, 3],
/// );
/// // Will create JSON input with an object at `test` with `int`, `str`, `flag`,
/// // and `structured` fields, where `structured` is an object with `array` field.
/// ```
///
/// ## Compilation-time checks
///
/// ```compile_fail
/// let json = smart_config::config!(
///     "test": false,
///     "test.int": 123,
///     // Compilation error: Path #0 `test` is a prefix of path #1 `test.int`
/// );
/// ```
#[macro_export]
macro_rules! config {
    ($($path:tt : $value:expr),* $(,)?) => {
        {
            const _:() = {
                $crate::metadata::_private::assert_paths(&[$($path,)*]);
            };

            #[allow(unused_mut)]
            let mut json = $crate::Json::empty(&::std::format!("inline config at {}:{}", file!(), line!()));
            $(
            json.merge($path, $value);
            )*
            json
        }
    };
}
