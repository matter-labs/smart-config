/// Creates [`Json`](crate::Json) configuration input based on the provided list of path–value tuples.
/// This is essentially a slightly fancier / more specialized version of [`json!`](serde_json::json!).
// TODO: validate paths
#[macro_export]
macro_rules! config {
    ($($path:tt : $value:expr),* $(,)?) => {
        {
            #[allow(unused_mut)]
            let mut json = $crate::Json::empty(&::std::format!("inline config at {}:{}", file!(), line!()));
            $(
            json.merge($path, $value);
            )*
            json
        }
    };
}
