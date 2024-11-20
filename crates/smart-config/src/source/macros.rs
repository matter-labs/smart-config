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
