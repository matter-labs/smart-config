#![allow(clippy::enum_variant_names)]

mod decimal;

use crate::metadata::ConfigMetadata;

/// Sealed trait marker. Intentionally not re-exported publicly.
pub trait Sealed {}

/// Const-compatible array / string comparison.
pub(crate) const fn const_eq(lhs: &[u8], rhs: &[u8]) -> bool {
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

#[derive(Debug, Clone, Copy)]
enum VariantCase {
    // lowercase / uppercase are not supported because they don't provide word boundaries
    CamelCase,
    SnakeCase,
    ScreamingSnakeCase,
    KebabCase,
    ScreamingKebabCase,
}

#[derive(Debug, Clone, Copy)]
enum TargetCase {
    LowerCase,
    UpperCase,
    CamelCase,
    SnakeCase,
    ScreamingSnakeCase,
    KebabCase,
    ScreamingKebabCase,
}

impl TargetCase {
    const ALL: [Self; 7] = [
        Self::LowerCase,
        Self::UpperCase,
        Self::CamelCase,
        Self::SnakeCase,
        Self::ScreamingSnakeCase,
        Self::KebabCase,
        Self::ScreamingKebabCase,
    ];
}

#[derive(Debug)]
pub(crate) struct EnumVariant<'a> {
    raw: &'a str,
    words: Vec<&'a str>,
    #[allow(dead_code)] // used for tests
    case: VariantCase,
}

impl<'a> EnumVariant<'a> {
    pub(crate) fn new(raw: &'a str) -> Option<Self> {
        if raw.is_empty() || !raw.is_ascii() {
            return None;
        }

        let mut sep = None::<u8>;
        let mut words = vec![];
        let mut word_start = 0;
        let mut is_lowercase = true;
        let mut is_uppercase = true;
        for (pos, ch) in raw.bytes().enumerate() {
            match ch {
                b'-' | b'_' => {
                    if let Some(prev_sep) = sep {
                        if prev_sep != ch {
                            return None; // Inconsistent separator
                        }
                    }
                    if word_start == pos {
                        // Two separators in a row
                        return None;
                    }

                    sep = Some(ch);
                    let word = &raw[word_start..pos];
                    if !word.is_empty() {
                        words.push(word);
                    }
                    word_start = pos + 1;
                }
                ch if ch.is_ascii_alphanumeric() => {
                    // Part of a word.
                    if ch.is_ascii_uppercase() {
                        is_lowercase = false;
                    } else if ch.is_ascii_lowercase() {
                        is_uppercase = false;
                    }
                }
                _ => return None, // Unknown separator
            }

            if !is_lowercase && !is_uppercase && sep.is_some() {
                return None; // Mixed case + splitter
            }
        }
        let last_word = &raw[word_start..];
        if !last_word.is_empty() {
            words.push(last_word);
        }
        if words.is_empty() {
            return None; // Degenerate case like `_`
        }

        let case = match sep {
            Some(b'_') | None if is_lowercase => VariantCase::SnakeCase,
            Some(b'_') | None if is_uppercase => VariantCase::ScreamingSnakeCase,
            Some(b'-') if is_lowercase => VariantCase::KebabCase,
            Some(b'-') if is_uppercase => VariantCase::ScreamingKebabCase,
            None => {
                // Guaranteed to have mixed case at this point.
                debug_assert_eq!(words.len(), 1);

                words.clear();
                let mut word_start = 0;
                for (pos, ch) in raw.bytes().enumerate() {
                    if ch.is_ascii_uppercase() && pos > 0 {
                        words.push(&raw[word_start..pos]);
                        word_start = pos;
                    }
                }
                words.push(&raw[word_start..]);

                VariantCase::CamelCase
            }
            _ => return None, // mixed case etc.
        };

        Some(Self { raw, words, case })
    }

    fn transform(&self, to_case: TargetCase) -> String {
        let mut dest = String::new();
        let sep = match to_case {
            TargetCase::LowerCase | TargetCase::UpperCase | TargetCase::CamelCase => None,
            TargetCase::SnakeCase | TargetCase::ScreamingSnakeCase => Some('_'),
            TargetCase::KebabCase | TargetCase::ScreamingKebabCase => Some('-'),
        };
        for (i, &word) in self.words.iter().enumerate() {
            dest.push_str(&match to_case {
                TargetCase::LowerCase | TargetCase::SnakeCase | TargetCase::KebabCase => {
                    word.to_ascii_lowercase()
                }
                TargetCase::UpperCase
                | TargetCase::ScreamingSnakeCase
                | TargetCase::ScreamingKebabCase => word.to_ascii_uppercase(),
                TargetCase::CamelCase => {
                    word[..1].to_ascii_uppercase() + &word[1..].to_ascii_lowercase()
                }
            });
            let is_last = i + 1 == self.words.len();
            if let (Some(sep), false) = (sep, is_last) {
                dest.push(sep);
            }
        }
        dest
    }

    pub(crate) fn to_snake_case(&self) -> String {
        self.transform(TargetCase::SnakeCase)
    }

