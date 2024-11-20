//! Metadata validations performed in compile time.

use compile_fmt::{clip, compile_args, compile_panic, fmt, CompileArgs};

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
