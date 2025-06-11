use serde::{de::DeserializeOwned, Serialize};

use crate::{
    de::{DeserializeContext, DeserializeParam, Qualified, Serde, WellKnown, WellKnownOption},
    error::ErrorWithOrigin,
    metadata::{BasicTypes, ParamMetadata, TypeDescription},
    value::Value,
};

const HASH_DE: Qualified<Serde![str]> =
    Qualified::new(Serde![str], "hex string with optional 0x prefix");

macro_rules! impl_well_known_hash {
    ($($ty:ident),+) => {
        $(
        /// Accepts a hex string with an optional `0x` prefix.
        #[cfg_attr(docsrs, doc(cfg(feature = "primitive-types")))]
        impl WellKnown for primitive_types::$ty {
            type Deserializer = Qualified<Serde![str]>;
            const DE: Self::Deserializer = HASH_DE;
        }

        #[cfg_attr(docsrs, doc(cfg(feature = "primitive-types")))]
        impl WellKnownOption for primitive_types::$ty {}
        )+
    };
}

impl_well_known_hash!(H128, H160, H256, H384, H512, H768);

/// Hex deserializer enforcing a `0x` prefix. This prefix is not required by `U*` deserializers,
/// but the value may be ambiguous otherwise (e.g., `34` being equal to 0x34, not decimal 34).
#[derive(Debug)]
pub struct HexUintDeserializer;

// This implementation is overly general, but since the struct is private, it's OK.
impl<T: Serialize + DeserializeOwned> DeserializeParam<T> for HexUintDeserializer {
    const EXPECTING: BasicTypes = BasicTypes::STRING;

    fn describe(&self, description: &mut TypeDescription) {
        description.set_details("0x-prefixed hex number");
    }

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<T, ErrorWithOrigin> {
        let deserializer = ctx.current_value_deserializer(param.name)?;
        if let Value::String(s) = deserializer.value() {
            if !s.expose().starts_with("0x") {
                return Err(deserializer.invalid_type("0x-prefixed hex number"));
            }
        }
        T::deserialize(deserializer)
    }

    fn serialize_param(&self, param: &T) -> serde_json::Value {
        serde_json::to_value(param).expect("failed serializing value")
    }
}

macro_rules! impl_well_known_uint {
    ($($ty:ident),+) => {
        $(
        /// Accepts a hex string with an **mandatory** `0x` prefix. This prefix is required to clearly signal hex encoding
        /// so that `"34"` doesn't get mistaken for decimal 34.
        #[cfg_attr(docsrs, doc(cfg(feature = "primitive-types")))]
        impl WellKnown for primitive_types::$ty {
            type Deserializer = HexUintDeserializer;
            const DE: Self::Deserializer = HexUintDeserializer;
        }

        #[cfg_attr(docsrs, doc(cfg(feature = "primitive-types")))]
        impl WellKnownOption for primitive_types::$ty {}
        )+
    };
}

impl_well_known_uint!(U128, U256, U512);

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use primitive_types::{H160 as Address, H256, U128, U256};
    use smart_config_derive::{DescribeConfig, DeserializeConfig};

    use crate::{
        config,
        testing::{test, test_complete},
    };

    #[derive(Debug, PartialEq, DescribeConfig, DeserializeConfig)]
    #[config(crate = crate)]
    struct TestConfig {
        int: U128,
        #[config(default)]
        hash: Option<H256>,
        #[config(default)]
        addresses: Vec<Address>,
        #[config(default)]
        balances: HashMap<Address, U256>,
    }

    #[test]
    fn deserializing_values() {
        let json = config!(
            "int": "0x123",
            "hash": "fefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefe",
            "addresses": [
                "0x0000000000000000000000000000000000001234",
                "1212121212121212121212121212121212121212",
            ],
            "balances": HashMap::from([
                ("0x0000000000000000000000000000000000004321", "0x3"),
            ])
        );
        let config = test_complete::<TestConfig>(json).unwrap();
        assert_eq!(config.int, U128::from(0x123));
        assert_eq!(config.hash, Some(H256::repeat_byte(0xfe)));
        assert_eq!(
            config.addresses,
            [Address::from_low_u64_be(0x1234), Address::repeat_byte(0x12)]
        );
        assert_eq!(
            config.balances,
            HashMap::from([(Address::from_low_u64_be(0x4321), U256::from(3))])
        );
    }

    #[test]
    fn uint_prefix_error() {
        let json = config!("int": "123");
        let err = test::<TestConfig>(json).unwrap_err();
        assert_eq!(err.len(), 1);
        let err = err.first();
        assert_eq!(err.path(), "int");
        let inner = err.inner().to_string();
        assert!(
            inner.contains("invalid type") && inner.contains("0x-prefixed hex number"),
            "{inner}"
        );
    }
}
