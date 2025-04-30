//! Visitor pattern for configs.

use std::{any, any::Any, mem};

use crate::{metadata::ConfigMetadata, utils::JsonObject};

/// Visitor of configuration parameters in a particular configuration.
#[doc(hidden)] // API is not stable yet
pub trait ConfigVisitor {
    /// Visits an enumeration tag in the configuration, if the config is an enumeration.
    /// Called once per configuration before any other calls.
    fn visit_tag(&mut self, variant_index: usize);

    /// Visits a parameter providing its value for inspection. This will be called for all params in a struct config,
    /// and for params associated with the active tag variant in an enum config.
    fn visit_param(&mut self, param_index: usize, value: &dyn any::Any);

    /// Visits a nested configuration. Similarly to params, this will be called for all nested / flattened configs in a struct config,
    /// and just for ones associated with the active tag variant in an enum config.
    fn visit_nested_config(&mut self, config_index: usize, config: &dyn VisitConfig);
}

/// Configuration that can be visited (e.g., to inspect its parameters in a generic way).
///
/// This is a supertrait for [`DescribeConfig`] that is automatically derived
/// via [`derive(DescribeConfig)`](macro@crate::DescribeConfig).
pub trait VisitConfig {
    /// Performs the visit.
    fn visit_config(&self, visitor: &mut dyn ConfigVisitor);
}

impl<C: VisitConfig> VisitConfig for Option<C> {
    fn visit_config(&self, visitor: &mut dyn ConfigVisitor) {
        if let Some(config) = self {
            config.visit_config(visitor);
        }
    }
}

/// Serializing [`ConfigVisitor`]. Can be used to serialize configs to the JSON object model.
#[derive(Debug)]
pub(crate) struct Serializer {
    metadata: &'static ConfigMetadata,
    json: JsonObject,
}

impl Serializer {
    /// Creates a serializer dynamically.
    pub(crate) fn new(metadata: &'static ConfigMetadata) -> Self {
        Self {
            metadata,
            json: serde_json::Map::new(),
        }
    }

    /// Unwraps the contained JSON model.
    pub(crate) fn into_inner(self) -> JsonObject {
        self.json
    }

    /// Serializes a config to JSON, recursively visiting its nested configs.
    #[cfg(test)]
    pub(crate) fn json<C: crate::DescribeConfig>(config: &C) -> JsonObject {
        let mut visitor = Self::new(&C::DESCRIPTION);
        config.visit_config(&mut visitor);
        visitor.json
    }
}

impl ConfigVisitor for Serializer {
    fn visit_tag(&mut self, variant_index: usize) {
        let tag = self.metadata.tag.unwrap();
        let tag_name = tag.param.name;
        let tag_value = tag.variants[variant_index].name;
        self.json.insert(tag_name.to_owned(), tag_value.into());
    }

    fn visit_param(&mut self, param_index: usize, value: &dyn Any) {
        let param = &self.metadata.params[param_index];
        let value = param.deserializer.serialize_param(value);
        self.json.insert(param.name.to_owned(), value);
    }

    fn visit_nested_config(&mut self, config_index: usize, config: &dyn VisitConfig) {
        let nested_metadata = &self.metadata.nested_configs[config_index];
        let prev_metadata = mem::replace(&mut self.metadata, nested_metadata.meta);

        if nested_metadata.name.is_empty() {
            config.visit_config(self);
        } else {
            let mut prev_json = mem::take(&mut self.json);
            config.visit_config(self);
            prev_json.insert(
                nested_metadata.name.to_owned(),
                mem::take(&mut self.json).into(),
            );
            self.json = prev_json;
        }

        self.metadata = prev_metadata;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::{
        metadata::ConfigMetadata,
        testonly::{DefaultingConfig, EnumConfig, NestedConfig},
        DescribeConfig,
    };

    #[derive(Debug)]
    struct PersistingVisitor {
        metadata: &'static ConfigMetadata,
        tag: Option<&'static str>,
        param_values: HashMap<&'static str, serde_json::Value>,
    }

    impl PersistingVisitor {
        fn new(metadata: &'static ConfigMetadata) -> Self {
            Self {
                metadata,
                tag: None,
                param_values: HashMap::new(),
            }
        }
    }

    impl ConfigVisitor for PersistingVisitor {
        fn visit_tag(&mut self, variant_index: usize) {
            assert!(self.tag.is_none());
            self.tag = Some(self.metadata.tag.unwrap().variants[variant_index].rust_name);
        }

        fn visit_param(&mut self, param_index: usize, value: &dyn any::Any) {
            let param = &self.metadata.params[param_index];
            let prev_value = self
                .param_values
                .insert(param.name, param.deserializer.serialize_param(value));
            assert!(
                prev_value.is_none(),
                "Param value {} is visited twice",
                param.name
            );
        }

        fn visit_nested_config(&mut self, _config_index: usize, _config: &dyn VisitConfig) {
            // Do nothing
        }
    }

    #[test]
    fn visiting_struct_config() {
        let config = DefaultingConfig::default();
        let mut visitor = PersistingVisitor::new(&DefaultingConfig::DESCRIPTION);
        config.visit_config(&mut visitor);

        assert_eq!(visitor.tag, None);
        assert_eq!(
            visitor.param_values,
            HashMap::from([
                ("float", serde_json::Value::Null),
                ("set", serde_json::json!([])),
                ("int", 12_u32.into()),
                ("url", "https://example.com/".into())
            ])
        );
    }

    #[test]
    fn visiting_enum_config() {
        let config = EnumConfig::First;
        let mut visitor = PersistingVisitor::new(&EnumConfig::DESCRIPTION);
        config.visit_config(&mut visitor);
        assert_eq!(visitor.tag, Some("First"));
        assert_eq!(visitor.param_values, HashMap::new());

        let config = EnumConfig::Nested(NestedConfig::default_nested());
        let mut visitor = PersistingVisitor::new(&EnumConfig::DESCRIPTION);
        config.visit_config(&mut visitor);
        assert_eq!(visitor.tag, Some("Nested"));
        assert_eq!(visitor.param_values, HashMap::new());

        let config = EnumConfig::WithFields {
            string: Some("test".to_owned()),
            flag: true,
            set: [1].into(),
        };
        let mut visitor = PersistingVisitor::new(&EnumConfig::DESCRIPTION);
        config.visit_config(&mut visitor);
        assert_eq!(visitor.tag, Some("WithFields"));
        assert_eq!(
            visitor.param_values,
            HashMap::from([
                ("string", "test".into()),
                ("flag", true.into()),
                ("set", serde_json::json!([1]))
            ])
        );
    }
}
