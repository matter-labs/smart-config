//! Miscellaneous utils.

use std::{collections::HashSet, iter};

use proc_macro2::Ident;
use quote::{quote, quote_spanned};
use syn::{
    spanned::Spanned, Attribute, Data, DataEnum, DataStruct, DeriveInput, Expr, Field, Fields,
    Index, Lit, LitStr, Member, Path, PathArguments, Type, TypePath,
};

pub(crate) fn wrap_in_option(val: Option<proc_macro2::TokenStream>) -> proc_macro2::TokenStream {
    match val {
        Some(val) => quote!(::core::option::Option::Some(#val)),
        None => quote!(::core::option::Option::None),
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

pub(crate) struct ConfigVariantAttrs {
    pub rename: Option<LitStr>,
    pub aliases: Vec<LitStr>,
    pub default: bool,
}

impl ConfigVariantAttrs {
    fn new(attrs: &[Attribute]) -> syn::Result<Self> {
        let config_attrs = attrs.iter().filter(|attr| attr.path().is_ident("config"));

        let mut rename = None;
        let mut aliases = vec![];
        let mut default = false;
        for attr in config_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    rename = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("alias") {
                    aliases.push(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("default") {
                    default = true;
                    Ok(())
                } else {
                    Err(meta.error("Unsupported attribute"))
                }
            })?;
        }
        Ok(Self {
            rename,
            aliases,
            default,
        })
    }
}

pub(crate) enum DefaultValue {
    DefaultTrait,
    Path(Path),
    Expr(Expr),
}

impl DefaultValue {
    pub fn instance(&self, span: proc_macro2::Span) -> proc_macro2::TokenStream {
        match self {
            Self::DefaultTrait => quote_spanned!(span=> ::core::default::Default::default()),
            Self::Path(path) => quote_spanned!(span=> #path()),
            Self::Expr(expr) => quote_spanned!(span=> #expr),
        }
    }

    fn fallback_fn(&self, span: proc_macro2::Span) -> proc_macro2::TokenStream {
        match self {
            Self::DefaultTrait => {
                quote_spanned!(span=> ::core::default::Default::default)
            }
            Self::Path(def_fn) => quote!(#def_fn),
            Self::Expr(expr) => quote_spanned!(span=> || #expr),
        }
    }
}

#[derive(Default)]
pub(crate) struct ConfigFieldAttrs {
    pub rename: Option<String>,
    pub aliases: Vec<String>,
    pub default: Option<DefaultValue>,
    pub flatten: bool,
    pub nest: bool,
    pub with: Option<Expr>,
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
        let mut with = None;
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
                        DefaultValue::Path(meta.value()?.parse()?)
                    } else {
                        DefaultValue::DefaultTrait
                    });
                    Ok(())
                } else if meta.path.is_ident("default_t") {
                    default = Some(DefaultValue::Expr(meta.value()?.parse()?));
                    Ok(())
                } else if meta.path.is_ident("flatten") {
                    flatten = true;
                    Ok(())
                } else if meta.path.is_ident("nest") {
                    nest = true;
                    nested_span = Some(meta.path.span());
                    Ok(())
                } else if meta.path.is_ident("with") {
                    with = Some(meta.value()?.parse()?);
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
        if with.is_some() && nest {
            let msg = "cannot specify `with` for a `nest`ed / `flatten`ed configuration";
            let err_span = nested_span.unwrap_or(name_span);
            return Err(syn::Error::new(err_span, msg));
        }

        Ok(Self {
            nest,
            with,
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
    pub name: Member,
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
            Member::Named(name)
        } else if support_unnamed {
            Member::Unnamed(Index {
                index: 0,
                span: raw.ty.span(),
            })
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

    pub fn from_tag(
        cr: &proc_macro2::TokenStream,
        tag: &LitStr,
        variants: impl Iterator<Item = String>,
        default: Option<&str>,
    ) -> Self {
        // FIXME: use `WithDefault` here?
        let default_opt = wrap_in_option(default.map(|val| quote!(#val)));
        let with = syn::parse_quote! {
            #cr::de::TagDeserializer::new(&[#(#variants,)*], #default_opt)
        };

        Self {
            attrs: ConfigFieldAttrs {
                default: default.map(|s| {
                    DefaultValue::Expr(syn::parse_quote_spanned!(tag.span() => #s.into()))
                }),
                with: Some(with),
                ..ConfigFieldAttrs::default()
            },
            docs: "Tag for the enum config".to_owned(),
            name: Ident::new(&tag.value(), tag.span()).into(),
            ty: syn::parse_quote_spanned!(tag.span()=> &'static str),
        }
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
            .unwrap_or_else(|| match &self.name {
                Member::Named(ident) => ident.to_string(),
                Member::Unnamed(idx) => idx.index.to_string(),
            })
    }

    pub fn default_fn(&self) -> Option<proc_macro2::TokenStream> {
        let name_span = self.name.span();
        if let Some(default) = &self.attrs.default {
            Some(default.fallback_fn(name_span))
        } else if Self::is_option(&self.ty) {
            Some(quote_spanned!(name_span=> || ::core::option::Option::None))
        } else {
            None
        }
    }
}

pub(crate) struct ConfigContainerAttrs {
    pub cr: Option<Path>,
    pub rename_all: Option<LitStr>,
    pub tag: Option<LitStr>,
    pub derive_default: bool,
}

impl ConfigContainerAttrs {
    fn new(attrs: &[Attribute]) -> syn::Result<Self> {
        let config_attrs = attrs.iter().filter(|attr| attr.path().is_ident("config"));

        let mut cr = None;
        let mut rename_all = None;
        let mut tag = None;
        let mut derive_default = false;
        for attr in config_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("crate") {
                    cr = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("rename_all") {
                    rename_all = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("tag") {
                    tag = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("derive") {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let tr: Ident = content.parse()?;
                    if tr == "Default" {
                        derive_default = true;
                    } else {
                        let msg = "Can only derive(Default) yet";
                        return Err(syn::Error::new(tr.span(), msg));
                    }
                    Ok(())
                } else {
                    Err(meta.error("Unsupported attribute"))
                }
            })?;
        }
        Ok(Self {
            cr,
            rename_all,
            tag,
            derive_default,
        })
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

pub(crate) struct ConfigEnumVariant {
    pub attrs: ConfigVariantAttrs,
    pub name: Ident,
    pub fields: Vec<ConfigField>,
}

impl ConfigEnumVariant {
    pub fn name(&self) -> String {
        self.attrs
            .rename
            .as_ref()
            .map(LitStr::value)
            .unwrap_or_else(|| self.name.to_string())
    }

    pub fn expected_variants(&self) -> impl Iterator<Item = String> + '_ {
        iter::once(self.name()).chain(self.attrs.aliases.iter().map(LitStr::value))
    }
}

pub(crate) enum ConfigContainerFields {
    Struct(Vec<ConfigField>),
    Enum {
        tag: LitStr,
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

        let attrs = ConfigContainerAttrs::new(&raw.attrs)?;
        let fields = match &raw.data {
            Data::Struct(data) => {
                attrs.validate_for_struct()?;
                ConfigContainerFields::Struct(Self::extract_struct_fields(data)?)
            }
            Data::Enum(data) => Self::extract_enum_fields(data, &attrs)?,
            _ => {
                let message = "#[derive(DescribeConfig)] can only be placed on structs or enums";
                return Err(syn::Error::new_spanned(raw, message));
            }
        };

        let name = raw.ident.clone();

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
        container_attrs: &ConfigContainerAttrs,
    ) -> syn::Result<ConfigContainerFields> {
        let mut variants = vec![];
        let mut merged_fields_by_name = HashSet::new();
        let mut variants_with_aliases = HashSet::new();
        let mut has_default_variant = false;

        for variant in &data.variants {
            let attrs = ConfigVariantAttrs::new(&variant.attrs)?;
            if let Some(rename) = &attrs.rename {
                if !variants_with_aliases.insert(rename.value()) {
                    let msg = "Tag value is redefined";
                    return Err(syn::Error::new(rename.span(), msg));
                }
            }
            for alias in &attrs.aliases {
                if !variants_with_aliases.insert(alias.value()) {
                    let msg = "Tag value is redefined";
                    return Err(syn::Error::new(alias.span(), msg));
                }
            }
            if attrs.default {
                if has_default_variant {
                    let msg = "Only one variant can be marked as default";
                    return Err(syn::Error::new(variant.ident.span(), msg));
                }
                has_default_variant = true;
            }

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
                attrs,
                name: variant.ident.clone(),
                fields: variant_fields,
            });
        }

        let has_fields = variants.iter().any(|variant| !variant.fields.is_empty());
        if !has_fields {
            let msg = "Cannot use an enum without fields as a config; this is useless";
            return Err(syn::Error::new_spanned(&data.variants, msg));
        }
        let tag = container_attrs.tag.clone().ok_or_else(|| {
            let msg = "Only tagged enums are supported as configs. Please add #[config(tag = ..)] to the enum";
            syn::Error::new_spanned(&data.variants, msg)
        })?;
        if merged_fields_by_name.contains(&tag.value()) {
            let msg = "Tag name coincides with an existing param name";
            return Err(syn::Error::new(tag.span(), msg));
        }

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
