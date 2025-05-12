use std::{
    any,
    collections::{BTreeMap, HashSet},
    iter,
    marker::PhantomData,
    sync::Arc,
};

pub use self::{env::Environment, json::Json, yaml::Yaml};
use crate::{
    de::{DeserializeContext, DeserializerOptions},
    error::ParseErrorCategory,
    fallback::Fallbacks,
    metadata::{BasicTypes, ConfigMetadata, ConfigTag},
    schema::{ConfigRef, ConfigSchema},
    utils::{merge_json, EnumVariant, JsonObject},
    value::{Map, Pointer, Value, ValueOrigin, WithOrigin},
    visit,
    visit::Serializer,
    DescribeConfig, DeserializeConfig, DeserializeConfigError, ParseError, ParseErrors,
};

#[macro_use]
mod macros;
mod env;
mod json;
#[cfg(test)]
mod tests;
mod yaml;

/// Contents of a [`ConfigSource`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ConfigContents {
    /// Key–value / flat configuration.
    KeyValue(Map<String>),
    /// Hierarchical configuration.
    Hierarchical(Map),
}

impl From<Map> for ConfigContents {
    fn from(map: Map) -> Self {
        Self::Hierarchical(map)
    }
}

impl From<Map<String>> for ConfigContents {
    fn from(map: Map<String>) -> Self {
        Self::KeyValue(map)
    }
}

/// Source of configuration parameters that can be added to a [`ConfigRepository`].
pub trait ConfigSource {
    /// Map of values produced by this source.
    type Map: Into<ConfigContents>;

    /// Converts this source into config contents.
    fn into_contents(self) -> WithOrigin<Self::Map>;
}

/// Wraps a hierarchical source into a prefix.
#[derive(Debug, Clone)]
pub struct Prefixed<T> {
    inner: T,
    prefix: String,
}

impl<T: ConfigSource<Map = Map>> Prefixed<T> {
    /// Wraps the provided source.
    pub fn new(inner: T, prefix: impl Into<String>) -> Self {
        Self {
            inner,
            prefix: prefix.into(),
        }
    }
}

impl<T: ConfigSource<Map = Map>> ConfigSource for Prefixed<T> {
    type Map = Map;

    fn into_contents(self) -> WithOrigin<Self::Map> {
        let contents = self.inner.into_contents();

        let origin = Arc::new(ValueOrigin::Synthetic {
            source: contents.origin.clone(),
            transform: format!("prefixed with `{}`", self.prefix),
        });

        if let Some((parent, key_in_parent)) = Pointer(&self.prefix).split_last() {
            let mut root = WithOrigin::new(Value::Object(Map::new()), origin.clone());
            root.ensure_object(parent, |_| origin.clone())
                .insert(key_in_parent.to_owned(), contents.map(Value::Object));
            root.map(|value| match value {
                Value::Object(map) => map,
                _ => unreachable!(), // guaranteed by `ensure_object`
            })
        } else {
            contents
        }
    }
}

/// Prioritized list of configuration sources. Can be used to push multiple sources at once
/// into a [`ConfigRepository`].
#[derive(Debug, Clone, Default)]
pub struct ConfigSources {
    inner: Vec<WithOrigin<ConfigContents>>,
}

impl ConfigSources {
    /// Pushes a configuration source at the end of the list.
    pub fn push(&mut self, source: impl ConfigSource) {
        self.inner.push(source.into_contents().map(Into::into));
    }
}

/// Information about a source returned from [`ConfigRepository::sources()`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SourceInfo {
    /// Origin of the source.
    pub origin: Arc<ValueOrigin>,
    /// Number of params in the source after it has undergone preprocessing (i.e., merging aliases etc.).
    pub param_count: usize,
}

/// Configuration serialization options.
#[derive(Debug, Clone, Default)]
pub struct SerializerOptions {
    pub(crate) diff_with_default: bool,
    pub(crate) secret_placeholder: Option<String>,
}

impl SerializerOptions {
    /// Will serialize only params with values differing from the default value.
    pub fn diff_with_default() -> Self {
        Self {
            diff_with_default: true,
            secret_placeholder: None,
        }
    }

