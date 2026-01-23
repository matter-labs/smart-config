//! # Derive macros reference
//!
//! All 3 proc macros exported by the library ([`DescribeConfig`](macro@crate::DescribeConfig),
//! [`DeserializeConfig`](macro@crate::DeserializeConfig) and [`ExampleConfig`](macro@crate::ExampleConfig))
//! share the same attributes detailed below.
//!
//! These macros support both structs and enums. Macro specifications are conceptually similar
//! to the `Deserialize` macro from `serde`.
//! Macro behavior can be configured with `#[config(_)]` attributes. Multiple `#[config(_)]` attributes
//! on a single item are supported.
//!
//! Each field in the struct / each enum variant is considered a configuration param (by default),
//! or a sub-config (if `#[config(nest)]` or `#[config(flatten)]` is present for the field).
//!
//! # Container attributes
//!
//! ## `validate`
//!
//! **Type:** One of the following:
//!
//! - Expression evaluating to a [`Validate`](crate::validation::Validate) implementation (e.g., a [`Range`](std::ops::Range); see the `Validate` docs
//!   for implementations). An optional human-readable string validation description may be provided delimited by the comma (e.g., to make the description
//!   more domain-specific).
//! - Pointer to a function with the `fn(&_) -> Result<(), ErrorWithOrigin>` signature and the validation description separated by a comma.
//! - Pointer to a function with the `fn(&_) -> bool` signature and the validation description separated by a comma. Validation fails
//!   if the function returns `false`.
//!
//! See the examples in the [`validation`](crate::validation) module.
//!
//! Specifies a post-deserialization validation for the config. This is useful to check invariants involving multiple params.
//! Multiple validations are supported by specifying the attribute multiple times.
//!
//! ## `tag`
//!
//! **Type:** string
//!
//! Specifies the param name holding the enum tag, similar to the corresponding attribute in `serde`.
//! Unlike `serde`, this attribute is *required* for enums; this is to ensure that source merging is well-defined.
//!
//! ## `rename_all`
//!
//! **Type:** string; one of `lowercase`, `UPPERCASE`, `camelCase`, `snake_case`, `SCREAMING_SNAKE_CASE`,
//! `kebab-case`, `SCREAMING-KEBAB-CASE`
//!
//! Renames all variants in an enum config according to the provided transform. Unlike in `serde`, this attribute
//! *only* works on enum variants. Params / sub-configs are always expected to have `snake_case` naming.
//!
//! Caveats:
//!
//! - `rename_all` assumes that original variant names are in `PascalCase` (i.e., follow Rust naming conventions).
//! - `rename_all` requires original variant names to consist of ASCII chars.
//! - Each letter of capitalized acronyms (e.g., "HTTP" in `HTTPServer`) is treated as a separate word.
//!   E.g., `rename_all = "snake_case"` will rename `HTTPServer` to `h_t_t_p_server`.
//!   Note that [it is recommended][clippy-acronyms] to not capitalize acronyms (i.e., use `HttpServer`).
//! - No spacing is inserted before numbers or other non-letter chars. E.g., `rename_all = "snake_case"`
//!   will rename `Status500` to `status500`, not to `status_500`.
//!
//! [clippy-acronyms]: https://rust-lang.github.io/rust-clippy/master/index.html#/upper_case_acronyms
//!
//! ## `derive(Default)`
//!
//! Derives `Default` according to the default values of params (+ the default variant for enum configs).
//! To work, all params must have a default value specified.
//!
//! # Variant attributes
//!
//! ## `rename`, `alias`
//!
//! **Type:** string
//!
//! Have the same meaning as in `serde`; i.e. allow to rename / specify additional names for the tag(s)
//! corresponding to the variant. `alias` can be specified multiple times.
//!
//! ## `default`
//!
//! If specified, marks the variant as default – one which will be used if the tag param is not set in the input.
//! At most one variant can be marked as default.
//!
//! # Field attributes
//!
//! ## `rename`, `alias`
//!
//! **Type:** string
//!
//! Have the same meaning as in `serde`; i.e. allow to rename / specify additional names for the param or a nested config.
//! Names are [validated](#validations) in compile time.
//!
//! In addition to simple names, *path* aliases are supported as well. A path alias starts with `.` and consists of dot-separated segments,
//! e.g. `.experimental.value` or `..value`. The paths are resolved relative to the config prefix. As in Python, more than one dot
//! at the start of the path signals that the path is relative to the parent(s) of the config.
//!
//! - `alias = ".experimental.value"` with config prefix `test` resolves to the absolute path `test.experimental.value`.
//! - `alias = "..value"` with config prefix `test.experimental` resolves to the absolute path `test.value`.
//!
//! If an alias requires more parents than is present in the config prefix, the alias is not applicable.
//! (E.g., `alias = "...value"` with config prefix `test`.)
//!
//! Path aliases are somewhat difficult to reason about, so avoid using them unless necessary.
//!
//! ## `deprecated`
//!
//! **Type:** string
//!
//! Similar to `alias`, with the difference that the alias is marked as deprecated in the schema docs,
//! and its usages are logged on the `WARN` level.
//!
//! ## `default`
//!
//! **Type:** path to function (optional)
//!
//! Has the same meaning as in `serde`, i.e. allows to specify a constructor of the default value for the param.
//! Without a value, [`Default`] is used for this purpose. Unlike `serde`, the path shouldn't be quoted.
//!
//! ## `default_t`
//!
//! **Type:** expression with param type
//!
//! Allows to specify the default typed value for the param. The provided expression doesn't need to be constant.
//!
//! ## `example`
//!
//! **Type:** expression with field type
//!
//! Allows to specify the example value for the param. The example value can be specified together with the `default` / `default_t`
//! attribute. In this case, the example value can be more "complex" than the default, to better illustrate how the configuration works.
//!
//! ## `fallback`
//!
//! **Type:** constant expression evaluating to `&'static dyn `[`FallbackSource`](crate::fallback::FallbackSource)
//!
//! Allows to provide a fallback source for the param. See the [`fallback`](crate::fallback) module docs for the discussion of fallbacks
//! and intended use cases.
//!
//! ## `with`
//!
//! **Type:** const expression implementing [`DeserializeParam`]
//!
//! Allows changing the param deserializer. See [`de`](crate::de) module docs for the overview of available deserializers.
//! For `Option`s, `with` refers to the *internal* type deserializer; it will be wrapped into an [`Optional`](crate::de::Optional) automatically.
//!
//! Note that there is an alternative: implementing [`WellKnown`](crate::de::WellKnown) for the param type.
//!
//! ## `nest`
//!
//! If specified, the field is treated as a nested sub-config rather than a param. Correspondingly, its type must
//! implement `DescribeConfig`, or wrap such a type in an `Option`.
//!
//! ## `flatten`
//!
//! If specified, the field is treated as a *flattened* sub-config rather than a param. Unlike `nest`, its params
//! will be added to the containing config instead of a separate object. The sub-config type must implement `DescribeConfig`.
//!
//! ## `validate`
//!
//! Has same semantics as [config validations](#validate), but applies to a specific config parameter.
//!
//! ## `deserialize_if`
//!
//! **Type:** same as [config validations](#validate)
//!
//! Filters an `Option`al value. This is useful to coerce semantically invalid values (e.g., empty strings for URLs)
//! to `None` in the case [automated null coercion](crate::de::Optional#encoding-nulls) doesn't apply.
//! See the [`validation`](crate::validation) module for examples of usage.
//!
//! # Validations
//!
//! The following validations are performed by the macro in compile time:
//!
//! - Param / sub-config names and aliases must be non-empty, consist of lowercase ASCII alphanumeric chars or underscore
//!   and not start with a digit (i.e., follow the `[a-z_][a-z0-9_]*` regex).
//! - Param names / aliases cannot coincide with nested config names.
//!
//! [`DeserializeParam`]: crate::de::DeserializeParam
