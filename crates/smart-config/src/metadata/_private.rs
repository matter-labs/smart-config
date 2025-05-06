//! Metadata validations performed in compile time.

use std::{any, marker::PhantomData};

use compile_fmt::{clip, clip_ascii, compile_args, compile_panic, Ascii, CompileArgs};

use super::{ConfigMetadata, NestedConfigMetadata, ParamMetadata};
use crate::{
    de::DeserializeContext,
    utils::const_eq,
    visit::{ConfigVisitor, VisitConfig},
    DeserializeConfig, DeserializeConfigError,
};

pub type BoxedDeserializer =
    fn(DeserializeContext<'_>) -> Result<Box<dyn any::Any>, DeserializeConfigError>;

pub type BoxedVisitor = fn(&dyn any::Any, &mut dyn ConfigVisitor);

pub const fn box_config_visitor<T: VisitConfig + 'static>() -> BoxedVisitor {
    |boxed_config, visitor| {
        let config = boxed_config
            .downcast_ref::<T>()
            .expect("Internal error: visit target has incorrect type");
        config.visit_config(visitor);
    }
}

pub trait DeserializeBoxedConfig {
    fn deserialize_boxed_config(
        &self,
        ctx: DeserializeContext<'_>,
    ) -> Result<Box<dyn any::Any>, DeserializeConfigError>;
}

impl<T: DeserializeConfig> DeserializeBoxedConfig for PhantomData<T> {
    fn deserialize_boxed_config(
        &self,
        ctx: DeserializeContext<'_>,
    ) -> Result<Box<dyn any::Any>, DeserializeConfigError> {
        T::deserialize_config(ctx).map(|config| Box::new(config) as Box<dyn any::Any>)
    }
}

impl<T> DeserializeBoxedConfig for &PhantomData<T> {
    fn deserialize_boxed_config(
        &self,
        _ctx: DeserializeContext<'_>,
    ) -> Result<Box<dyn any::Any>, DeserializeConfigError> {
        Err(DeserializeConfigError::new())
    }
}

const fn is_valid_start_name_char(ch: u8) -> bool {
    ch == b'_' || ch.is_ascii_lowercase()
}

const fn is_valid_name_char(ch: u8) -> bool {
    ch == b'_' || ch.is_ascii_lowercase() || ch.is_ascii_digit()
}

#[derive(Debug, Clone, Copy)]
enum AllowedChars {
    NameStart,
    Name,
    Path,
}

impl AllowedChars {
    const fn as_str(self) -> Ascii<'static> {
        Ascii::new(match self {
            Self::NameStart => "[_a-z]",
            Self::Name => "[_a-z0-9]",
            Self::Path => "[_a-z0-9.]",
        })
    }
}

#[derive(Debug)]
enum ValidationError {
    Empty,
    NonAscii {
        pos: usize,
    },
    DisallowedChar {
        pos: usize,
        ch: char,
        allowed: AllowedChars,
    },
}

type ErrorArgs = CompileArgs<101>;

impl ValidationError {
    const fn fmt(self) -> ErrorArgs {
        match self {
            Self::Empty => compile_args!(capacity: ErrorArgs::CAPACITY, "name cannot be empty"),
            Self::NonAscii { pos } => compile_args!(
                capacity: ErrorArgs::CAPACITY,
                "name contains non-ASCII chars, first at position ",
                pos => compile_fmt::fmt::<usize>()
            ),
            Self::DisallowedChar { pos, ch, allowed } => compile_args!(
                "name contains a disallowed char '",
                ch => compile_fmt::fmt::<char>(),
                "' at position ", pos => compile_fmt::fmt::<usize>(),
                "; allowed chars are ",
                allowed.as_str() => clip_ascii(10, "")
            ),
        }
    }
}

const fn validate_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::Empty);
    }

    let name_bytes = name.as_bytes();
    let mut pos = 0;
    while pos < name.len() {
        if name_bytes[pos] > 127 {
            return Err(ValidationError::NonAscii { pos });
        }
        let ch = name_bytes[pos];
        let is_disallowed = (pos == 0 && !is_valid_start_name_char(ch)) || !is_valid_name_char(ch);
        if is_disallowed {
            return Err(ValidationError::DisallowedChar {
                pos,
                ch: ch as char,
                allowed: if pos == 0 {
                    AllowedChars::NameStart
                } else {
                    AllowedChars::Name
                },
            });
        }
        pos += 1;
    }
    Ok(())
}

