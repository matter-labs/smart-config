use std::iter;

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{quote, quote_spanned};
use syn::{spanned::Spanned, DeriveInput, LitStr};

use crate::utils::{
    ConfigContainer, ConfigContainerFields, ConfigEnumVariant, ConfigField, DefaultValue,
};

impl DefaultValue {
    fn fallback_fn(
        this: Option<&Self>,
        span: proc_macro2::Span,
        is_option: bool,
    ) -> proc_macro2::TokenStream {
        match this {
            None if !is_option => {
                quote_spanned!(span=> ::core::option::Option::None)
            }
            Some(Self::DefaultTrait) | None => {
                quote_spanned!(span=> ::core::option::Option::Some(::core::default::Default::default))
            }
            Some(Self::Path(def_fn)) => {
                quote_spanned!(span=> ::core::option::Option::Some(#def_fn))
            }
            Some(Self::Expr(expr)) => quote_spanned!(span=> ::core::option::Option::Some(|| #expr)),
        }
    }
}

impl ConfigField {
    /// Returns `Option<_>`.
    fn deserialize_param(
        &self,
        cr: &proc_macro2::TokenStream,
        index: usize,
    ) -> proc_macro2::TokenStream {
        let name_span = self.name.span();
        let param_name = self.param_name();
        let is_option = Self::is_option(&self.ty);
        let default_fallback =
            DefaultValue::fallback_fn(self.attrs.default.as_ref(), name_span, is_option);

        if !self.attrs.nest {
            quote_spanned! {name_span=>
                match deserializer.deserialize_param(
                    #index,
                    #param_name,
                    #default_fallback,
                ) {
                    ::core::result::Result::Ok(value) => ::core::option::Option::Some(value),
                    ::core::result::Result::Err(err) => {
                        errors.push(err);
                        ::core::option::Option::None
                    }
                }
            }
        } else if self.attrs.flatten {
            quote_spanned! {name_span=>
                #cr::DeserializeConfig::deserialize_config_full(
                    deserializer.for_flattened_config(),
                    errors,
                )
            }
        } else {
            quote_spanned! {name_span=>
                deserializer.deserialize_nested_config(
                    errors,
                    #index,
                    #param_name,
                    #default_fallback,
                )
            }
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
            ConfigContainerFields::Enum { tag, variants } => {
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

                let tag_name = tag.value();
                let expected_variants = variants
                    .iter()
                    .flat_map(ConfigEnumVariant::expected_variants);
                let default = variants
                    .iter()
                    .find_map(|variant| variant.attrs.default.then(|| variant.name()));
                let default = if let Some(val) = default {
                    quote!(::core::option::Option::Some(#val))
                } else {
                    quote!(::core::option::Option::None)
                };

                quote! {{
                    const EXPECTED_VARIANTS: &[&str] = &[#(#expected_variants,)*];
                    let __tag = match deserializer.deserialize_tag(
                        #param_index,
                        #tag_name,
                        EXPECTED_VARIANTS,
                        #default,
                    ) {
                        ::core::result::Result::Ok(tag) => tag,
                        ::core::result::Result::Err(err) => {
                            errors.push(err);
                            return ::core::option::Option::None;
                        }
                    };
                    match __tag {
                        #(#match_hands)*
                        _ => ::core::unreachable!(),
                    }
                }}
            }
        };

        quote! {
            impl #cr::DeserializeConfig for #name {
                fn deserialize_config_full(
                    deserializer: #cr::ValueDeserializer<'_>,
                    errors: &mut #cr::ParseErrors,
                ) -> ::core::option::Option<Self> {
                    let deserializer = deserializer.for_config::<Self>();
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
