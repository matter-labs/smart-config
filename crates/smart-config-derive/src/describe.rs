//! `DescribeConfig` derive macro implementation.

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{spanned::Spanned, DeriveInput};

use crate::utils::{ConfigContainer, ConfigContainerFields, ConfigField};

impl ConfigField {
    fn describe_param(
        &self,
        meta_mod: &proc_macro2::TokenStream,
    ) -> syn::Result<proc_macro2::TokenStream> {
        let name_span = self.name.span();
        let aliases = self.attrs.aliases.iter();
        let help = &self.docs;
        let param_name = self.param_name();

        let ty = &self.ty;
        let ty_in_code = if let Some(text) = ty.span().source_text() {
            quote!(#text)
        } else {
            quote!(::core::stringify!(#ty))
        };
        let type_kind = self.type_kind(meta_mod, ty)?;

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

        let aliases_validation = aliases.clone().map(
            |alias| quote_spanned!(name_span=> #meta_mod::validation::assert_param_name(#alias);),
        );

        Ok(quote_spanned! {name_span=> {
            const _: () = {
                #meta_mod::validation::assert_param_name(#param_name);
                #(#aliases_validation)*
            };

            #meta_mod::ParamMetadata {
                name: #param_name,
                aliases: &[#(#aliases,)*],
                help: #help,
                ty: #meta_mod::RustType::of::<#ty>(#ty_in_code),
                type_kind: #type_kind,
                unit: #meta_mod::UnitOfMeasurement::detect(#param_name, #type_kind),
                default_value: #default_value,
            }
        }})
    }

    fn describe_nested_config(&self, cr: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let ty = &self.ty;
        let config_name = if self.attrs.flatten {
            String::new()
        } else {
            self.param_name()
        };

        quote_spanned! {self.name.span()=>
            #cr::metadata::NestedConfigMetadata {
                name: #config_name,
                meta: <#ty as #cr::DescribeConfig>::describe_config(),
            }
        }
    }
}

impl ConfigContainer {
    fn derive_describe_config(&self) -> syn::Result<proc_macro2::TokenStream> {
        let cr = self.cr();
        let meta_mod = quote!(#cr::metadata);
        let name = &self.name;
        let name_str = name.to_string();
        let help = &self.help;

        let all_fields = self.fields.all_fields();
        let params = all_fields.iter().filter_map(|field| {
            if !field.attrs.nest {
                return Some(field.describe_param(&meta_mod));
            }
            None
        });
        let mut params = params.collect::<syn::Result<Vec<_>>>()?;

        if let ConfigContainerFields::Enum { tag: Some(tag), .. } = &self.fields {
            // Add the tag field description
            let tag = ConfigField::from_tag(tag);
            params.push(tag.describe_param(&meta_mod)?);
        }

        let nested_configs = all_fields.iter().filter_map(|field| {
            if field.attrs.nest {
                return Some(field.describe_nested_config(&cr));
            }
            None
        });

        Ok(quote! {
            impl #cr::DescribeConfig for #name {
                fn describe_config() -> &'static #meta_mod::ConfigMetadata {
                    static METADATA_CELL: #cr::Lazy<#meta_mod::ConfigMetadata> = #cr::Lazy::new(|| #cr::ConfigMetadata {
                        ty: #meta_mod::RustType::of::<#name>(#name_str),
                        help: #help,
                        params: ::std::boxed::Box::new([#(#params,)*]),
                        nested_configs: ::std::boxed::Box::new([#(#nested_configs,)*]),
                    });
                    &METADATA_CELL
                }
            }
        })
    }
}

pub(crate) fn impl_describe_config(input: TokenStream) -> TokenStream {
    let input: DeriveInput = syn::parse(input).unwrap();
    let trait_impl = match ConfigContainer::new(&input) {
        Ok(trait_impl) => trait_impl,
        Err(err) => return err.into_compile_error().into(),
    };
    match trait_impl.derive_describe_config() {
        Ok(derived) => derived.into(),
        Err(err) => err.into_compile_error().into(),
    }
}
