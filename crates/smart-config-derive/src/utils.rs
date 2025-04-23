//! Miscellaneous utils.

use std::{
    collections::{HashMap, HashSet},
    iter,
};

use proc_macro2::Ident;
use quote::{quote, quote_spanned};
use syn::{
    ext::IdentExt, parse::ParseStream, spanned::Spanned, Attribute, Data, DataEnum, DataStruct,
    DeriveInput, Expr, Field, Fields, GenericArgument, Index, Lit, LitStr, Member, Path,
    PathArguments, Token, Type, TypePath,
};

pub(crate) fn wrap_in_option(val: Option<proc_macro2::TokenStream>) -> proc_macro2::TokenStream {
    if let Some(val) = val {
        quote!(::core::option::Option::Some(#val))
    } else {
        quote!(::core::option::Option::None)
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

#[derive(Debug)]
pub(crate) struct ConfigVariantAttrs {
    pub(crate) rename: Option<LitStr>,
    pub(crate) aliases: Vec<LitStr>,
    pub(crate) default: bool,
    pub(crate) help: String,
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
            help: parse_docs(attrs),
        })
    }
}

#[derive(Debug)]
pub(crate) enum DefaultValue {
    DefaultTrait,
    Path(Path),
    Expr(Expr),
}

impl DefaultValue {
    pub(crate) fn instance(&self, span: proc_macro2::Span) -> proc_macro2::TokenStream {
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

#[derive(Debug, Default)]
pub(crate) struct ConfigFieldAttrs {
    pub(crate) rename: Option<LitStr>,
    pub(crate) aliases: Vec<LitStr>,
    pub(crate) default: Option<DefaultValue>,
    pub(crate) alt: Option<Expr>,
    pub(crate) flatten: bool,
    pub(crate) nest: bool,
    pub(crate) is_secret: bool,
    pub(crate) with: Option<Expr>,
    #[allow(dead_code)] // FIXME
    pub(crate) validations: Vec<Validation>,
}

impl ConfigFieldAttrs {
    fn new(attrs: &[Attribute], is_option: bool) -> syn::Result<Self> {
        let config_attrs = attrs.iter().filter(|attr| attr.path().is_ident("config"));

        let mut rename = None;
        let mut aliases = vec![];
        let mut default = None;
        let mut fallback = None;
        let mut nested_span = None;
        let mut flatten_span = None;
        let mut with = None;
        let mut secret_span = None;
        let mut validations = vec![];
        for attr in config_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    rename = Some(meta.value()?.parse::<LitStr>()?);
                    Ok(())
                } else if meta.path.is_ident("alias") {
                    aliases.push(meta.value()?.parse()?);
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
                } else if meta.path.is_ident("fallback") {
                    fallback = Some(meta.value()?.parse::<Expr>()?);
                    Ok(())
                } else if meta.path.is_ident("flatten") {
                    flatten_span = Some(meta.path.span());
                    Ok(())
                } else if meta.path.is_ident("nest") {
                    nested_span = Some(meta.path.span());
                    Ok(())
                } else if meta.path.is_ident("secret") {
                    secret_span = Some(meta.path.span());
                    Ok(())
                } else if meta.path.is_ident("with") {
                    with = Some(meta.value()?.parse::<Expr>()?);
                    Ok(())
                } else if meta.path.is_ident("validate") {
                    validations.push(Validation::new(meta.input)?);
                    Ok(())
                } else {
                    Err(meta.error("Unsupported attribute"))
                }
            })?;
        }

        if let (Some(nested_span), Some(_)) = (nested_span, flatten_span) {
            let msg = "cannot specify both `nest` and `flatten` for config";
            return Err(syn::Error::new(nested_span, msg));
        }
        let flatten = flatten_span.is_some();
        // All flattened configs are nested internally, but not necessarily vice versa.
        let nest = flatten_span.is_some() || nested_span.is_some();

        if let (Some(with), true) = (&with, nest) {
            let msg = "cannot specify `with` for a `nest`ed / `flatten`ed configuration";
            return Err(syn::Error::new(with.span(), msg));
        }
        if let (Some(fallback), true) = (&fallback, nest) {
            let msg = "cannot specify `fallback` for a `nest`ed / `flatten`ed configuration";
            return Err(syn::Error::new(fallback.span(), msg));
        }

        if let (Some(flatten_span), Some(_)) = (flatten_span, &rename) {
            let msg = "`rename` attribute is useless for flattened configs; did you mean to make a config nested?";
            return Err(syn::Error::new(flatten_span, msg));
        }
        if let (Some(flatten_span), true) = (flatten_span, is_option) {
            let msg = "cannot make `flatten`ed config optional; did you mean to make it nested?";
            return Err(syn::Error::new(flatten_span, msg));
        }
        if let (Some(flatten_span), false) = (flatten_span, aliases.is_empty()) {
            let msg = "aliases for flattened configs are not supported yet; did you mean to make a config nested?";
            return Err(syn::Error::new(flatten_span, msg));
        }
        if let (Some(secret_span), true) = (secret_span, nest) {
            let msg = "only params can be marked as secret, sub-configs cannot";
            return Err(syn::Error::new(secret_span, msg));
        }

        Ok(Self {
            rename,
            aliases,
            default,
            alt: fallback,
            flatten,
            nest,
            with,
            validations,
            is_secret: secret_span.is_some(),
        })
    }
}