/// Checks that a param name is valid.
#[track_caller]
pub const fn assert_param_name(name: &str) {
    if let Err(err) = validate_name(name) {
        compile_panic!(
            "Param / config name `", name => clip(32, "…"), "` is invalid: ",
            &err.fmt() => compile_fmt::fmt::<&ErrorArgs>()
        );
    }
}

#[track_caller]
const fn assert_param_against_config(
    param_parent: &'static str,
    param: &ParamMetadata,
    config_parent: &'static str,
    config: &NestedConfigMetadata,
) {
    let mut param_i = 0;
    while param_i <= param.aliases.len() {
        let param_name = if param_i == 0 {
            param.name
        } else {
            param.aliases[param_i - 1]
        };

        let mut config_i = 0;
        while config_i <= config.aliases.len() {
            let config_name = if config_i == 0 {
                config.name
            } else {
                config.aliases[config_i - 1]
            };

            if const_eq(param_name.as_bytes(), config_name.as_bytes()) {
                compile_panic!(
                    "Name / alias `", param_name => clip(32, "…"), "` of param `",
                    param_parent => clip(32, "…"), ".",
                    param.rust_field_name  => clip(32, "…"),
                    "` coincides with a name / alias of a nested config `",
                    config_parent => clip(32, "…"), ".",
                    config.rust_field_name  => clip(32, "…"),
                    "`. This is an unconditional error; \
                    config deserialization relies on the fact that configs never coincide with params"
                );
            }

            config_i += 1;
        }
        param_i += 1;
    }

    if const_eq(param.name.as_bytes(), config.name.as_bytes()) {
        compile_panic!(
            "Name `", param.name => clip(32, "…"), "` of param `",
            param_parent => clip(32, "…"), ".",
            param.rust_field_name  => clip(32, "…"),
            "` coincides with a name of a nested config `",
            config_parent => clip(32, "…"), ".",
            config.rust_field_name  => clip(32, "…"),
            "`. This is an unconditional error; \
            config deserialization relies on the fact that configs never coincide with params"
        );
    }

    let mut alias_i = 0;
    while alias_i < param.aliases.len() {
        let alias = param.aliases[alias_i];
        if const_eq(alias.as_bytes(), config.name.as_bytes()) {
            compile_panic!(
                "Alias `", alias => clip(32, "…"), "` of param `",
                param_parent => clip(32, "…"), ".",
                param.rust_field_name  => clip(32, "…"),
                "` coincides with a name of a nested config `",
                config_parent => clip(32, "…"), ".",
                config.rust_field_name  => clip(32, "…"),
                "`. This is an unconditional error; \
                config deserialization relies on the fact that configs never coincide with params"
            );
        }
        alias_i += 1;
    }
}

#[track_caller]
const fn assert_param_name_is_not_a_config(
    param_parent: &'static str,
    param: &ParamMetadata,
    config: &ConfigMetadata,
) {
    let mut config_i = 0;
    while config_i < config.nested_configs.len() {
        let nested = &config.nested_configs[config_i];
        if nested.name.is_empty() {
            // Flattened config; recurse.
            assert_param_name_is_not_a_config(param_parent, param, nested.meta);
        } else {
            assert_param_against_config(param_parent, param, config.ty.name_in_code(), nested);
        }
        config_i += 1;
    }
}

#[track_caller]
const fn assert_config_name_is_not_a_param(
    config_parent: &'static str,
    config: &NestedConfigMetadata,
    configs: &[NestedConfigMetadata],
) {
    let mut config_i = 0;
    while config_i < configs.len() {
        let flattened = &configs[config_i];
        if flattened.name.is_empty() {
            let param_parent = flattened.meta.ty.name_in_code();
            let params = flattened.meta.params;
            let mut param_i = 0;
            while param_i < params.len() {
                assert_param_against_config(param_parent, &params[param_i], config_parent, config);
                param_i += 1;
            }

            // Recurse into the next level.
            assert_config_name_is_not_a_param(config_parent, config, flattened.meta.nested_configs);
        }
        config_i += 1;
    }
}

