//! Visitor pattern for configs.

use std::{any, any::Any, mem};

use crate::{SerializerOptions, metadata::ConfigMetadata, utils::JsonObject, value::Pointer};

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

    /// Visits an optional nested configuration.
    ///
    /// The default implementation calls [`Self::visit_nested_config()`] if `config` is `Some(_)`,
    /// and does nothing if it is `None`.
    fn visit_nested_opt_config(&mut self, config_index: usize, config: Option<&dyn VisitConfig>) {
        if let Some(config) = config {
            self.visit_nested_config(config_index, config);
        }
    }
}

/// Configuration that can be visited (e.g., to inspect its parameters in a generic way).
///
/// This is a supertrait for [`DescribeConfig`](trait@crate::DescribeConfig) that is automatically derived
/// via [`derive(DescribeConfig)`](macro@crate::DescribeConfig).
pub trait VisitConfig {
    /// Performs the visit.
    fn visit_config(&self, visitor: &mut dyn ConfigVisitor);
}

/// Serializing [`ConfigVisitor`]. Can be used to serialize configs to the JSON object model.
#[derive(Debug)]
pub(crate) struct Serializer {
    metadata: &'static ConfigMetadata,
    // Only filled when serializing into a flat object.
    current_prefix: Option<String>,
    json: JsonObject,
    options: SerializerOptions,
}

impl Serializer {
    /// Creates a serializer dynamically.
    pub(crate) fn new(
        metadata: &'static ConfigMetadata,
        prefix: &str,
        options: SerializerOptions,
    ) -> Self {
        Self {
            metadata,
            current_prefix: options.flat.then(|| prefix.to_owned()),
            json: serde_json::Map::new(),
            options,
        }
    }

    /// Unwraps the contained JSON model.
    pub(crate) fn into_inner(self) -> JsonObject {
        self.json
    }

    fn insert(&mut self, param_name: &str, value: serde_json::Value) {
        let key = if let Some(prefix) = &self.current_prefix {
            Pointer(prefix).join(param_name)
        } else {
            param_name.to_owned()
        };
        self.json.insert(key, value);
    }
}

impl ConfigVisitor for Serializer {
    fn visit_tag(&mut self, variant_index: usize) {
        let tag = self.metadata.tag.unwrap();
        let tag_variant = &tag.variants[variant_index];

        let should_insert = !self.options.diff_with_default
            || tag
                .default_variant
                .is_none_or(|default_variant| default_variant.rust_name != tag_variant.rust_name);
        if should_insert {
            self.insert(tag.param.name, tag_variant.name.into());
        }
    }

    fn visit_param(&mut self, param_index: usize, value: &dyn Any) {
        let param = &self.metadata.params[param_index];
        // TODO: this exposes secret values, but we cannot easily avoid serialization because of `should_insert` filtering below.
        let mut value = param.deserializer.serialize_param(value);

        // If a parameter has a fallback, it should be inserted regardless of whether it has the default value;
        // otherwise, since fallbacks have higher priority than defaults, the parameter value may be unexpected after parsing
        // the produced JSON.
        let should_insert = !self.options.diff_with_default
            || param.fallback.is_some()
            || param.default_value_json().as_ref() != Some(&value);
        if should_insert {
            if let (Some(placeholder), true) = (
                &self.options.secret_placeholder,
                param.type_description().contains_secrets(),
            ) {
                value = placeholder.clone().into();
            }
            self.insert(param.name, value);
        }
    }

    fn visit_nested_config(&mut self, config_index: usize, config: &dyn VisitConfig) {
        let nested_metadata = &self.metadata.nested_configs[config_index];
        let prev_metadata = mem::replace(&mut self.metadata, nested_metadata.meta);

        if nested_metadata.name.is_empty() {
            config.visit_config(self);
        } else if let Some(prefix) = &mut self.current_prefix {
            let new_prefix = Pointer(prefix).join(nested_metadata.name);
            let prev_prefix = mem::replace(prefix, new_prefix);
            config.visit_config(self);
            self.current_prefix = Some(prev_prefix);
        } else {
            let mut prev_json = mem::take(&mut self.json);
            config.visit_config(self);

            let nested_json = mem::take(&mut self.json);
            let should_insert = !self.options.diff_with_default || !nested_json.is_empty();
            if should_insert {
                prev_json.insert(nested_metadata.name.to_owned(), nested_json.into());
            }
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
        DescribeConfig,
        metadata::ConfigMetadata,
        testonly::{ConfigWithNesting, DefaultingConfig, EnumConfig, NestedConfig, SimpleEnum},
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

    #[test]
    fn serializing_config() {
        let config = EnumConfig::WithFields {
            string: Some("test".to_owned()),
            flag: true,
            set: [1].into(),
        };
        let json = SerializerOptions::default().serialize(&config);
        assert_eq!(
            serde_json::Value::from(json),
            serde_json::json!({
                "type": "WithFields",
                "string": "test",
                "flag": true,
                "set": [1]
            })
        );
    }

    #[test]
    fn serializing_nested_config() {
        let config = ConfigWithNesting {
            value: 23,
            merged: String::new(),
            nested: NestedConfig {
                simple_enum: SimpleEnum::First,
                other_int: 42,
                map: HashMap::new(),
            },
        };

        let json = SerializerOptions::default().serialize(&config);
        assert_eq!(
            serde_json::Value::from(json),
            serde_json::json!({
                "value": 23,
                "merged": "",
                "nested": {
                    "renamed": "first",
                    "other_int": 42,
                    "map": {},
                },
            })
        );

        let json = SerializerOptions::diff_with_default().serialize(&config);
        assert_eq!(
            serde_json::Value::from(json),
            serde_json::json!({
                "value": 23,
                "nested": {
                    "renamed": "first",
                },
            })
        );

        let json = SerializerOptions::default().flat(true).serialize(&config);
        assert_eq!(
            serde_json::Value::from(json),
            serde_json::json!({
                "value": 23,
                "merged": "",
                "nested.renamed": "first",
                "nested.other_int": 42,
                "nested.map": {},
            })
        );

        let json = SerializerOptions::diff_with_default()
            .flat(true)
            .serialize(&config);
        assert_eq!(
            serde_json::Value::from(json),
            serde_json::json!({
                "value": 23,
                "nested.renamed": "first",
            })
        );
    }
}