    /// Sets the placeholder string value for secret params. By default, secrets will be output as-is.
    #[must_use]
    pub fn with_secret_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.secret_placeholder = Some(placeholder.into());
        self
    }

    /// Serializes a config to JSON, recursively visiting its nested configs.
    pub fn serialize<C: DescribeConfig>(self, config: &C) -> JsonObject {
        let mut visitor = Serializer::new(&C::DESCRIPTION, self);
        config.visit_config(&mut visitor);
        visitor.into_inner()
    }
}

/// Configuration repository containing zero or more [configuration sources](ConfigSource).
/// Sources are preprocessed and merged according to the provided [`ConfigSchema`].
///
/// # Merging sources
///
/// [`Self::with()`] merges a new source into this repo. The new source has higher priority and will overwrite
/// values defined in old sources, including via parameter aliases.
///
/// # Type coercion
///
/// When processing [`ConfigSource`]s, values can be *coerced* depending on the [expected type](BasicTypes)
/// at the corresponding location [as indicated](crate::de::DeserializeParam::EXPECTING) by the param deserializer.
/// Currently, coercion only happens if the original value is a string.
///
/// - If the expected type is [`BasicTypes::INTEGER`], [`BasicTypes::FLOAT`], or [`BasicTypes::BOOL`],
///   the number / Boolean is [parsed](str::parse()) from the string. If parsing succeeds, the value is replaced.
/// - If the expected type is [`BasicTypes::ARRAY`], [`BasicTypes::OBJECT`], or their union, then the original string
///   is considered to be a JSON array / object. If JSON parsing succeeds, and the parsed value has the expected shape,
///   then it replaces the original value.
///
/// Coercion is not performed if the param deserializer doesn't specify an expected type.
///
/// This means that it's possible to supply values for structured params from env vars without much hassle:
///
/// ```rust
/// # use std::collections::HashMap;
/// use smart_config::{testing, DescribeConfig, DeserializeConfig, Environment};
///
/// #[derive(Debug, DescribeConfig, DeserializeConfig)]
/// struct CoercingConfig {
///     flag: bool,
///     ints: Vec<u64>,
///     map: HashMap<String, u32>,
/// }
///
/// let env = Environment::from_iter("APP_", [
///     ("APP_FLAG", "true"),
///     ("APP_INTS", "[2, 3, 5]"),
///     ("APP_MAP", r#"{ "value": 5 }"#),
/// ]);
/// // `testing` functions create a repository internally
/// let config: CoercingConfig = testing::test(env)?;
/// assert!(config.flag);
/// assert_eq!(config.ints, [2, 3, 5]);
/// assert_eq!(config.map, HashMap::from([("value".into(), 5)]));
/// # anyhow::Ok(())
/// ```
///
/// # Other preprocessing
///
/// Besides type coercion, sources undergo a couple of additional transforms:
///
/// - **Garbage collection:** All values not corresponding to params or their ancestor objects
///   are removed.
/// - **Hiding secrets:** Values corresponding to [secret params](crate::de#secrets) are wrapped in
///   opaque, zero-on-drop wrappers.
#[derive(Debug, Clone)]
pub struct ConfigRepository<'a> {
    schema: &'a ConfigSchema,
    prefixes_for_canonical_configs: HashSet<Pointer<'a>>,
    de_options: DeserializerOptions,
    sources: Vec<SourceInfo>,
    merged: WithOrigin,
}

impl<'a> ConfigRepository<'a> {
    /// Creates an empty config repo based on the provided schema.
    pub fn new(schema: &'a ConfigSchema) -> Self {
        let prefixes_for_canonical_configs: HashSet<_> = schema
            .iter_ll()
            .flat_map(|(path, _)| path.with_ancestors())
            .chain([Pointer("")])
            .collect();

        let this = Self {
            schema,
            prefixes_for_canonical_configs,
            de_options: DeserializerOptions::default(),
            sources: vec![],
            merged: WithOrigin {
                inner: Value::Object(Map::default()),
                origin: Arc::default(),
            },
        };
        if let Some(fallbacks) = Fallbacks::new(schema) {
            this.with(fallbacks)
        } else {
            this
        }
    }

