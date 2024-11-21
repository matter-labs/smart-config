use std::iter;

use proc_macro::TokenStream;
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
        let name = self
            .attrs
            .rename
            .as_ref()
            .map(LitStr::value)
            .unwrap_or_else(|| self.name.to_string());
        iter::once(name).chain(self.attrs.aliases.iter().map(LitStr::value))
    }
}

impl ConfigContainer {
    fn process_fields<'a>(
        fields: &'a [ConfigField],
        cr: &'a proc_macro2::TokenStream,
        param_index: &'a mut usize,
        nested_index: &'a mut usize,
    ) -> impl Iterator<Item = proc_macro2::TokenStream> + 'a {
        fields.iter().map(move |field| {
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
            quote_spanned!(name.span()=> #name: #value)
        })
    }

    fn derive_deserialize_config(&self) -> proc_macro2::TokenStream {
        let cr = self.cr();
        let name = &self.name;

        let mut param_index = 0;
        let mut nested_index = 0;
        let instance = match &self.fields {
            ConfigContainerFields::Struct(fields) => {
                let fields = Self::process_fields(fields, &cr, &mut param_index, &mut nested_index);
                quote!(Self { #(#fields,)* })
            }
            ConfigContainerFields::Enum { tag, variants } => {
                let match_hands = variants.iter().map(|variant| {
                    let name = &variant.name;
                    let matches = variant.matches();
                    let variant_fields = Self::process_fields(
                        &variant.fields,
                        &cr,
                        &mut param_index,
                        &mut nested_index,
                    );
                    quote!(#matches => Self::#name { #(#variant_fields,)* })
                });
                let match_hands: Vec<_> = match_hands.collect();
                let tag_name = tag.value();
                let expected_variants = variants
                    .iter()
                    .flat_map(ConfigEnumVariant::expected_variants);

                quote! {{
                    const EXPECTED_VARIANTS: &[&str] = &[#(#expected_variants,)*];
                    match deserializer.deserialize_tag(#param_index, #tag_name, EXPECTED_VARIANTS)? {
                        #(#match_hands,)*
                        _ => ::core::unreachable!(),
                    }
                }}
            }
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
