use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::DeriveInput;

use crate::utils::{ConfigContainer, ConfigContainerFields, ConfigField};

impl ConfigField {
    fn deserialize_param(
        &self,
        cr: &proc_macro2::TokenStream,
        index: usize,
    ) -> proc_macro2::TokenStream {
        let name = &self.name;
        let name_span = self.name.span();
        let param_name = self.param_name();

        let default_fallback = match &self.attrs.default {
            None if !Self::is_option(&self.ty) => {
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

impl ConfigContainer {
    fn derive_deserialize_config(&self) -> proc_macro2::TokenStream {
        let cr = self.cr();
        let name = &self.name;

        let mut param_index = 0;
        let mut nested_index = 0;
        let instance = match &self.fields {
            ConfigContainerFields::Struct(fields) => {
                let fields = fields.iter().map(|field| {
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
                quote!(Self { #(#fields,)* })
            }
            ConfigContainerFields::Enum { .. } => todo!(),
        };

        quote! {
            impl #cr::DeserializeConfig for #name {
                fn deserialize_config(
                    deserializer: #cr::ValueDeserializer<'_>,
                ) -> ::core::result::Result<Self, #cr::ParseError> {
                    let deserializer = deserializer.for_config::<Self>();
                    ::core::result::Result::Ok(#instance)
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
