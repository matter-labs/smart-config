//! Miscellaneous utils.

use std::collections::HashSet;

use proc_macro2::Ident;
use quote::{quote, quote_spanned};
use syn::{
    spanned::Spanned, Attribute, Data, DataEnum, DataStruct, DeriveInput, Expr, Field, Fields,
    GenericArgument, Lit, LitStr, Path, PathArguments, Type, TypePath,
};

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

    pub fn to_tokens(&self, meta_mod: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        match self {
            Self::Bool => quote!(#meta_mod::PrimitiveType::Bool.as_type()),
            Self::Integer => quote!(#meta_mod::PrimitiveType::Integer.as_type()),
            Self::Float => quote!(#meta_mod::PrimitiveType::Float.as_type()),
            Self::String => quote!(#meta_mod::PrimitiveType::String.as_type()),
            Self::Path => quote!(#meta_mod::PrimitiveType::Path.as_type()),
            Self::Array => quote!(#meta_mod::SchemaType::Array),
            Self::Object => quote!(#meta_mod::SchemaType::Object),
        }
    }
}

fn parse_docs(attrs: &[Attribute]) -> String {
    let doc_lines = attrs.iter().filter_map(|attr| {
        if attr.meta.path().is_ident("doc") {
            let name_value = attr.meta.require_name_value().ok()?;
            let Expr::Lit(doc_literal) = &name_value.value else {
                return None;
            };
            match &doc_literal.lit {
                Lit::Str(doc_literal) => Some(doc_literal.value()),
                _ => None,
            }
        } else {
            None
        }
    });

    let mut docs = String::new();
    for line in doc_lines {
        let line = line.trim();
        if line.is_empty() {
            if !docs.is_empty() {
                // New paragraph; convert it to a new line.
                docs.push('\n');
            }
        } else {
            if !docs.is_empty() && !docs.ends_with(|ch: char| ch.is_ascii_whitespace()) {
                docs.push(' ');
            }
            docs.push_str(line);
        }
    }
    docs
}

pub(crate) struct ContainerAttrs {
    pub rename_all: Option<LitStr>,
    pub tag: Option<LitStr>,
}

impl ContainerAttrs {
    fn new(attrs: &[Attribute]) -> syn::Result<Self> {
        let serde_attrs = attrs.iter().filter(|attr| attr.path().is_ident("serde"));

        let mut rename_all = None;
        let mut tag = None;
        for attr in serde_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename_all") {
                    rename_all = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("tag") {
                    tag = Some(meta.value()?.parse()?);
                    Ok(())
                } else {
                    Err(meta
                        .error("Unsupported attribute; only `rename_all` and `tag` are supported`"))
                }
            })?;
        }
        Ok(Self { rename_all, tag })
    }

    fn validate_for_struct(&self) -> syn::Result<()> {
        if let Some(rename_all) = &self.rename_all {
            let msg = "`rename_all` attribute must not be used on struct configs";
            return Err(syn::Error::new(rename_all.span(), msg));
        }
        if let Some(tag) = &self.tag {
            let msg = "`tag` attribute must not be used on struct configs";
            return Err(syn::Error::new(tag.span(), msg));
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct ConfigFieldAttrs {
    pub rename: Option<String>,
    pub aliases: Vec<String>,
    pub default: Option<Option<Path>>,
    pub flatten: bool,
    pub nest: bool,
    pub kind: Option<Expr>,
}

impl ConfigFieldAttrs {
    fn new(attrs: &[Attribute], name_span: proc_macro2::Span) -> syn::Result<Self> {
        let config_attrs = attrs.iter().filter(|attr| attr.path().is_ident("config"));

        let mut rename = None;
        let mut aliases = vec![];
        let mut default = None;
        let mut flatten = false;
        let mut nest = false;
        let mut nested_span = None;
        let mut kind = None;
        for attr in config_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    let s: LitStr = meta.value()?.parse()?;
                    rename = Some(s.value());
                    Ok(())
                } else if meta.path.is_ident("alias") {
                    let s: LitStr = meta.value()?.parse()?;
                    aliases.push(s.value());
                    Ok(())
                } else if meta.path.is_ident("default") {
                    default = Some(if meta.input.peek(syn::Token![=]) {
                        Some(meta.value()?.parse()?)
                    } else {
                        None
                    });
                    Ok(())
                } else if meta.path.is_ident("flatten") {
                    flatten = true;
                    Ok(())
                } else if meta.path.is_ident("nest") {
                    nest = true;
                    nested_span = Some(meta.path.span());
                    Ok(())
                } else if meta.path.is_ident("kind") {
                    kind = Some(meta.value()?.parse()?);
                    Ok(())
                } else {
                    Err(meta.error("Unsupported attribute"))
                }
            })?;
        }

        if flatten {
            // All flattened configs are nested, but not necessarily vice versa.
            nest = true;
        }
        if kind.is_some() && nest {
            let msg = "cannot specify `kind` for a `nest`ed / `flatten`ed configuration";
            let err_span = nested_span.unwrap_or(name_span);
            return Err(syn::Error::new(err_span, msg));
        }

        Ok(Self {
            nest,
            kind,
            rename,
            aliases,
            default,
            flatten,
        })
    }
}

pub(crate) struct ConfigField {
    pub attrs: ConfigFieldAttrs,
    pub docs: String,
    pub name: Ident,
    pub ty: Type,
}

impl ConfigField {
    fn new(raw: &Field) -> syn::Result<Self> {
        Self::new_inner(raw, false)
    }

    fn from_newtype_variant(raw: &Field) -> syn::Result<Self> {
        let mut this = Self::new_inner(raw, true)?;
        // Emulate flattening which happens to newtype variants in tagged enums.
        this.attrs.flatten = true;
        this.attrs.nest = true;
        Ok(this)
    }

    fn new_inner(raw: &Field, support_unnamed: bool) -> syn::Result<Self> {
        let name = if let Some(name) = raw.ident.clone() {
            name
        } else if support_unnamed {
            Ident::new("_", raw.ty.span()) // This name will not be used
        } else {
            let message = "Only named fields are supported";
            return Err(syn::Error::new_spanned(raw, message));
        };
        let ty = raw.ty.clone();

        let attrs = ConfigFieldAttrs::new(&raw.attrs, raw.span())?;
        Ok(Self {
            attrs,
            docs: parse_docs(&raw.attrs),
            name,
            ty,
        })
    }

    pub fn from_tag(tag: &LitStr) -> Self {
        Self {
            attrs: ConfigFieldAttrs::default(),
            docs: "Tag for the enum config".to_owned(),
            name: Ident::new(&tag.value(), tag.span()),
            ty: syn::parse_quote_spanned!(tag.span()=> ::std::string::String),
        }
    }

    pub fn extract_base_type(mut ty: &Type) -> &Type {
        loop {
            ty = match ty {
                Type::Array(array) => array.elem.as_ref(),
                Type::Path(TypePath { path, .. }) => {
                    if path.segments.len() != 1 {
                        break;
                    }
                    let segment = &path.segments[0];
                    if segment.ident != "Option" {
                        break;
                    }
                    let PathArguments::AngleBracketed(angle_bracketed) = &segment.arguments else {
                        break;
                    };
                    if angle_bracketed.args.len() != 1 {
                        break;
                    }
                    match &angle_bracketed.args[0] {
                        GenericArgument::Type(ty) => ty,
                        _ => break,
                    }
                }
                _ => break,
            };
        }
        ty
    }

    pub fn is_option(ty: &Type) -> bool {
        let Type::Path(TypePath { path, .. }) = ty else {
            return false;
        };
        if path.segments.len() != 1 {
            return false;
        }
        let segment = &path.segments[0];
        if segment.ident != "Option" {
            return false;
        }
        let PathArguments::AngleBracketed(angle_bracketed) = &segment.arguments else {
            return false;
        };
        angle_bracketed.args.len() == 1
    }

    pub fn param_name(&self) -> String {
        self.attrs
            .rename
            .clone()
            .unwrap_or_else(|| self.name.to_string())
    }

    pub fn type_kind(
        &self,
        meta_mod: &proc_macro2::TokenStream,
        ty: &Type,
    ) -> syn::Result<proc_macro2::TokenStream> {
        let base_type = Self::extract_base_type(ty);
        Ok(if let Some(kind) = &self.attrs.kind {
            quote!(#kind)
        } else if let Some(kind) = TypeKind::detect(base_type) {
            kind.to_tokens(meta_mod)
        } else {
            let msg = "Cannot auto-detect kind of this type; please add #[config(kind = ..)] attribute for the field";
            return Err(syn::Error::new_spanned(base_type, msg));
        })
    }
}

pub(crate) struct ConfigContainerAttrs {
    pub cr: Option<Path>,
}

impl ConfigContainerAttrs {
    fn new(attrs: &[Attribute]) -> syn::Result<Self> {
        let config_attrs = attrs.iter().filter(|attr| attr.path().is_ident("config"));

        let mut cr = None;
        for attr in config_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("crate") {
                    cr = Some(meta.value()?.parse()?);
                    Ok(())
                } else {
                    Err(meta.error("Unsupported attribute; only `crate` is supported`"))
                }
            })?;
        }
        Ok(Self { cr })
    }
}