    /// Returns the wrapped configuration schema.
    pub fn schema(&self) -> &'a ConfigSchema {
        self.schema
    }

    /// Accesses options used during `serde`-powered deserialization.
    pub fn deserializer_options(&mut self) -> &mut DeserializerOptions {
        &mut self.de_options
    }

    /// Extends this environment with a new configuration source.
    #[must_use]
    pub fn with<S: ConfigSource>(mut self, source: S) -> Self {
        self.insert_inner(source.into_contents().map(Into::into));
        self
    }

    #[tracing::instrument(
        level = "debug",
        name = "ConfigRepository::insert",
        skip(self, contents)
    )]
    fn insert_inner(&mut self, contents: WithOrigin<ConfigContents>) {
        let mut source_value = match contents.inner {
            ConfigContents::KeyValue(kv) => WithOrigin::nest_kvs(kv, self.schema, &contents.origin),
            ConfigContents::Hierarchical(map) => WithOrigin {
                inner: Value::Object(map),
                origin: contents.origin.clone(),
            },
        };

        let param_count = source_value.preprocess_source(
            self.schema,
            &self.prefixes_for_canonical_configs,
            &self.de_options,
        );
        tracing::debug!(param_count, "Inserted source into config repo");
        self.merged
            .guided_merge(source_value, self.schema, Pointer(""));
        self.sources.push(SourceInfo {
            origin: contents.origin,
            param_count,
        });
    }

    ///  Extends this environment with a multiple configuration sources.
    #[must_use]
    pub fn with_all(mut self, sources: ConfigSources) -> Self {
        for contents in sources.inner {
            self.insert_inner(contents);
        }
        self
    }

    /// Provides information about sources merged in this repository.
    pub fn sources(&self) -> &[SourceInfo] {
        &self.sources
    }

    #[doc(hidden)] // not stable yet
    pub fn merged(&self) -> &WithOrigin {
        &self.merged
    }

    /// Returns canonical JSON for all configurations contained in the schema, with values filled both from the contained sources
    /// and from defaults.
    ///
    /// This method differs from [`Self::merged()`] by taking defaults into account.
    ///
    /// # Errors
    ///
    /// If parsing any of the configs in the schema fails, returns parsing errors early (i.e., errors are **not** exhaustive).
    /// Importantly, missing config / parameter errors are swallowed provided this is the only kind of errors for the config,
    /// and the corresponding config serialization is skipped.
    #[doc(hidden)] // not stable yet
    pub fn canonicalize(&self, options: &SerializerOptions) -> Result<JsonObject, ParseErrors> {
        let mut json = serde_json::Map::new();
        for config_parser in self.iter() {
            if !config_parser.config().is_top_level() {
                // The config should be serialized as a part of the parent config.
                continue;
            }

            let parsed = match config_parser.parse() {
                Ok(config) => config,
                Err(err) => {
                    if err
                        .iter()
                        .all(|err| matches!(err.category, ParseErrorCategory::MissingField))
                    {
                        tracing::debug!(
                            config = ?config_parser.config().metadata().ty,
                            prefix = config_parser.config().prefix(),
                            "Skipped config because of missing fields"
                        );
                        continue;
                    }
                    return Err(err);
                }
            };
            let metadata = config_parser.config().metadata();
            let visitor_fn = metadata.visitor;
            let mut visitor = visit::Serializer::new(metadata, options.clone());
            visitor_fn(parsed.as_ref(), &mut visitor);
            merge_json(
                &mut json,
                metadata,
                config_parser.config().prefix(),
                visitor.into_inner(),
            );
        }
        Ok(json)
    }

    /// Iterates over parsers for all configs in the schema.
    pub fn iter(&self) -> impl Iterator<Item = ConfigParser<'_, ()>> + '_ {
        self.schema.iter().map(|config_ref| ConfigParser {
            repo: self,
            config_ref,
            _config: PhantomData,
        })
    }

    /// Returns a parser for the single configuration of the specified type.
    ///
    /// # Errors
    ///
    /// Errors if the config is not a part of the schema or is mounted to multiple locations.
    pub fn single<C: DeserializeConfig>(&self) -> anyhow::Result<ConfigParser<'_, C>> {
        let config_ref = self.schema.single(&C::DESCRIPTION)?;
        Ok(ConfigParser {
            repo: self,
            config_ref,
            _config: PhantomData,
        })
    }

    /// Gets a parser for a configuration of the specified type mounted at the canonical `prefix`.
    /// If the config is not present at `prefix`, returns `None`.
    pub fn get<'s, C: DeserializeConfig>(&'s self, prefix: &'s str) -> Option<ConfigParser<'s, C>> {
        let config_ref = self.schema.get(&C::DESCRIPTION, prefix)?;
        Some(ConfigParser {
            repo: self,
            config_ref,
            _config: PhantomData,
        })
    }
}

