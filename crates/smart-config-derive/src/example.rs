use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, quote_spanned};
use syn::DeriveInput;

use crate::utils::{ConfigContainer, ConfigContainerFields, ConfigField};

impl ConfigField {
    fn example_initializer(
        &self,
        cr: &proc_macro2::TokenStream,
    ) -> syn::Result<proc_macro2::TokenStream> {
        let name = &self.name;
        let name_span = self.name_span();
        let val = if let Some(example) = &self.attrs.example {
            // `example` attribute takes precedence, even if it's specified on a config
            quote!(#example)
        } else if let Some(default) = &self.attrs.default {
            default.instance(name_span)
        } else if self.attrs.nest {
            quote_spanned!(name_span=> #cr::ExampleConfig::example_config())
        } else {
            let msg = "example or default value required to derive `ExampleConfig`";
            return Err(syn::Error::new(name_span, msg));
        };
        Ok(quote_spanned!(name_span=> #name: #val))
    }
}

impl ConfigContainer {
    fn derive_example(&self) -> syn::Result<proc_macro2::TokenStream> {
        let name = &self.name;
        let cr = self.cr(Span::call_site());

        let example_impl = match &self.fields {
            ConfigContainerFields::Struct(fields) => {
                let fields: syn::Result<Vec<_>> = fields
                    .iter()
                    .map(|field| field.example_initializer(&cr))
                    .collect();
                let fields = fields?;
                quote!(Self { #(#fields,)* })
            }
            ConfigContainerFields::Enum { .. } => {
                let mut msg = "Deriving `ExampleConfig` for enum configs isn't supported yet; implement `ExampleConfig` manually".to_owned();
                if self.attrs.derive_default {
                    msg += " (e.g., by delegating to `Default::default()`)";
                }
                return Err(syn::Error::new(name.span(), msg));
            }
        };

        Ok(quote! {
            impl #cr::ExampleConfig for #name {
                fn example_config() -> Self {
                    #example_impl
                }
            }
        })
    }
}

pub(crate) fn impl_example_config(input: TokenStream) -> TokenStream {
    let input: DeriveInput = syn::parse(input).unwrap();
    let trait_impl = match ConfigContainer::new(&input) {
        Ok(trait_impl) => trait_impl,
        Err(err) => return err.into_compile_error().into(),
    };
    match trait_impl.derive_example() {
        Ok(derived) => derived.into(),
        Err(err) => err.into_compile_error().into(),
    }
}