#[derive(Debug)]
pub(crate) struct ConfigField {
    pub(crate) attrs: ConfigFieldAttrs,
    pub(crate) docs: String,
    pub(crate) name: Member,
    pub(crate) ty: Type,
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

        let attrs = ConfigFieldAttrs::new(&raw.attrs, Self::is_option(&ty))?;
        Ok(Self {
            attrs,
            docs: parse_docs(&raw.attrs),
            name,
            ty,
        })
    }

    pub(crate) fn from_tag(
        cr: &proc_macro2::TokenStream,
        tag: &LitStr,
        variants: impl Iterator<Item = String>,
        default: Option<&str>,
    ) -> Self {
        // FIXME: use `WithDefault` here?
        let default_opt = wrap_in_option(default.map(|val| quote!(#val)));
        let with = syn::parse_quote! {
            #cr::de::_private::TagDeserializer::new(&[#(#variants,)*], #default_opt)
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

    pub(crate) fn is_option(ty: &Type) -> bool {
        Self::unwrap_option(ty).is_some()
    }

    pub(crate) fn unwrap_option(ty: &Type) -> Option<&Type> {
        let Type::Path(TypePath { path, .. }) = ty else {
            return None;
        };
        if path.segments.len() != 1 {
            return None;
        }
        let segment = &path.segments[0];
        if segment.ident != "Option" {
            return None;
        }
        let PathArguments::AngleBracketed(angle_bracketed) = &segment.arguments else {
            return None;
        };
        if angle_bracketed.args.len() != 1 {
            return None;
        }
        match &angle_bracketed.args[0] {
            GenericArgument::Type(ty) => Some(ty),
            _ => None,
        }
    }

    pub(crate) fn param_name(&self) -> String {
        self.attrs.rename.as_ref().map_or_else(
            || match &self.name {
                Member::Named(ident) => ident.unraw().to_string(),
                Member::Unnamed(idx) => idx.index.to_string(),
            },
            LitStr::value,
        )
    }

    pub(crate) fn name_span(&self) -> proc_macro2::Span {
        match &self.name {
            Member::Named(ident) => ident.span(),
            Member::Unnamed(_) => self.ty.span(),
        }
    }

    pub(crate) fn default_fn(&self) -> Option<proc_macro2::TokenStream> {
        let name_span = self.name_span();
        self.attrs
            .default
            .as_ref()
            .map(|default| default.fallback_fn(name_span))
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum RenameRule {
    LowerCase,
    UpperCase,
    CamelCase,
    SnakeCase,
    ScreamingSnakeCase,
    KebabCase,
    ScreamingKebabCase,
}

impl RenameRule {
    fn parse(s: &str) -> Result<Self, &'static str> {
        Ok(match s {
            "lowercase" => Self::LowerCase,
            "UPPERCASE" => Self::UpperCase,
            "camelCase" => Self::CamelCase,
            "snake_case" => Self::SnakeCase,
            "SCREAMING_SNAKE_CASE" => Self::ScreamingSnakeCase,
            "kebab-case" => Self::KebabCase,
            "SCREAMING-KEBAB-CASE" => Self::ScreamingKebabCase,
            _ => {
                return Err(
                    "Invalid case specified; should be one of: lowercase, UPPERCASE, camelCase, \
                     snake_case, SCREAMING_SNAKE_CASE, kebab-case, SCREAMING-KEBAB-CASE",
                )
            }
        })
    }

    fn transform(self, ident: &str) -> String {
        debug_assert!(ident.is_ascii()); // Should be checked previously
        let (spacing_char, scream) = match self {
            Self::LowerCase => return ident.to_ascii_lowercase(),
            Self::UpperCase => return ident.to_ascii_uppercase(),
            Self::CamelCase => return ident[..1].to_ascii_lowercase() + &ident[1..],
            // ^ Since `ident` is an ASCII string, indexing is safe
            Self::SnakeCase => ('_', false),
            Self::ScreamingSnakeCase => ('_', true),
            Self::KebabCase => ('-', false),
            Self::ScreamingKebabCase => ('-', true),
        };

        let mut output = String::with_capacity(ident.len());
        for (i, ch) in ident.char_indices() {
            if i > 0 && ch.is_ascii_uppercase() {
                output.push(spacing_char);
            }
            output.push(if scream {
                ch.to_ascii_uppercase()
            } else {
                ch.to_ascii_lowercase()
            });
        }
        output
    }
}

#[derive(Debug)]
pub(crate) struct Validation {
    pub(crate) description: LitStr,
    pub(crate) path: Path,
}

impl Validation {
    fn new(input: ParseStream<'_>) -> syn::Result<Self> {
        let content;
        syn::parenthesized!(content in input);
        let description = content.parse()?;
        content.parse::<Token![,]>()?;
        let path = content.parse()?;
        Ok(Self { description, path })
    }
}

#[derive(Debug)]
pub(crate) struct ConfigContainerAttrs {
    pub(crate) cr: Option<Path>,
    pub(crate) rename_all: Option<RenameRule>,
    pub(crate) tag: Option<LitStr>,
    pub(crate) validations: Vec<Validation>,
    pub(crate) derive_default: bool,
}

impl ConfigContainerAttrs {
    fn new(attrs: &[Attribute], is_struct: bool) -> syn::Result<Self> {
        let config_attrs = attrs.iter().filter(|attr| attr.path().is_ident("config"));

        let mut cr = None;
        let mut rename_all = None;
        let mut tag = None;
        let mut validations = vec![];
        let mut derive_default = false;
        for attr in config_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("crate") {
                    cr = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("rename_all") {
                    let rule: LitStr = meta.value()?.parse()?;
                    let parsed = RenameRule::parse(&rule.value())
                        .map_err(|msg| syn::Error::new(rule.span(), msg))?;
                    rename_all = Some((rule, parsed));
                    Ok(())
                } else if meta.path.is_ident("tag") {
                    tag = Some(meta.value()?.parse::<LitStr>()?);
                    Ok(())
                } else if meta.path.is_ident("validate") {
                    validations.push(Validation::new(meta.input)?);
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

        if is_struct {
            if let Some((rename_all, _)) = &rename_all {
                let msg = "`rename_all` attribute must not be used on struct configs";
                return Err(syn::Error::new(rename_all.span(), msg));
            }
            if let Some(tag) = &tag {
                let msg = "`tag` attribute must not be used on struct configs";
                return Err(syn::Error::new(tag.span(), msg));
            }
        }

        Ok(Self {
            cr,
            rename_all: rename_all.map(|(_, parsed)| parsed),
            tag,
            validations,
            derive_default,
        })
    }
}

#[derive(Debug)]
pub(crate) struct ConfigEnumVariant {
    pub(crate) attrs: ConfigVariantAttrs,
    pub(crate) name: Ident,
    pub(crate) fields: Vec<ConfigField>,
}

impl ConfigEnumVariant {
    pub(crate) fn name(&self, rename_rule: Option<RenameRule>) -> String {
        self.attrs.rename.as_ref().map_or_else(
            || {
                let name = self.name.to_string();
                if let Some(rule) = rename_rule {
                    rule.transform(&name)
                } else {
                    name
                }
            },
            LitStr::value,
        )
    }

    pub(crate) fn expected_variants(
        &self,
        rename_rule: Option<RenameRule>,
    ) -> impl Iterator<Item = String> + '_ {
        iter::once(self.name(rename_rule)).chain(self.attrs.aliases.iter().map(LitStr::value))
    }
}

#[derive(Debug)]
pub(crate) enum ConfigContainerFields {
    Struct(Vec<ConfigField>),
    Enum {
        tag: LitStr,
        variants: Vec<ConfigEnumVariant>,
    },
}

impl ConfigContainerFields {
    /// Returns the variant index together with each field. For struct configs, all indices are 0.
    pub(crate) fn all_fields(&self) -> Vec<(usize, &ConfigField)> {
        match self {
            Self::Struct(fields) => fields.iter().map(|field| (0, field)).collect(),
            Self::Enum { variants, .. } => variants
                .iter()
                .enumerate()
                .flat_map(|(variant_idx, variant)| {
                    variant.fields.iter().map(move |field| (variant_idx, field))
                })
                .collect(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ConfigContainer {
    pub(crate) attrs: ConfigContainerAttrs,
    pub(crate) name: Ident,
    pub(crate) help: String,
    pub(crate) fields: ConfigContainerFields,
}

impl ConfigContainer {
    pub(crate) fn new(raw: &DeriveInput) -> syn::Result<Self> {
        if raw.generics.type_params().count() != 0
            || raw.generics.const_params().count() != 0
            || raw.generics.lifetimes().count() != 0
        {
            let message = "generics are not supported";
            return Err(syn::Error::new_spanned(&raw.generics, message));
        }

        let attrs = ConfigContainerAttrs::new(&raw.attrs, matches!(&raw.data, Data::Struct(_)))?;
        let fields = match &raw.data {
            Data::Struct(data) => ConfigContainerFields::Struct(Self::extract_struct_fields(data)?),
            Data::Enum(data) => Self::extract_enum_fields(data, &attrs)?,
            Data::Union(_) => {
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
        let mut merged_fields_by_name = HashMap::new();
        let mut variants_with_aliases = HashSet::new();
        let mut has_default_variant = false;

        for variant in &data.variants {
            let attrs = ConfigVariantAttrs::new(&variant.attrs)?;

            let (name, name_span) = attrs.rename.as_ref().map_or_else(
                || (variant.ident.to_string(), variant.ident.span()),
                |lit| (lit.value(), lit.span()),
            );
            if !variants_with_aliases.insert(name) {
                let msg = "Tag value is redefined";
                return Err(syn::Error::new(name_span, msg));
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
                        if let Some(prev_ty) =
                            merged_fields_by_name.insert(new_field.param_name(), &field.ty)
                        {
                            if *prev_ty != new_field.ty {
                                let msg = "Parameter with this name and another type is already defined in another enum variant; \
                                    this may lead to unexpected config merge results and thus not supported";
                                return Err(syn::Error::new_spanned(field, msg));
                            }
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
        if merged_fields_by_name.contains_key(&tag.value()) {
            let msg = "Tag name coincides with an existing param name";
            return Err(syn::Error::new(tag.span(), msg));
        }

        Ok(ConfigContainerFields::Enum { tag, variants })
    }

    // Need to specify span as an input, since setting span to e.g. `self.name.span()` will lead to "stretched" error spans
    // for validations.
    pub(crate) fn cr(&self, span: proc_macro2::Span) -> proc_macro2::TokenStream {
        if let Some(cr) = &self.attrs.cr {
            quote_spanned!(span=> #cr)
        } else {
            quote_spanned!(span=> ::smart_config)
        }
    }
}