pub(crate) struct ConfigEnumVariant {
    #[allow(dead_code)] // FIXME
    name: Ident,
    fields: Vec<ConfigField>,
}

pub(crate) enum ConfigContainerFields {
    Struct(Vec<ConfigField>),
    Enum {
        tag: Option<LitStr>,
        variants: Vec<ConfigEnumVariant>,
    },
}

impl ConfigContainerFields {
    pub fn all_fields(&self) -> Vec<&ConfigField> {
        match self {
            Self::Struct(fields) => fields.iter().collect(),
            Self::Enum { variants, .. } => variants
                .iter()
                .flat_map(|variant| &variant.fields)
                .collect(),
        }
    }
}

pub(crate) struct ConfigContainer {
    pub attrs: ConfigContainerAttrs,
    pub name: Ident,
    pub help: String,
    pub fields: ConfigContainerFields,
}

impl ConfigContainer {
    pub fn new(raw: &DeriveInput) -> syn::Result<Self> {
        if raw.generics.type_params().count() != 0
            || raw.generics.const_params().count() != 0
            || raw.generics.lifetimes().count() != 0
        {
            let message = "generics are not supported";
            return Err(syn::Error::new_spanned(&raw.generics, message));
        }

        let serde_attrs = ContainerAttrs::new(&raw.attrs)?;
        let fields = match &raw.data {
            Data::Struct(data) => {
                serde_attrs.validate_for_struct()?;
                ConfigContainerFields::Struct(Self::extract_struct_fields(data)?)
            }
            Data::Enum(data) => Self::extract_enum_fields(data, &serde_attrs)?,
            _ => {
                let message = "#[derive(DescribeConfig)] can only be placed on structs or enums";
                return Err(syn::Error::new_spanned(raw, message));
            }
        };

        let name = raw.ident.clone();
        let attrs = ConfigContainerAttrs::new(&raw.attrs)?;
        Ok(Self {
            attrs,
            name,
            help: parse_docs(&raw.attrs),
            fields,
        })
    }

