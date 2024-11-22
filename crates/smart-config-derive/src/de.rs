use std::iter;

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{quote, quote_spanned};
use syn::{spanned::Spanned, DeriveInput, LitStr};

use crate::utils::{ConfigContainer, ConfigContainerFields, ConfigEnumVariant, ConfigField};

impl ConfigField {
    /// Returns `Option<_>`.
    fn deserialize_param(
        &self,
        _cr: &proc_macro2::TokenStream, // will be required for `with = ..`
        index: usize,
    ) -> proc_macro2::TokenStream {
        let name_span = self.name.span();
        let default_instance = if let Some(default) = &self.attrs.default {
            Some(default.instance(name_span))
        } else if Self::is_option(&self.ty) {
            Some(quote_spanned!(name_span=> ::core::option::Option::None))
        } else {
            None
        };
        let err_if_missing = default_instance.is_none();

        if !self.attrs.nest {
            let mut param_expr = quote_spanned! {name_span=>
                ctx.deserialize_param(#index, #err_if_missing, &())
            };
            if let Some(default) = default_instance {
                param_expr = quote!(#param_expr.or_else(|| ::core::option::Option::Some(#default)));
            }
            param_expr
        } else {
            let mut config_expr = quote_spanned! {name_span=>
                ctx.deserialize_nested_config(#index, #err_if_missing)
            };
            if let Some(default) = default_instance {
                config_expr =
                    quote!(#config_expr.or_else(|| ::core::option::Option::Some(#default)));
            }
            config_expr
        }
    }
}

// FIXME: support rename_all = "snake_case"
impl ConfigEnumVariant {
    fn matches(&self) -> proc_macro2::TokenStream {
        let mut all_names = self.expected_variants();
        let name = all_names.next().unwrap();
        let name_span = self.name.span();
        quote_spanned!(name_span=> #name #(| #all_names)*)
    }

    fn expected_variants(&self) -> impl Iterator<Item = String> + '_ {
        iter::once(self.name()).chain(self.attrs.aliases.iter().map(LitStr::value))
    }
}

impl ConfigContainer {
    fn process_fields<'a>(
        fields: &'a [ConfigField],
        cr: &'a proc_macro2::TokenStream,
        param_index: &'a mut usize,
        nested_index: &'a mut usize,
    ) -> (proc_macro2::TokenStream, Vec<proc_macro2::TokenStream>) {
        let mut init = proc_macro2::TokenStream::default();
        let fields = fields.iter().enumerate().map(|(i, field)| {
            let index;
            if field.attrs.nest {
                index = *nested_index;
                *nested_index += 1;
            } else {
                index = *param_index;
                *param_index += 1;
            };

            let name = &field.name;
            let value = field.deserialize_param(cr, index);
            let local_var = Ident::new(&format!("__{i}"), name.span());
            init.extend(quote_spanned! {name.span()=>
                let #local_var = #value;
            });
            quote_spanned!(name.span()=> #name: #local_var?)
        });
        let fields = fields.collect();
        (init, fields)
    }

    fn derive_deserialize_config(&self) -> proc_macro2::TokenStream {
        let cr = self.cr();
        let name = &self.name;

        let mut param_index = 0;
        let mut nested_index = 0;
        let instance = match &self.fields {
            ConfigContainerFields::Struct(fields) => {
                let (init, fields) =
                    Self::process_fields(fields, &cr, &mut param_index, &mut nested_index);
                quote!({
                    #init
                    Self { #(#fields,)* }
                })
            }
            ConfigContainerFields::Enum { variants, .. } => {
                let match_hands = variants.iter().map(|variant| {
                    let name = &variant.name;
                    let matches = variant.matches();
                    let (init, variant_fields) = Self::process_fields(
                        &variant.fields,
                        &cr,
                        &mut param_index,
                        &mut nested_index,
                    );
                    quote!(#matches => {
                        #init
                        Self::#name { #(#variant_fields,)* }
                    })
                });
                let match_hands: Vec<_> = match_hands.collect();

                let expected_variants = variants
                    .iter()
                    .flat_map(ConfigEnumVariant::expected_variants);
                let default = variants
                    .iter()
                    .find_map(|variant| variant.attrs.default.then(|| variant.name()));
                let err_if_missing_tag = default.is_none();
                let mut tag_expr = quote! {
                    ctx.deserialize_tag(#param_index, EXPECTED_VARIANTS, #err_if_missing_tag)
                };
                if let Some(default) = default {
                    tag_expr = quote! {
                        #tag_expr.or_else(|| ::core::option::Option::Some(#default))
                    };
                }

                quote! {{
                    const EXPECTED_VARIANTS: &[&str] = &[#(#expected_variants,)*];
                    match #tag_expr? {
                        #(#match_hands)*
                        _ => ::core::unreachable!(),
                    }
                }}
            }
        };

        quote! {
            impl #cr::DeserializeConfig for #name {
                #[allow(unused_mut)]
                fn deserialize_config(
                    mut ctx: #cr::DeserializeContext<'_>,
                ) -> ::core::option::Option<Self> {
                    ::core::option::Option::Some(#instance)
                }
            }
        }
    }
}

pub(crate) fn impl_deserialize_config(input: TokenStream) -> TokenStream {
    let input: DeriveInput = syn::parse(input).unwrap();
    let trait_impl = match ConfigContainer::new(&input) {
        Ok(trait_impl) => trait_impl,
        Err(err) => return err.into_compile_error().into(),
    };
    trait_impl.derive_deserialize_config().into()
}
