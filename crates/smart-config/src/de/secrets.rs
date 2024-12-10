use secrecy::SecretString;

use super::{DeserializeContext, DeserializeParam, WellKnown};
use crate::{
    error::ErrorWithOrigin,
    metadata::{BasicTypes, ParamMetadata, TypeQualifiers},
    value::Value,
};

#[derive(Debug)]
pub struct Secret<De>(pub De);

impl DeserializeParam<SecretString> for Secret<()> {
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
            Value::SecretString(s) => s.clone(),
            Value::String(s) => s.clone().into(),
            _ => return Err(de.invalid_type("secret string")),
        })
    }
}

impl WellKnown for SecretString {
    type Deserializer = Secret<()>;
    const DE: Self::Deserializer = Secret(());
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