    fn extract_struct_fields(data: &DataStruct) -> syn::Result<Vec<ConfigField>> {
        data.fields.iter().map(ConfigField::new).collect()
    }

    fn extract_enum_fields(
        data: &DataEnum,
        serde_attrs: &ContainerAttrs,
    ) -> syn::Result<ConfigContainerFields> {
        let mut variants = vec![];
        let mut merged_fields_by_name = HashSet::new();

        for variant in &data.variants {
            let mut variant_fields = vec![];
            match &variant.fields {
                Fields::Named(fields) => {
                    for field in &fields.named {
                        let new_field = ConfigField::new(field)?;
                        if !merged_fields_by_name.insert(new_field.param_name()) {
                            let msg = "Parameter with this name is already defined in another enum variant; \
                                this may lead to unexpected config merge results and thus not supported";
                            return Err(syn::Error::new_spanned(field, msg));
                        }
                        variant_fields.push(new_field);
                    }
                }
                Fields::Unnamed(fields) => {
                    if fields.unnamed.len() >= 2 {
                        let msg = "Variants with >=2 unnamed fields are not supported";
                        return Err(syn::Error::new(variant.ident.span(), msg));
                    } else if fields.unnamed.len() == 1 {
                        let field = fields.unnamed.first().unwrap();
                        variant_fields.push(ConfigField::from_newtype_variant(field)?);
                    }
                }
                Fields::Unit => { /* no fields to add */ }
            }
            variants.push(ConfigEnumVariant {
                name: variant.ident.clone(),
                fields: variant_fields,
            });
        }

        let has_fields = variants.iter().any(|variant| !variant.fields.is_empty());
        let tag = if has_fields {
            let tag = serde_attrs.tag.clone().ok_or_else(|| {
                let msg = "Only tagged enums are supported as configs. Please add #[serde(tag = ..)] to the enum";
                syn::Error::new_spanned(&data.variants, msg)
            })?;
            if merged_fields_by_name.contains(&tag.value()) {
                let msg = "Tag name coincides with an existing param name";
                return Err(syn::Error::new(tag.span(), msg));
            }
            Some(tag)
        } else {
            None
        };

        Ok(ConfigContainerFields::Enum { tag, variants })
    }

    pub fn cr(&self) -> proc_macro2::TokenStream {
        if let Some(cr) = &self.attrs.cr {
            quote!(#cr)
        } else {
            let name = &self.name;
            quote_spanned!(name.span()=> ::smart_config)
        }
    }
}