/// Parser of configuration input in a [`ConfigRepository`].
#[derive(Debug)]
pub struct ConfigParser<'a, C> {
    repo: &'a ConfigRepository<'a>,
    config_ref: ConfigRef<'a>,
    _config: PhantomData<C>,
}

impl ConfigParser<'_, ()> {
    /// Attempts to parse the related config from the repository input. Returns the boxed parsed config.
    ///
    /// # Errors
    ///
    /// Returns parsing errors if any.
    #[doc(hidden)] // not stable yet
    #[allow(clippy::redundant_closure_for_method_calls)] // false positive because of lifetimes
    pub fn parse(&self) -> Result<Box<dyn any::Any>, ParseErrors> {
        self.with_context(|ctx| ctx.deserialize_any_config())
    }
}

impl<'a, C> ConfigParser<'a, C> {
    /// Returns a reference to the configuration.
    pub fn config(&self) -> ConfigRef<'a> {
        self.config_ref
    }

    fn with_context<R>(
        &self,
        action: impl FnOnce(DeserializeContext<'_>) -> Result<R, DeserializeConfigError>,
    ) -> Result<R, ParseErrors> {
        let mut errors = ParseErrors::default();
        let prefix = self.config_ref.prefix();
        let metadata = self.config_ref.data.metadata;
        let ctx = DeserializeContext::new(
            &self.repo.de_options,
            &self.repo.merged,
            prefix.to_owned(),
            metadata,
            &mut errors,
        );
        action(ctx).map_err(|_| {
            if errors.len() == 0 {
                errors.push(ParseError::generic(prefix.to_owned(), metadata));
            }
            errors
        })
    }
}

impl<C: DeserializeConfig> ConfigParser<'_, C> {
    /// Performs parsing.
    ///
    /// # Errors
    ///
    /// Returns errors encountered during parsing. This list of errors is as full as possible (i.e.,
    /// there is no short-circuiting on encountering an error).
    #[allow(clippy::redundant_closure_for_method_calls)] // doesn't work as an fn pointer because of the context lifetime
    pub fn parse(self) -> Result<C, ParseErrors> {
        self.with_context(|ctx| ctx.deserialize_config::<C>())
    }

    /// Parses an optional config. Returns `None` if the config object is not present (i.e., none of the config params / sub-configs
    /// are set); otherwise, tries to perform parsing.
    ///
    /// # Errors
    ///
    /// Returns errors encountered during parsing.
    pub fn parse_opt(self) -> Result<Option<C>, ParseErrors> {
        self.with_context(|ctx| {
            if ctx.current_value().is_none() {
                Ok(None)
            } else {
                ctx.deserialize_config::<C>().map(Some)
            }
        })
    }
}

