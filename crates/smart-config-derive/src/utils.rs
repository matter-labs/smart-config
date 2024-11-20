//! Miscellaneous utils.

use quote::quote;
use syn::Type;

/// Corresponds to the type kind in the main crate. Necessary because `TypeId::of()` is not a `const fn`
/// and unlikely to get stabilized as one in the near future.
#[derive(Debug)]
pub(crate) enum TypeKind {
    Bool,
    Integer,
    Float,
    String,
    Path,
}

const BUILTIN_INTEGER_TYPES: &[&str] = &[
    "u8", "i8", "u16", "i16", "u32", "i32", "u64", "i64", "u128", "i128", "usize", "isize",
];
const BUILTIN_FLOAT_TYPES: &[&str] = &["f32", "f64"];
const STD_INTEGER_TYPES: &[&str] = &[
    "NonZeroU8",
    "NonZeroI8",
    "NonZeroU16",
    "NonZeroI16",
    "NonZeroU32",
    "NonZeroI32",
    "NonZeroU64",
    "NonZeroI64",
    "NonZeroUsize",
    "NonZeroIsize",
];

impl TypeKind {
    pub fn detect(ty: &Type) -> Option<Self> {
        let Type::Path(ty) = ty else {
            return None;
        };

        if let Some(ident) = ty.path.get_ident() {
            // Only recognize built-in types if the type isn't qualified
            if ident == "bool" {
                return Some(Self::Bool);
            } else if BUILTIN_INTEGER_TYPES.iter().any(|&name| ident == name) {
                return Some(Self::Integer);
            } else if BUILTIN_FLOAT_TYPES.iter().any(|&name| ident == name) {
                return Some(Self::Float);
            }
        }

        let last_segment = ty.path.segments.last()?;
        if !last_segment.arguments.is_empty() {
            return None;
        }
        if last_segment.ident == "String" {
            return Some(Self::String);
        } else if last_segment.ident == "PathBuf" {
            return Some(Self::Path);
        } else if STD_INTEGER_TYPES
            .iter()
            .any(|&name| last_segment.ident == name)
        {
            return Some(Self::Integer);
        }
        None
    }

    pub fn to_tokens(&self, cr: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        match self {
            Self::Bool => quote!(#cr::TypeKind::Bool),
            Self::Integer => quote!(#cr::TypeKind::Integer),
            Self::Float => quote!(#cr::TypeKind::Float),
            Self::String => quote!(#cr::TypeKind::String),
            Self::Path => quote!(#cr::TypeKind::Path),
        }
    }
}
