//! `DescribeConfig` derive macro implementation.

use std::collections::HashSet;

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{quote, quote_spanned};
use syn::{
    spanned::Spanned, Attribute, Data, DataEnum, DataStruct, DeriveInput, Expr, Field, Fields,
    GenericArgument, Lit, LitStr, Path, PathArguments, Type, TypePath,
};

use crate::utils::TypeKind;

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

struct ContainerAttrs {
    rename_all: Option<LitStr>,
    tag: Option<LitStr>,
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
struct ConfigFieldAttrs {
    rename: Option<String>,
    aliases: Vec<String>,
    default: Option<Option<Path>>,
    flatten: bool,
    nest: bool,
    kind: Option<Expr>,
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

enum ConfigFieldData {
    Ordinary { name: Ident, ty: Type },
    EnumTag(LitStr),
}

impl ConfigFieldData {
    fn name(&self) -> String {
        match self {
            Self::Ordinary { name, .. } => name.to_string(),
            Self::EnumTag(tag) => tag.value(),
        }
    }

    fn name_span(&self) -> proc_macro2::Span {
        match self {
            Self::Ordinary { name, .. } => name.span(),
            Self::EnumTag(tag) => tag.span(),
        }
    }
}

struct ConfigField {
    attrs: ConfigFieldAttrs,
    docs: String,
    data: ConfigFieldData,
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
            data: ConfigFieldData::Ordinary { name, ty },
        })
    }

    fn from_tag(tag: LitStr) -> Self {
        Self {
            attrs: ConfigFieldAttrs::default(),
            docs: "Tag for the enum config".to_owned(),
            data: ConfigFieldData::EnumTag(tag),
        }
    }

    fn extract_base_type(mut ty: &Type) -> &Type {
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

    fn is_option(ty: &Type) -> bool {
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

    fn param_name(&self) -> String {
        self.attrs
            .rename
            .clone()
            .unwrap_or_else(|| self.data.name())
    }

    fn describe_param(
        &self,
        cr: &proc_macro2::TokenStream,
    ) -> syn::Result<proc_macro2::TokenStream> {
        let name_span = self.data.name_span();
        let aliases = self.attrs.aliases.iter();
        let help = &self.docs;
        let param_name = self.param_name();

        let string_type;
        let (ty, base_type) = match &self.data {
            ConfigFieldData::Ordinary { ty, .. } => (ty, Self::extract_base_type(ty)),
            ConfigFieldData::EnumTag(tag) => {
                string_type = syn::parse_quote_spanned!(tag.span()=> ::std::string::String);
                (&string_type, &string_type)
            }
        };

        let ty_in_code = if let Some(text) = ty.span().source_text() {
            quote!(#text)
        } else {
            quote!(::core::stringify!(#ty))
        };

        let type_kind = if let Some(kind) = &self.attrs.kind {
            quote!(#kind)
        } else if let Some(kind) = TypeKind::detect(base_type) {
            kind.to_tokens(cr)
        } else {
            let msg = "Cannot auto-detect kind of this type; please add #[config(kind = ..)] attribute for the field";
            return Err(syn::Error::new_spanned(base_type, msg));
        };

        let default_value = match &self.attrs.default {
            None if !Self::is_option(ty) => None,
            Some(None) | None => Some(quote_spanned! {name_span=>
                <::std::boxed::Box<#ty> as ::core::default::Default>::default()
            }),
            Some(Some(path)) => {
                Some(quote_spanned!(name_span=> ::std::boxed::Box::<#ty>::new(#path())))
            }
        };
        let default_value = if let Some(value) = default_value {
            quote_spanned!(name_span=> ::core::option::Option::Some(|| #value))
        } else {
            quote_spanned!(name_span=> ::core::option::Option::None)
        };

        let aliases_validation = aliases
            .clone()
            .map(|alias| quote_spanned!(name_span=> #cr::validation::assert_param_name(#alias);));

        Ok(quote_spanned! {name_span=> {
            const _: () = {
                #cr::validation::assert_param_name(#param_name);
                #(#aliases_validation)*
            };

            #cr::ParamMetadata {
                name: #param_name,
                aliases: &[#(#aliases,)*],
                help: #help,
                ty: #cr::RustType::of::<#ty>(#ty_in_code),
                type_kind: #type_kind,
                unit: #cr::UnitOfMeasurement::detect(#param_name, #type_kind),
                default_value: #default_value,
            }
        }})
    }

    fn describe_nested_config(&self, cr: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let ConfigFieldData::Ordinary { name, ty } = &self.data else {
            unreachable!("enum tags are never nested");
        };
        let config_name = if self.attrs.flatten {
            String::new()
        } else {
            self.param_name()
        };

        quote_spanned! {name.span()=>
            #cr::NestedConfigMetadata {
                name: #config_name,
                meta: <#ty as #cr::DescribeConfig>::describe_config(),
            }
        }
    }

    fn deserialize_param(
        &self,
        cr: &proc_macro2::TokenStream,
        index: usize,
    ) -> proc_macro2::TokenStream {
        let ConfigFieldData::Ordinary { name, ty } = &self.data else {
            unreachable!("enum tags are not deserialized using this method");
        };
        let name_span = name.span();
        let param_name = self.param_name();

        let default_fallback = match &self.attrs.default {
            None if !Self::is_option(ty) => {
                quote_spanned!(name_span=> ::core::option::Option::None)
            }
            Some(None) | None => {
                quote_spanned!(name_span=> ::core::option::Option::Some(::core::default::Default::default))
            }
            Some(Some(def_fn)) => quote_spanned!(name_span=> ::core::option::Option::Some(#def_fn)),
        };

        let value = if !self.attrs.nest {
            quote_spanned! {name_span=>
                deserializer.deserialize_param(
                    #index,
                    #param_name,
                    #default_fallback,
                )?
            }
        } else if self.attrs.flatten {
            quote_spanned! {name_span=>
                #cr::DeserializeConfig::deserialize_config(deserializer.for_flattened_config())?
            }
        } else {
            quote_spanned! {name_span=>
                deserializer.deserialize_nested_config(
                    #index,
                    #param_name,
                    #default_fallback,
                )?
            }
        };
        quote_spanned!(name_span=> #name: #value)
    }
}

struct DescribeConfigAttrs {
    cr: Option<Path>,
}

impl DescribeConfigAttrs {
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

struct DescribeConfigImpl {
    attrs: DescribeConfigAttrs,
    name: Ident,
    help: String,
    fields: Vec<ConfigField>,
}

impl DescribeConfigImpl {
    fn new(raw: &DeriveInput) -> syn::Result<Self> {
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
                Self::extract_struct_fields(data)?
            }
            Data::Enum(data) => Self::extract_enum_fields(data, &serde_attrs)?,
            _ => {
                let message = "#[derive(DescribeConfig)] can only be placed on structs or enums";
                return Err(syn::Error::new_spanned(raw, message));
            }
        };

        let name = raw.ident.clone();
        let attrs = DescribeConfigAttrs::new(&raw.attrs)?;
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
    ) -> syn::Result<Vec<ConfigField>> {
        let mut merged_fields = vec![];
        let mut merged_fields_by_name = HashSet::new();

        for variant in &data.variants {
            match &variant.fields {
                Fields::Named(fields) => {
                    for field in &fields.named {
                        let new_field = ConfigField::new(field)?;
                        if !merged_fields_by_name.insert(new_field.param_name()) {
                            let msg = "Parameter with this name is already defined in another enum variant; \
                                this may lead to unexpected config merge results and thus not supported";
                            return Err(syn::Error::new_spanned(field, msg));
                        }
                        merged_fields.push(new_field);
                    }
                }
                Fields::Unnamed(fields) => {
                    if fields.unnamed.len() >= 2 {
                        let msg = "Variants with >=2 unnamed fields are not supported";
                        return Err(syn::Error::new(variant.ident.span(), msg));
                    } else if fields.unnamed.len() == 1 {
                        let field = fields.unnamed.first().unwrap();
                        merged_fields.push(ConfigField::from_newtype_variant(field)?);
                    }
                }
                Fields::Unit => { /* no fields to add */ }
            }
        }

        if !merged_fields.is_empty() {
            let tag = serde_attrs.tag.clone().ok_or_else(|| {
                let msg = "Only tagged enums are supported as configs. Please add #[serde(tag = ..)] to the enum";
                syn::Error::new_spanned(&data.variants, msg)
            })?;
            if merged_fields_by_name.contains(&tag.value()) {
                let msg = "Tag name coincides with an existing param name";
                return Err(syn::Error::new(tag.span(), msg));
            }

            merged_fields.push(ConfigField::from_tag(tag));
        }

        Ok(merged_fields)
    }

    fn metadata_mod(&self) -> proc_macro2::TokenStream {
        if let Some(cr) = &self.attrs.cr {
            quote!(#cr::metadata)
        } else {
            let name = &self.name;
            quote_spanned!(name.span()=> ::smart_config::metadata)
        }
    }

    fn derive_describe_config(&self) -> syn::Result<proc_macro2::TokenStream> {
        let cr = self.metadata_mod();
        let name = &self.name;
        let name_str = name.to_string();
        let help = &self.help;

        let params = self.fields.iter().filter_map(|field| {
            if !field.attrs.nest {
                return Some(field.describe_param(&cr));
            }
            None
        });
        let params = params.collect::<syn::Result<Vec<_>>>()?;

        let nested_configs = self.fields.iter().filter_map(|field| {
            if field.attrs.nest {
                return Some(field.describe_nested_config(&cr));
            }
            None
        });

        Ok(quote! {
            impl #cr::DescribeConfig for #name {
                fn describe_config() -> &'static #cr::ConfigMetadata {
                    static METADATA_CELL: #cr::Lazy<#cr::ConfigMetadata> = #cr::Lazy::new(|| #cr::ConfigMetadata {
                        ty: #cr::RustType::of::<#name>(#name_str),
                        help: #help,
                        params: ::std::boxed::Box::new([#(#params,)*]),
                        nested_configs: ::std::boxed::Box::new([#(#nested_configs,)*]),
                    });
                    &METADATA_CELL
                }
            }
        })
    }

    fn de_mod(&self) -> proc_macro2::TokenStream {
        if let Some(cr) = &self.attrs.cr {
            quote!(#cr::de)
        } else {
            let name = &self.name;
            quote_spanned!(name.span()=> ::smart_config::de)
        }
    }

    fn derive_deserialize_config(&self) -> proc_macro2::TokenStream {
        let cr = self.de_mod();
        let name = &self.name;

        let mut param_index = 0;
        let mut nested_index = 0;
        let fields = self.fields.iter().map(|field| {
            let index;
            if field.attrs.nest {
                index = param_index;
                param_index += 1;
            } else {
                index = nested_index;
                nested_index += 1;
            };
            field.deserialize_param(&cr, index)
        });

        quote! {
            impl #cr::DeserializeConfig for #name {
                fn deserialize_config(
                    deserializer: #cr::ValueDeserializer<'_>,
                ) -> ::core::result::Result<Self, #cr::ParseError> {
                    let deserializer = deserializer.for_config::<Self>();
                    ::core::result::Result::Ok(Self {
                        #(#fields,)*
                    })
                }
            }
        }
    }

    fn derive_everything(&self) -> syn::Result<proc_macro2::TokenStream> {
        let describe = self.derive_describe_config()?;
        let deserialize = self.derive_deserialize_config();
        Ok(quote! {
            #describe
            #deserialize
        })
    }
}

pub(crate) fn impl_describe_config(input: TokenStream) -> TokenStream {
    let input: DeriveInput = syn::parse(input).unwrap();
    let trait_impl = match DescribeConfigImpl::new(&input) {
        Ok(trait_impl) => trait_impl,
        Err(err) => return err.into_compile_error().into(),
    };
    match trait_impl.derive_everything() {
        Ok(derived) => derived.into(),
        Err(err) => err.into_compile_error().into(),
    }
}
