use secrecy::SecretString;

use super::{DeserializeContext, DeserializeParam, WellKnown};
use crate::{
    error::ErrorWithOrigin,
    metadata::{BasicTypes, ParamMetadata, TypeQualifiers},
    value::{StrValue, Value},
};

/// FIXME
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
