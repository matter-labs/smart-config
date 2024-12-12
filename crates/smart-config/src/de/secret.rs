use secrecy::SecretString;

use super::{DeserializeContext, DeserializeParam, WellKnown};
use crate::{
    error::ErrorWithOrigin,
    metadata::{BasicTypes, ParamMetadata, TypeQualifiers},
    value::{StrValue, Value},
};

/// Deserializer for secret params. Will set the corresponding flag for [`ParamMetadata`],
/// making raw param value hidden in the debug output etc.
///
/// **Important.** The deserializer does not hide the deserialized value of the param! You are responsible
/// for doing it by selecting an appropriate param type (e.g., one that zeroizes its contents on drop).
///
/// There are 2 ways to mark a parameter as secret:
///
/// - Use a [`SecretString`] or another type that has a [`WellKnown`] secret deserializer.
/// - Set `#[config(secret)]` for the param.
///
/// # Examples
///
/// ## Basic usage
///
/// The simplest way to declare a secret param is to use `SecretString` from the `secrecy` crate.
///
/// ```
/// use secrecy::ExposeSecret;
/// # use smart_config::{testing, DescribeConfig, DeserializeConfig};
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     secret: secrecy::SecretString,
/// }
///
/// let input = smart_config::config!("secret": "correct horse battery staple");
/// let config: TestConfig = testing::test(input)?;
/// assert_eq!(config.secret.expose_secret(), "correct horse battery staple");
/// # anyhow::Ok(())
/// ```
///
/// ## Custom secret types
///
/// ```
/// use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox};
/// use serde::{Deserialize, Deserializer};
/// use smart_config::{de::Serde, testing, DescribeConfig, DeserializeConfig};
///
/// // It is generally a good idea to wrap a secret into a `SecretBox`
/// // so that it is zeroized on drop and has an opaque `Debug` representation.
/// struct NumSecret(SecretBox<u64>);
///
/// impl<'de> serde::Deserialize<'de> for NumSecret {
///     fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
///         // Deserialize a `u64` and wrap it into a secret.
///         let mut secret = SecretBox::default();
///         *secret.expose_secret_mut() = u64::deserialize(deserializer)?;
///         Ok(Self(secret))
///     }
/// }
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     // Secret values must be deserializable from a string
///     // because all secrets are strings. Because of type coercion, a `u64` deserializer
///     // will work correctly if supplied with a string, which we express
///     // through `Serde![]` args.
///     #[config(secret, with = Serde![int, str])]
///     secret: NumSecret,
/// }
///
/// let input = smart_config::config!("secret": "123");
/// let config: TestConfig = testing::test(input)?;
/// assert_eq!(*config.secret.0.expose_secret(), 123);
/// # anyhow::Ok(())
/// ```
#[derive(Debug)]
pub struct Secret<De>(pub De);

// We don't really caret about the `Secret` type param; we just need so it doesn't intersect with the generic implementation below.
impl DeserializeParam<SecretString> for Secret<String> {
    const EXPECTING: BasicTypes = BasicTypes::STRING;

    fn type_qualifiers(&self) -> TypeQualifiers {
        TypeQualifiers::secret()
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<SecretString, ErrorWithOrigin> {
        let de = ctx.current_value_deserializer(param.name)?;
        Ok(match de.value() {
            Value::String(StrValue::Secret(s)) => s.clone(),
            Value::String(StrValue::Plain(s)) => s.clone().into(),
            _ => return Err(de.invalid_type("secret string")),
        })
    }
}

impl WellKnown for SecretString {
    type Deserializer = Secret<String>;
    const DE: Self::Deserializer = Secret(String::new());
}

impl<T, De> DeserializeParam<T> for Secret<De>
where
    De: DeserializeParam<T>,
{
    const EXPECTING: BasicTypes = {
        assert!(
            De::EXPECTING.contains(BasicTypes::STRING),
            "must be able to deserialize from string"
        );
        BasicTypes::STRING
    };

    fn type_qualifiers(&self) -> TypeQualifiers {
        self.0.type_qualifiers().with_secret()
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        self.0.deserialize_param(ctx, param)
    }
}