impl WithOrigin {
    fn preprocess_source(
        &mut self,
        schema: &ConfigSchema,
        prefixes_for_canonical_configs: &HashSet<Pointer<'_>>,
        options: &DeserializerOptions,
    ) -> usize {
        self.copy_aliased_values(schema);
        self.mark_secrets(schema);
        if options.coerce_serde_enums {
            self.convert_serde_enums(schema);
        }
        self.nest_object_params_and_sub_configs(schema);
        self.nest_array_params(schema);
        self.collect_garbage(schema, prefixes_for_canonical_configs, Pointer(""))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn copy_aliased_values(&mut self, schema: &ConfigSchema) {
        for (prefix, config_data) in schema.iter_ll() {
            let canonical_map = match self.get(prefix).map(|val| &val.inner) {
                Some(Value::Object(map)) => Some(map),
                Some(_) => {
                    tracing::warn!(
                        prefix = prefix.0,
                        config = ?config_data.metadata.ty,
                        "canonical config location contains a non-object"
                    );
                    continue;
                }
                None => None,
            };

            let alias_maps: Vec<_> = config_data
                .aliases()
                .filter_map(|alias| {
                    let val = self.get(Pointer(alias))?;
                    Some((val.inner.as_object()?, &val.origin))
                })
                .collect();

            let (new_values, new_map_origin) = Self::copy_aliases_for_config(
                config_data.metadata,
                prefix,
                canonical_map,
                &alias_maps,
            );

            if new_values.is_empty() {
                continue;
            }

            let new_map_origin = new_map_origin.map(|source| {
                Arc::new(ValueOrigin::Synthetic {
                    source,
                    transform: format!("copy to '{prefix}' per aliasing rules"),
                })
            });
            // `unwrap()` below is safe: if there is no `current_map`, `new_values` are obtained from the alias maps,
            // meaning that `new_map_origin` has been set.
            self.ensure_object(prefix, |_| new_map_origin.clone().unwrap())
                .extend(new_values);
        }
    }

    #[must_use = "returned map should be inserted into the config"]
    fn copy_aliases_for_config(
        config: &ConfigMetadata,
        prefix: Pointer<'_>,
        canonical_map: Option<&Map>,
        alias_maps: &[(&Map, &Arc<ValueOrigin>)],
    ) -> (Map, Option<Arc<ValueOrigin>>) {
        let mut new_values = Map::new();
        let mut new_map_origin = None;
        for param in config.params {
            // Create a prioritized iterator of all candidates
            let local_candidates = canonical_map
                .into_iter()
                .flat_map(|map| param.aliases.iter().map(move |&alias| (map, alias, None)));
            let all_names = iter::once(param.name).chain(param.aliases.iter().copied());
            let alias_candidates = alias_maps.iter().flat_map(|&(map, origin)| {
                all_names.clone().map(move |name| (map, name, Some(origin)))
            });
            let all_candidates = local_candidates.chain(alias_candidates);

            for (map, name, origin) in all_candidates {
                // Copy all values in `map` that either match `name` exactly, or start with `{name}_`
                // (the latter can be used for object or array nesting).
                for (key, val) in map {
                    let canonical_key_string;
                    let canonical_key = if key == name {
                        param.name // Exact match
                    } else if let Some(key_suffix) = Self::strip_prefix(key, name) {
                        canonical_key_string = format!("{}_{key_suffix}", param.name);
                        &canonical_key_string
                    } else {
                        continue;
                    };

                    if canonical_map.is_some_and(|map| map.contains_key(canonical_key)) {
                        // Key is already present in the original map
                        continue;
                    }

                    if !new_values.contains_key(canonical_key) {
                        tracing::trace!(
                            prefix = prefix.0,
                            config = ?config.ty,
                            param = param.rust_field_name,
                            name,
                            ?origin,
                            canonical_key,
                            "copied aliased param"
                        );
                        new_values.insert(canonical_key.to_owned(), val.clone());
                        if new_map_origin.is_none() {
                            new_map_origin = origin.cloned();
                        }
                    }
                }
            }
        }

        (new_values, new_map_origin)
    }

    fn strip_prefix<'s>(s: &'s str, prefix: &str) -> Option<&'s str> {
        s.strip_prefix(prefix)?
            .strip_prefix('_')
            .filter(|suffix| !suffix.is_empty())
    }

    /// Wraps secret string values into `Value::SecretString(_)`.
    fn mark_secrets(&mut self, schema: &ConfigSchema) {
        for (prefix, config_data) in schema.iter_ll() {
            let Some(Self {
                inner: Value::Object(config_object),
                ..
            }) = self.get_mut(prefix)
            else {
                continue;
            };

            for param in config_data.metadata.params {
                if !param.type_description().contains_secrets() {
                    continue;
                }
                let Some(value) = config_object.get_mut(param.name) else {
                    continue;
                };

                if let Value::String(str) = &mut value.inner {
                    tracing::trace!(
                        prefix = prefix.0,
                        config = ?config_data.metadata.ty,
                        param = param.rust_field_name,
                        "marked param as secret"
                    );
                    str.make_secret();
                } else {
                    tracing::warn!(
                        prefix = prefix.0,
                        config = ?config_data.metadata.ty,
                        param = param.rust_field_name,
                        "param marked as secret has non-string value"
                    );
                }
            }
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn convert_serde_enums(&mut self, schema: &ConfigSchema) {
        for (prefix, config_data) in schema.iter_ll() {
            let Some(tag) = &config_data.metadata.tag else {
                continue; // Not an enum config, nothing to do.
            };
            let Some(Self {
                inner: Value::Object(config_object),
                ..
            }) = self.get_mut(prefix)
            else {
                continue;
            };

            if config_object.contains_key(tag.param.name) {
                continue; // The source contains the relevant tag.
            }

            let _span_guard = tracing::info_span!(
                "convert_serde_enum",
                config = ?config_data.metadata.ty,
                prefix = prefix.0,
                tag = tag.param.name,
            )
            .entered();

            if Self::convert_serde_enum(config_object, tag) {
                // Run local de-aliasing for the config again.
                let (new_values, _) = Self::copy_aliases_for_config(
                    config_data.metadata,
                    prefix,
                    Some(config_object),
                    &[],
                );
                config_object.extend(new_values);
            }
        }
    }

    /// Returns true if at least one field was copied. This doesn't include the inserted tag value.
    fn convert_serde_enum(config_object: &mut Map, tag: &ConfigTag) -> bool {
        let all_variant_names = tag.variants.iter().flat_map(|variant| {
            iter::once(variant.name)
                .chain(variant.aliases.iter().copied())
                .filter_map(move |name| Some((EnumVariant::new(name)?.to_snake_case(), variant)))
        });

        let mut variant_match = None;
        for (candidate_field_name, variant) in all_variant_names {
            if config_object.contains_key(&candidate_field_name) {
                if let Some((prev_field, _)) = &variant_match {
                    tracing::info!(
                        prev_field,
                        field = candidate_field_name,
                        "multiple serde-like variant fields present"
                    );
                    return false;
                }
                variant_match = Some((candidate_field_name, variant));
            }
        }

        let Some((field_name, variant)) = variant_match else {
            return false; // No matches found
        };
        let variant_content = config_object.get(&field_name).unwrap();
        if !matches!(&variant_content.inner, Value::Object(_)) {
            tracing::info!(
                field = field_name,
                "variant contents is not an object, skipping"
            );
            return false;
        }

        tracing::debug!(
            field = field_name,
            variant = variant.name,
            "copying variant contents"
        );
        let origin = ValueOrigin::Synthetic {
            source: variant_content.origin.clone(),
            transform: "coercing serde enum".to_owned(),
        };
        let Value::Object(variant_content) = variant_content.inner.clone() else {
            unreachable!(); // checked above
        };

        config_object.insert(
            tag.param.name.to_owned(),
            WithOrigin::new(variant.name.to_owned().into(), Arc::new(origin)),
        );

        let mut fields_copied = false;
        for (key, value) in variant_content {
            #[allow(clippy::map_entry)] // False positive: `key` would need to be cloned
            if config_object.contains_key(&key) {
                tracing::info!(
                    field = key,
                    "config already contains field, not overwriting it"
                );
            } else {
                config_object.insert(key, value);
                fields_copied = true;
            }
        }
        fields_copied
    }

    /// Removes all values that do not correspond to canonical params or their ancestors.
    fn collect_garbage(
        &mut self,
        schema: &ConfigSchema,
        prefixes_for_canonical_configs: &HashSet<Pointer<'_>>,
        at: Pointer<'_>,
    ) -> usize {
        if schema.contains_canonical_param(at) {
            1
        } else if prefixes_for_canonical_configs.contains(&at) {
            if let Value::Object(map) = &mut self.inner {
                let mut count = 0;
                map.retain(|key, value| {
                    let child_path = at.join(key);
                    let descendant_count = value.collect_garbage(
                        schema,
                        prefixes_for_canonical_configs,
                        Pointer(&child_path),
                    );
                    count += descendant_count;
                    descendant_count > 0
                });
                count
            } else {
                // Retain a (probably erroneous) non-object value at config location to provide more intelligent errors.
                1
            }
        } else {
            // The object is neither a param nor a config or a config ancestor; remove it.
            0
        }
    }

    /// Nests values inside matching object params or nested configs.
    ///
    /// For example, we have an object param at `test.param` and a source with a value at `test.param_ms`.
    /// This transform will copy this value to `test.param.ms` (i.e., inside the param object), provided that
    /// the source doesn't contain `test.param` or contains an object at this path.
    #[tracing::instrument(level = "debug", skip_all)]
    fn nest_object_params_and_sub_configs(&mut self, schema: &ConfigSchema) {
        for (prefix, config_data) in schema.iter_ll() {
            let Some(config_object) = self.get_mut(prefix) else {
                continue;
            };
            let config_origin = &config_object.origin;
            let Value::Object(config_object) = &mut config_object.inner else {
                continue;
            };

            let object_params = config_data.metadata.params.iter().filter_map(|param| {
                param
                    .expecting
                    .contains(BasicTypes::OBJECT)
                    .then_some(param.name)
            });
            let nested_configs = config_data
                .metadata
                .nested_configs
                .iter()
                .filter_map(|nested| (!nested.name.is_empty()).then_some(nested.name));

            for child_name in object_params.chain(nested_configs) {
                let target_object = match config_object.get(child_name) {
                    None => None,
                    Some(WithOrigin {
                        inner: Value::Object(obj),
                        ..
                    }) => Some(obj),
                    // Never overwrite non-objects with an object value.
                    Some(_) => continue,
                };

                let matching_fields: Vec<_> = config_object
                    .iter()
                    .filter_map(|(name, field)| {
                        let stripped_name = Self::strip_prefix(name, child_name)?;
                        if let Some(param_object) = target_object {
                            if param_object.contains_key(stripped_name) {
                                return None; // Never overwrite existing fields
                            }
                        }
                        Some((stripped_name.to_owned(), field.clone()))
                    })
                    .collect();
                if matching_fields.is_empty() {
                    continue;
                }

                tracing::trace!(
                    prefix = prefix.0,
                    config = ?config_data.metadata.ty,
                    child_name,
                    fields = ?matching_fields.iter().map(|(name, _)| name).collect::<Vec<_>>(),
                    "nesting for object param / config"
                );

                if !config_object.contains_key(child_name) {
                    let origin = Arc::new(ValueOrigin::Synthetic {
                        source: config_origin.clone(),
                        transform: format!("nesting for object param '{child_name}'"),
                    });
                    let val = Self::new(Value::Object(Map::new()), origin);
                    config_object.insert(child_name.to_owned(), val);
                }

                let Value::Object(target_object) =
                    &mut config_object.get_mut(child_name).unwrap().inner
                else {
                    unreachable!(); // Due to the checks above
                };
                target_object.extend(matching_fields);
            }
        }
    }

    /// Nests values inside matching array params.
    ///
    /// For example, we have an array param at `test.param` and a source with values at `test.param_0`, `test.param_1`, `test.param_2`
    /// (and no `test.param`). This transform will copy these values as a 3-element array at `test.param`.
    #[tracing::instrument(level = "debug", skip_all)]
    fn nest_array_params(&mut self, schema: &ConfigSchema) {
        for (prefix, config_data) in schema.iter_ll() {
            let Some(config_object) = self.get_mut(prefix) else {
                continue;
            };
            let config_origin = &config_object.origin;
            let Value::Object(config_object) = &mut config_object.inner else {
                continue;
            };

            for param in config_data.metadata.params {
                if !param.expecting.contains(BasicTypes::ARRAY)
                    || param.expecting.contains(BasicTypes::OBJECT)
                {
                    // If a param expects an object, a transform is ambiguous; `_${i}` suffix could be either an array index
                    // or an object key.
                    continue;
                }
                if config_object.contains_key(param.name) {
                    // Unlike objects, we never extend existing arrays.
                    continue;
                }

                let matching_fields: BTreeMap<_, _> = config_object
                    .iter()
                    .filter_map(|(name, field)| {
                        let stripped_name = Self::strip_prefix(name, param.name)?;
                        let idx: usize = stripped_name.parse().ok()?;
                        Some((idx, field.clone()))
                    })
                    .collect();
                let Some(&last_idx) = matching_fields.keys().next_back() else {
                    continue; // No matching fields
                };

                if last_idx != matching_fields.len() - 1 {
                    tracing::info!(
                        prefix = prefix.0,
                        config = ?config_data.metadata.ty,
                        param = param.rust_field_name,
                        indexes = ?matching_fields.keys().copied().collect::<Vec<_>>(),
                        "indexes for array nesting are not sequential"
                    );
                    continue;
                }

                tracing::trace!(
                    prefix = prefix.0,
                    config = ?config_data.metadata.ty,
                    param = param.rust_field_name,
                    len = matching_fields.len(),
                    "nesting for array param"
                );

                let origin = Arc::new(ValueOrigin::Synthetic {
                    source: config_origin.clone(),
                    transform: format!("nesting for array param '{}'", param.name),
                });
                let array_items = matching_fields.into_values().collect();
                let val = Self::new(Value::Array(array_items), origin);
                config_object.insert(param.name.to_owned(), val);
            }
        }
    }

    /// Nests a flat key–value map into a structured object using the provided `schema`.
    ///
    /// Has complexity `O(kvs.len() * log(n_params))`, which seems about the best possible option if `kvs` is not presorted.
    #[tracing::instrument(level = "debug", skip_all)]
    fn nest_kvs(kvs: Map<String>, schema: &ConfigSchema, source_origin: &Arc<ValueOrigin>) -> Self {
        let mut dest = Self {
            inner: Value::Object(Map::new()),
            origin: source_origin.clone(),
        };

        for (key, value) in kvs {
            let value = Self::new(value.inner.into(), value.origin);

            // Get all params with full paths matching a prefix of `key` split on one of `_`s. E.g.,
            // for `key = "very_long_prefix_value"`, we'll try "very_long_prefix_value", "very_long_prefix", ..., "very".
            // If any of these prefixes corresponds to a param, we'll nest the value to align with the param.
            // For example, if `very.long_prefix.value` is a param, we'll nest the value to `very.long_prefix.value`,
            // and if `very_long.prefix.value` is a param as well, we'll copy the value to both places.
            //
            // For prefixes, we only copy the value if the param supports objects; e.g. if `very_long.prefix` is a param,
            // then we'll copy the value to `very_long.prefix_value`.
            let mut key_prefix = key.as_str();
            while !key_prefix.is_empty() {
                for (param_path, expecting) in schema.params_with_kv_path(key_prefix) {
                    let should_copy = key_prefix == key || expecting.contains(BasicTypes::OBJECT);
                    if should_copy {
                        tracing::trace!(
                            param_path = param_path.0,
                            ?expecting,
                            key,
                            key_prefix,
                            "copied key–value entry"
                        );
                        dest.copy_kv_entry(source_origin, param_path, &key, value.clone());
                    }
                }

                key_prefix = match key_prefix.rsplit_once('_') {
                    Some((prefix, _)) => prefix,
                    None => break,
                };
            }

            // Allow for array params.
            let Some((key_prefix, maybe_idx)) = key.rsplit_once('_') else {
                continue;
            };
            if !maybe_idx.bytes().all(|ch| ch.is_ascii_digit()) {
                continue;
            }
            for (param_path, expecting) in schema.params_with_kv_path(key_prefix) {
                if expecting.contains(BasicTypes::ARRAY) && !expecting.contains(BasicTypes::OBJECT)
                {
                    dest.copy_kv_entry(source_origin, param_path, &key, value.clone());
                }
            }
        }
        dest
    }

    fn copy_kv_entry(
        &mut self,
        source_origin: &Arc<ValueOrigin>,
        param_path: Pointer<'_>,
        key: &str,
        value: WithOrigin,
    ) {
        // `unwrap()` is safe: params have non-empty paths
        let (parent, _) = param_path.split_last().unwrap();
        let field_name_start = if parent.0.is_empty() {
            parent.0.len()
        } else {
            parent.0.len() + 1 // skip `_` after the parent
        };
        let field_name = key[field_name_start..].to_owned();

        let origin = Arc::new(ValueOrigin::Synthetic {
            source: source_origin.clone(),
            transform: format!("nesting kv entries for '{param_path}'"),
        });
        self.ensure_object(parent, |_| origin.clone())
            .insert(field_name, value);
    }

    /// Deep merge stopped at params (i.e., params are always merged atomically).
    fn guided_merge(&mut self, overrides: Self, schema: &ConfigSchema, current_path: Pointer<'_>) {
        match (&mut self.inner, overrides.inner) {
            (Value::Object(this), Value::Object(other))
                if !schema.contains_canonical_param(current_path) =>
            {
                for (key, value) in other {
                    if let Some(existing_value) = this.get_mut(&key) {
                        let child_path = current_path.join(&key);
                        existing_value.guided_merge(value, schema, Pointer(&child_path));
                    } else {
                        this.insert(key, value);
                    }
                }
            }
            (this, value) => {
                *this = value;
                self.origin = overrides.origin;
            }
        }
    }
}
