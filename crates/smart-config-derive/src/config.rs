//! `DescribeConfig` derive macro implementation.

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{quote, quote_spanned};
use syn::{
    parse::Parse, spanned::Spanned, Attribute, Data, DeriveInput, Expr, Field, GenericArgument,
    Lit, LitStr, Path, PathArguments, Type, TypePath,
};

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

/// Recognized subset of `serde` field attributes.
struct SerdeData {
    rename: Option<String>,
    aliases: Vec<String>,
    default: Option<Option<Path>>,
    flatten: bool,
}

impl SerdeData {
    fn new(attrs: &[Attribute]) -> syn::Result<Self> {
        let serde_attrs = attrs.iter().filter(|attr| attr.path().is_ident("serde"));

        let mut rename = None;
        let mut aliases = vec![];
        let mut default = None;
        let mut flatten = false;
        for attr in serde_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    let s: LitStr = meta.value()?.parse()?;
                    rename = Some(s.value());
                } else if meta.path.is_ident("alias") {
                    let s: LitStr = meta.value()?.parse()?;
                    aliases.push(s.value());
                } else if meta.path.is_ident("default") {
                    if meta.input.is_empty() {
                        default = Some(None);
                    } else {
                        let s: LitStr = meta.value()?.parse()?;
                        default = Some(Some(s.parse()?));
                    }
                } else if meta.path.is_ident("flatten") {
                    flatten = true;
                } else {
                    // Digest any tokens
                    meta.input.parse::<proc_macro2::TokenStream>()?;
                }
                Ok(())
            })?;
        }
        Ok(Self {
            rename,
            aliases,
            default,
            flatten,
        })
    }
}

struct ConfigFieldAttrs {
    nested: bool,
    merge_from: Option<Vec<LitStr>>,
}

impl ConfigFieldAttrs {
    fn new(attrs: &[Attribute]) -> syn::Result<Self> {
        let config_attrs = attrs.iter().filter(|attr| attr.path().is_ident("config"));

        let mut nested = false;
        let mut merge_from = None;
        let mut nested_span = None;
        for attr in config_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("nested") {
                    nested = true;
                    nested_span = Some(meta.path.span());
                    Ok(())
                } else if meta.path.is_ident("merge_from") {
                    let merge_from = merge_from.get_or_insert_with(Vec::new);
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let punctuated =
                        content.parse_terminated(<LitStr as Parse>::parse, syn::Token![,])?;
                    merge_from.extend(punctuated);
                    Ok(())
                } else {
                    Err(meta.error(
                        "Unsupported attribute; only `nested` and `merge_from` are supported`",
                    ))
                }
            })?;
        }

        if let Some(nested_span) = nested_span {
            if merge_from.is_some() {
                let message =
                    "`merge_from` can only be specified on common parameters (not nested configs)";
                return Err(syn::Error::new(nested_span, message));
            }
        }
        Ok(Self { nested, merge_from })
    }
}

struct ConfigField {
    attrs: ConfigFieldAttrs,
    name: Ident,
    ty: Type,
    docs: String,
    serde_data: SerdeData,
}

