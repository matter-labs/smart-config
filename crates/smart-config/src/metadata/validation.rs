//! Metadata validations performed in compile time.

use compile_fmt::{clip, compile_args, compile_panic, fmt, CompileArgs};

use super::{ConfigMetadata, NestedConfigMetadata, ParamMetadata};

const fn is_valid_start_name_char(ch: u8) -> bool {
    ch == b'_' || ch.is_ascii_lowercase()
}

const fn is_valid_name_char(ch: u8) -> bool {
    ch == b'_' || ch.is_ascii_lowercase() || ch.is_ascii_digit()
}

#[derive(Debug)]
enum ValidationError {
    Empty,
    NonAscii { pos: usize },
    DisallowedChar { pos: usize, ch: char },
}

type ErrorArgs = CompileArgs<100>;

impl ValidationError {
    const fn fmt(self) -> ErrorArgs {
        match self {
            Self::Empty => compile_args!(capacity: ErrorArgs::CAPACITY, "name cannot be empty"),
            Self::NonAscii { pos } => compile_args!(
                capacity: ErrorArgs::CAPACITY,
                "name contains non-ASCII chars, first at position ",
                pos => fmt::<usize>()
            ),
            Self::DisallowedChar { pos: 0, ch } => compile_args!(
                capacity: ErrorArgs::CAPACITY,
                "name starts with disallowed char '",
                ch => fmt::<char>(),
                "'; allowed chars are [_a-z]"
            ),
            Self::DisallowedChar { pos, ch } => compile_args!(
                "name contains a disallowed char '",
                ch => fmt::<char>(),
                "' at position ", pos => fmt::<usize>(),
                "; allowed chars are [_a-z0-9]"
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
            "Param name `", name => clip(32, "…"), "` is invalid: ",
            &err.fmt() => fmt::<&ErrorArgs>()
        );
    }
}

const fn const_eq(lhs: &[u8], rhs: &[u8]) -> bool {
    if lhs.len() != rhs.len() {
        return false;
    }

    let mut i = 0;
    while i < lhs.len() {
        if lhs[i] != rhs[i] {
            return false;
        }
        i += 1;
    }
    true
}

#[track_caller]
const fn assert_param_against_config(
    param_parent: &'static str,
    param: &ParamMetadata,
    config_parent: &'static str,
    config: &NestedConfigMetadata,
) {
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
