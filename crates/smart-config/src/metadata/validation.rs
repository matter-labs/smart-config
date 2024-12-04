//! Metadata validations performed in compile time.

use compile_fmt::{clip, clip_ascii, compile_args, compile_panic, fmt, Ascii, CompileArgs};

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
                pos => fmt::<usize>()
            ),
            Self::DisallowedChar { pos, ch, allowed } => compile_args!(
                "name contains a disallowed char '",
                ch => fmt::<char>(),
                "' at position ", pos => fmt::<usize>(),
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
            "Param name `", name => clip(32, "…"), "` is invalid: ",
            &err.fmt() => fmt::<&ErrorArgs>()
        );
    }
}

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
                "Path #", i => fmt::<usize>(), " `", path => clip(32, "…"), "` is invalid: ",
                &err.fmt() => fmt::<&ErrorArgs>()
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
                    "Path #", short_i => fmt::<usize>(), " `", short => clip(32, "…"), "` is a prefix of path #",
                    long_i => fmt::<usize>(), " `", long => clip(32, "…"), "`"
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
