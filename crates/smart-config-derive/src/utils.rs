//! Miscellaneous utils.

use quote::quote;
use syn::{PathArguments, Type};

/// Corresponds to the type kind in the main crate. Necessary because `TypeId::of()` is not a `const fn`
/// and unlikely to get stabilized as one in the near future.
#[derive(Debug)]
pub(crate) enum TypeKind {
    Bool,
    Integer,
    Float,
    String,
    Path,
    Array,
    Object,
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
const STD_ARRAY_TYPES: &[&str] = &["Vec", "HashSet", "BTreeSet"];
const STD_MAP_TYPES: &[&str] = &["HashMap", "BTreeMap"];

impl TypeKind {
    pub fn detect(ty: &Type) -> Option<Self> {
        let ty = match ty {
            Type::Path(ty) => ty,
            Type::Array(_) => return Some(Self::Array),
            _ => return None,
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
        let args_len = match &last_segment.arguments {
            PathArguments::None => 0,
            PathArguments::AngleBracketed(args) => args.args.len(),
            PathArguments::Parenthesized(_) => return None,
        };

        if last_segment.ident == "String" && args_len == 0 {
            return Some(Self::String);
        } else if last_segment.ident == "PathBuf" && args_len == 0 {
            return Some(Self::Path);
        } else if args_len == 0
            && STD_INTEGER_TYPES
                .iter()
                .any(|&name| last_segment.ident == name)
        {
            return Some(Self::Integer);
        } else if args_len == 1
            && STD_ARRAY_TYPES
                .iter()
                .any(|&name| last_segment.ident == name)
        {
            return Some(Self::Array);
        } else if args_len == 2 && STD_MAP_TYPES.iter().any(|&name| last_segment.ident == name) {
            return Some(Self::Object);
        }
        None
    }

    pub fn to_tokens(&self, cr: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        match self {
            Self::Bool => quote!(#cr::PrimitiveType::Bool.as_type()),
            Self::Integer => quote!(#cr::PrimitiveType::Integer.as_type()),
            Self::Float => quote!(#cr::PrimitiveType::Float.as_type()),
            Self::String => quote!(#cr::PrimitiveType::String.as_type()),
            Self::Path => quote!(#cr::PrimitiveType::Path.as_type()),
            Self::Array => quote!(#cr::SchemaType::Array),
            Self::Object => quote!(#cr::SchemaType::Object),
        }
    }
}