impl ConfigMetadata {
    #[track_caller]
    pub const fn assert_valid(&self) {
        // Check that param names don't coincide with nested config names (both params and nested configs include
        // ones through flattened configs, recursively). Having both a param and a config bound to the same location
        // doesn't logically make sense, and accounting for it would make merging / deserialization logic unreasonably complex.
        self.assert_params_are_not_configs();
        self.assert_configs_are_not_params();
    }

    #[track_caller]
    const fn assert_params_are_not_configs(&self) {
        let mut param_i = 0;
        while param_i < self.params.len() {
            assert_param_name_is_not_a_config(self.ty.name_in_code(), &self.params[param_i], self);
            param_i += 1;
        }
    }

    #[track_caller]
    const fn assert_configs_are_not_params(&self) {
        let mut config_i = 0;
        while config_i < self.nested_configs.len() {
            let config = &self.nested_configs[config_i];
            if !config.name.is_empty() {
                assert_config_name_is_not_a_param(
                    self.ty.name_in_code(),
                    config,
                    self.nested_configs,
                );
            }
            config_i += 1;
        }
    }
}

// TODO: validate param types (non-empty intersection)

const fn validate_path(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::Empty);
    }

    let name_bytes = name.as_bytes();
    let mut pos = 0;
    let mut is_segment_start = true;
    while pos < name.len() {
        if name_bytes[pos] > 127 {
            return Err(ValidationError::NonAscii { pos });
        }
        let ch = name_bytes[pos];

        let is_disallowed = (is_segment_start && !is_valid_start_name_char(ch))
            || (ch != b'.' && !is_valid_name_char(ch));
        if is_disallowed {
            return Err(ValidationError::DisallowedChar {
                pos,
                ch: ch as char,
                allowed: if is_segment_start {
                    AllowedChars::NameStart
                } else {
                    AllowedChars::Path
                },
            });
        }

        is_segment_start = ch == b'.';
        pos += 1;
    }
    Ok(())
}

const fn have_prefix_relation(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut i = 0;
    while i < a.len() && i < b.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }

    if a.len() == b.len() {
        true
    } else {
        (a.len() < b.len() && b[a.len()] == b'.') || (b.len() < a.len() && a[b.len()] == b'.')
    }
}

/// Asserts config paths for the `config!` macro.
#[track_caller]
pub const fn assert_paths(paths: &[&str]) {
    // First, validate each path in isolation.
    let mut i = 0;
    while i < paths.len() {
        let path = paths[i];
        if let Err(err) = validate_path(path) {
            compile_panic!(
                "Path #", i => compile_fmt::fmt::<usize>(), " `", path => clip(32, "…"), "` is invalid: ",
                &err.fmt() => compile_fmt::fmt::<&ErrorArgs>()
            );
        }
        i += 1;
    }

    let mut i = 0;
    while i + 1 < paths.len() {
        let path = paths[i];
        let mut j = i + 1;
        while j < paths.len() {
            let other_path = paths[j];
            if have_prefix_relation(path, other_path) {
                let (short_i, short, long_i, long) = if path.len() < other_path.len() {
                    (i, path, j, other_path)
                } else {
                    (j, other_path, i, path)
                };

                compile_panic!(
                    "Path #", short_i => compile_fmt::fmt::<usize>(), " `", short => clip(32, "…"), "` is a prefix of path #",
                    long_i => compile_fmt::fmt::<usize>(), " `", long => clip(32, "…"), "`"
                );
            }
            j += 1;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn validating_paths() {
        validate_path("test").unwrap();
        validate_path("long.test_path._with_3_segments").unwrap();

        assert_matches!(
            validate_path("test.pa!th").unwrap_err(),
            ValidationError::DisallowedChar { .. }
        );
        assert_matches!(
            validate_path("test.3").unwrap_err(),
            ValidationError::DisallowedChar { .. }
        );
        assert_matches!(
            validate_path("test..path").unwrap_err(),
            ValidationError::DisallowedChar { .. }
        );
    }

    #[test]
    fn checking_prefix_relations() {
        assert!(have_prefix_relation("test", "test.path"));
        assert!(have_prefix_relation("test.path", "test"));
        assert!(have_prefix_relation("test.path", "test.path"));

        assert!(!have_prefix_relation("test.path", "test_path"));
        assert!(!have_prefix_relation("test", "test_path"));
    }
}