impl ConfigField {
    fn new(raw: &Field) -> syn::Result<Self> {
        let name = raw.ident.clone().ok_or_else(|| {
            let message = "Only named fields are supported";
            syn::Error::new_spanned(raw, message)
        })?;
        let ty = raw.ty.clone();

        let serde_data = SerdeData::new(&raw.attrs)?;
        let attrs = ConfigFieldAttrs::new(&raw.attrs)?;

        if serde_data.flatten && !attrs.nested {
            let message = "#[serde(flatten)] should only be placed on nested configurations";
            return Err(syn::Error::new_spanned(raw, message));
        }

        Ok(Self {
            attrs,
            name,
            ty,
            docs: parse_docs(&raw.attrs),
            serde_data,
        })
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
                    if segment.ident != "Vec" && segment.ident != "Option" {
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

    fn describe_param(
        &self,
        cr: &proc_macro2::TokenStream,
        config_merge: &[syn::LitStr],
    ) -> proc_macro2::TokenStream {
        let name = &self.name;
        let aliases = self.serde_data.aliases.iter();
        let merge_from = self.attrs.merge_from.as_deref().unwrap_or(config_merge);
        let help = &self.docs;

        let param_name = self
            .serde_data
            .rename
            .clone()
            .unwrap_or_else(|| self.name.to_string());

        let ty = &self.ty;
        let ty_in_code = if let Some(text) = ty.span().source_text() {
            quote!(#text)
        } else {
            quote!(::core::stringify!(#ty))
        };
        let base_type = Self::extract_base_type(&self.ty);
        let base_type_in_code = if let Some(text) = base_type.span().source_text() {
            quote!(#text)
        } else {
            quote!(::core::stringify!(#base_type))
        };

        let default_value = match &self.serde_data.default {
            None if !Self::is_option(ty) => None,
            Some(None) | None => Some(quote_spanned! {name.span()=>
                <::std::boxed::Box<#ty> as ::core::default::Default>::default()
            }),
            Some(Some(path)) => {
                Some(quote_spanned!(name.span()=> ::std::boxed::Box::<#ty>::new(#path())))
            }
        };
        let default_value = if let Some(value) = default_value {
            quote_spanned!(name.span()=> ::core::option::Option::Some(|| #value))
        } else {
            quote_spanned!(name.span()=> ::core::option::Option::None)
        };

        quote_spanned! {name.span()=> {
            let base_type = #cr::RustType::of::<#base_type>(#base_type_in_code);
            #cr::ParamMetadata {
                name: #param_name,
                aliases: &[#(#aliases,)*],
                merge_from: &[#(#merge_from,)*],
                help: #help,
                ty: #cr::RustType::of::<#ty>(#ty_in_code),
                base_type,
                unit: #cr::UnitOfMeasurement::detect(#param_name, base_type),
                default_value: #default_value,
            }
        }}
    }

    fn describe_nested_config(&self, cr: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let name = &self.name;
        let ty = &self.ty;
        let config_name = if self.serde_data.flatten {
            String::new()
        } else {
            self.serde_data
                .rename
                .clone()
                .unwrap_or_else(|| self.name.to_string())
        };

        quote_spanned! {name.span()=>
            #cr::NestedConfigMetadata {
                name: #config_name,
                meta: <#ty as #cr::DescribeConfig>::describe_config(),
            }
        }
    }
}

struct DescribeConfigAttrs {
    cr: Option<Path>,
    merge_from: Vec<LitStr>,
}

impl DescribeConfigAttrs {
    fn new(attrs: &[Attribute]) -> syn::Result<Self> {
        let config_attrs = attrs.iter().filter(|attr| attr.path().is_ident("config"));

        let mut cr = None;
        let mut merge_from = vec![];
        for attr in config_attrs {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("crate") {
                    cr = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("merge_from") {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let punctuated =
                        content.parse_terminated(<LitStr as Parse>::parse, syn::Token![,])?;
                    merge_from.extend(punctuated);
                    Ok(())
                } else {
                    Err(meta.error(
                        "Unsupported attribute; only `crate` and `merge_from` are supported`",
                    ))
                }
            })?;
        }
        Ok(Self { cr, merge_from })
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
        let Data::Struct(data) = &raw.data else {
            let message = "#[derive(DescribeConfig)] can only be placed on structs";
            return Err(syn::Error::new_spanned(raw, message));
        };
        if raw.generics.type_params().count() != 0
            || raw.generics.const_params().count() != 0
            || raw.generics.lifetimes().count() != 0
        {
            let message = "generics are not supported";
            return Err(syn::Error::new_spanned(&raw.generics, message));
        }

        let name = raw.ident.clone();
        let attrs = DescribeConfigAttrs::new(&raw.attrs)?;
        let fields = data
            .fields
            .iter()
            .map(ConfigField::new)
            .collect::<syn::Result<_>>()?;
        Ok(Self {
            attrs,
            name,
            help: parse_docs(&raw.attrs),
            fields,
        })
    }

    fn cr(&self) -> proc_macro2::TokenStream {
        if let Some(cr) = &self.attrs.cr {
            quote!(#cr::metadata)
        } else {
            let name = &self.name;
            quote_spanned!(name.span()=> ::zksync_config::metadata)
        }
    }

    fn derive_describe_config(&self) -> proc_macro2::TokenStream {
        let cr = self.cr();
        let merge_from = &self.attrs.merge_from;
        let name = &self.name;
        let help = &self.help;

        let params = self.fields.iter().filter_map(|field| {
            if !field.attrs.nested {
                return Some(field.describe_param(&cr, merge_from));
            }
            None
        });
        let nested_configs = self.fields.iter().filter_map(|field| {
            if field.attrs.nested {
                return Some(field.describe_nested_config(&cr));
            }
            None
        });

        quote! {
            impl #cr::DescribeConfig for #name {
                fn describe_config() -> &'static #cr::ConfigMetadata {
                    static METADATA_CELL: #cr::Lazy<#cr::ConfigMetadata> = #cr::Lazy::new(|| #cr::ConfigMetadata {
                        help: #help,
                        params: ::std::boxed::Box::new([#(#params,)*]),
                        nested_configs: ::std::boxed::Box::new([#(#nested_configs,)*]),
                    });
                    &METADATA_CELL
                }
            }
        }
    }
}

pub(crate) fn impl_describe_config(input: TokenStream) -> TokenStream {
    let input: DeriveInput = syn::parse(input).unwrap();
    let trait_impl = match DescribeConfigImpl::new(&input) {
        Ok(trait_impl) => trait_impl,
        Err(err) => return err.into_compile_error().into(),
    };
    trait_impl.derive_describe_config().into()
}