    // This logic can be optimized, e.g. by detecting the case in `variants`.
    pub(crate) fn try_match(&self, variants: &[&'static str]) -> Option<&'static str> {
        // First, search a complete match to provide a shortcut for the common case.
        let matching = variants.iter().copied().find(|&var| var == self.raw);
        if let Some(matching) = matching {
            return Some(matching);
        }

        for to_case in TargetCase::ALL {
            let transformed = self.transform(to_case);
            let matching = variants.iter().copied().find(|&var| var == transformed);
            if let Some(matching) = matching {
                return Some(matching);
            }
        }
        None
    }
}

pub(crate) type JsonObject = serde_json::Map<String, serde_json::Value>;

pub(crate) fn merge_json(
    mut target: &mut JsonObject,
    metadata: &ConfigMetadata,
    path: &str,
    value: JsonObject,
) {
    if !path.is_empty() {
        for segment in path.split('.') {
            if !target.contains_key(segment) {
                target.insert(segment.to_owned(), serde_json::Map::new().into());
            }

            // `unwrap()` is safe due to the check above.
            let child = target.get_mut(segment).unwrap();
            target = child.as_object_mut().unwrap_or_else(|| {
                panic!(
                    "Internal error: Attempted to merge {config_name} at '{path}', which is not an object",
                    config_name = metadata.ty.name_in_code()
                )
            });
        }
    }
    deep_merge(target, value);
}

fn deep_merge(dest: &mut JsonObject, src: JsonObject) {
    for (key, value) in src {
        if let Some(existing) = dest.get_mut(&key) {
            if let Some(existing_map) = existing.as_object_mut() {
                if let serde_json::Value::Object(value) = value {
                    deep_merge(existing_map, value);
                } else {
                    *existing = value;
                }
            } else {
                *existing = value;
            }
        } else {
            dest.insert(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn detecting_cases() {
        let variant = EnumVariant::new("snake_case10_12").unwrap();
        assert_matches!(variant.case, VariantCase::SnakeCase);
        assert_eq!(variant.words, ["snake", "case10", "12"]);

        let variant = EnumVariant::new("03_snake_case").unwrap();
        assert_matches!(variant.case, VariantCase::SnakeCase);
        assert_eq!(variant.words, ["03", "snake", "case"]);

        let variant = EnumVariant::new("SNAKE_CASE10_12").unwrap();
        assert_matches!(variant.case, VariantCase::ScreamingSnakeCase);
        assert_eq!(variant.words, ["SNAKE", "CASE10", "12"]);

        let variant = EnumVariant::new("kebab-case10-12").unwrap();
        assert_matches!(variant.case, VariantCase::KebabCase);
        assert_eq!(variant.words, ["kebab", "case10", "12"]);

        let variant = EnumVariant::new("KEBAB-CASE10-12").unwrap();
        assert_matches!(variant.case, VariantCase::ScreamingKebabCase);
        assert_eq!(variant.words, ["KEBAB", "CASE10", "12"]);

        let variant = EnumVariant::new("CamelCase10").unwrap();
        assert_matches!(variant.case, VariantCase::CamelCase);
        assert_eq!(variant.words, ["Camel", "Case10"]);

        let variant = EnumVariant::new("CamelCaseC").unwrap();
        assert_matches!(variant.case, VariantCase::CamelCase);
        assert_eq!(variant.words, ["Camel", "Case", "C"]);
    }

    #[test]
    fn detecting_case_in_single_word() {
        let variant = EnumVariant::new("snake").unwrap();
        assert_matches!(variant.case, VariantCase::SnakeCase);
        assert_eq!(variant.words, ["snake"]);

        let variant = EnumVariant::new("SNAKE").unwrap();
        assert_matches!(variant.case, VariantCase::ScreamingSnakeCase);
        assert_eq!(variant.words, ["SNAKE"]);

        let variant = EnumVariant::new("Camel").unwrap();
        assert_matches!(variant.case, VariantCase::CamelCase);
        assert_eq!(variant.words, ["Camel"]);
    }

    #[test]
    fn detecting_no_case() {
        // Not ASCII
        let variant = EnumVariant::new("змея");
        assert!(variant.is_none(), "{variant:?}");

        // Unknown separator
        let variant = EnumVariant::new("snake!case10");
        assert!(variant.is_none(), "{variant:?}");

        // Mixed separator
        let variant = EnumVariant::new("snake_case10-12");
        assert!(variant.is_none(), "{variant:?}");

        // Mixed case + separator
        let variant = EnumVariant::new("snake_Case10_12");
        assert!(variant.is_none(), "{variant:?}");
    }

    fn assert_case_transforms(variant: &EnumVariant) {
        assert_eq!(variant.transform(TargetCase::LowerCase), "snakecase10u12i");
        assert_eq!(variant.transform(TargetCase::UpperCase), "SNAKECASE10U12I");
        assert_eq!(variant.transform(TargetCase::CamelCase), "SnakeCase10U12i");
        assert_eq!(
            variant.transform(TargetCase::SnakeCase),
            "snake_case10_u12i"
        );
        assert_eq!(
            variant.transform(TargetCase::ScreamingSnakeCase),
            "SNAKE_CASE10_U12I"
        );
        assert_eq!(
            variant.transform(TargetCase::KebabCase),
            "snake-case10-u12i"
        );
        assert_eq!(
            variant.transform(TargetCase::ScreamingKebabCase),
            "SNAKE-CASE10-U12I"
        );
    }

    #[test]
    fn transforming_case() {
        let variant = EnumVariant::new("snake_case10_u12i").unwrap();
        assert_case_transforms(&variant);
        let variant = EnumVariant::new("SNAKE_CASE10_U12I").unwrap();
        assert_case_transforms(&variant);
        let variant = EnumVariant::new("snake-case10-u12i").unwrap();
        assert_case_transforms(&variant);
        let variant = EnumVariant::new("SNAKE-CASE10-U12I").unwrap();
        assert_case_transforms(&variant);
        let variant = EnumVariant::new("SnakeCase10U12i").unwrap();
        assert_case_transforms(&variant);
    }
}
