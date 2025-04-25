//! Visitor pattern for configs.

use std::any;

/// Visitor of configuration parameters in a particular configuration.
#[doc(hidden)] // API is not stable yet
pub trait ConfigVisitor {
    /// Visits an enumeration tag in the configuration, if the config is an enumeration.
    /// Called once per configuration before any other calls.
    fn visit_tag(&mut self, variant_index: usize);

    /// Visits a parameter providing its value for inspection. This will be called for all params in a struct config,
    /// and for params associated with the active tag variant in an enum config.
    fn visit_param(&mut self, param_index: usize, value: &dyn any::Any);
}

/// Configuration that can be visited (e.g., to inspect its parameters in a generic way).
///
/// This is a supertrait for [`DescribeConfig`](crate::DescribeConfig) that should be automatically derived.
pub trait VisitConfig {
    /// Performs the visit.
    fn visit_config(&self, visitor: &mut dyn ConfigVisitor);
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
