//! `DescribeConfig` derive macro implementation.

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{spanned::Spanned, DeriveInput, LitStr, Type};

use crate::utils::{ConfigContainer, ConfigContainerFields, ConfigField, DefaultValue};

impl DefaultValue {
    fn boxed(
        this: Option<&Self>,
        span: proc_macro2::Span,
        ty: &Type,
    ) -> Option<proc_macro2::TokenStream> {
        match this {
            None if !ConfigField::is_option(ty) => None,
            Some(Self::DefaultTrait) | None => Some(quote_spanned! {span=>
                <::std::boxed::Box<#ty> as ::core::default::Default>::default()
            }),
            Some(Self::Path(path)) => {
                Some(quote_spanned!(span=> ::std::boxed::Box::<#ty>::new(#path())))
            }
            Some(Self::Expr(expr)) => {
                Some(quote_spanned!(span=> ::std::boxed::Box::<#ty>::new(#expr)))
            }
        }
    }
}

impl ConfigField {
    fn deserializer(&self, cr: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let mut deserializer = if let Some(with) = &self.attrs.with {
            quote!(#with)
        } else {
            let ty = &self.ty;
            quote_spanned!(ty.span()=> <#ty as #cr::de::WellKnown>::DE)
        };
        let default_fn = self.default_fn();

        if self.attrs.is_secret {
            deserializer = quote!(#cr::de::Secret(#deserializer));
        }
        if let Some(default_fn) = &default_fn {
            deserializer = quote!(#cr::de::WithDefault::new(#deserializer, #default_fn));
        }
        deserializer
    }

    fn validate_param(&self, parent: &ConfigContainer) -> proc_macro2::TokenStream {
        let name_span = self.name_span();
        let param_name = self.param_name();
        let name_validation_span = self.attrs.rename.as_ref().map_or(name_span, LitStr::span);
        let cr = parent.cr(name_validation_span);
        let name_validation = quote_spanned! {name_validation_span=>
            const _: () = #cr::metadata::validation::assert_param_name(#param_name);
        };

        let aliases = self.attrs.aliases.iter();
        let aliases_validation = aliases.map(|alias| {
            let cr = parent.cr(alias.span());
            quote_spanned! {alias.span()=>
                const _: () = #cr::metadata::validation::assert_param_name(#alias);
            }
        });

        quote! {
            #name_validation
            #(#aliases_validation)*
        }
    }

    fn describe_param(&self, parent: &ConfigContainer) -> proc_macro2::TokenStream {
        let name = &self.name;
        let name_span = self.name_span();
        let aliases = self.attrs.aliases.iter();
        let help = &self.docs;
        let param_name = self.param_name();

        let ty = &self.ty;
        let ty_in_code = if let Some(text) = ty.span().source_text() {
            quote!(#text)
        } else {
            quote!(::core::stringify!(#ty))
        };

        let default_value = DefaultValue::boxed(self.attrs.default.as_ref(), name_span, ty);
        let default_value = if let Some(value) = default_value {
            quote_spanned!(name_span=> ::core::option::Option::Some(|| #value))
        } else {
            quote_spanned!(name_span=> ::core::option::Option::None)
        };

        let cr = parent.cr(name_span);
        let deserializer = self.deserializer(&cr);
        quote_spanned! {name_span=> {
            let deserializer = #deserializer;

            #cr::metadata::ParamMetadata {
                name: #param_name,
                aliases: &[#(#aliases,)*],
                help: #help,
                rust_field_name: ::core::stringify!(#name),
                rust_type: #cr::metadata::RustType::of::<#ty>(#ty_in_code),
                expecting: #cr::de::_private::extract_expected_types::<#ty, _>(&deserializer),
                deserializer: &#cr::de::_private::Erased::<#ty, _>::new(deserializer),
                default_value: #default_value,
            }
        }}
    }

    fn describe_nested_config(&self, parent: &ConfigContainer) -> proc_macro2::TokenStream {
        let cr = parent.cr(self.name_span());
        let name = &self.name;
        let ty = Self::unwrap_option(&self.ty).unwrap_or(&self.ty);
        let config_name = if self.attrs.flatten {
            String::new()
        } else {
            self.param_name()
        };

        quote_spanned! {self.name_span()=>
            #cr::metadata::NestedConfigMetadata {
                name: #config_name,
                rust_field_name: ::core::stringify!(#name),
                meta: &<#ty as #cr::DescribeConfig>::DESCRIPTION,
            }
        }
    }
}

impl ConfigContainer {
    fn derive_describe_config(&self) -> proc_macro2::TokenStream {
        let name = &self.name;
        let cr = self.cr(name.span());
        let name_str = name.to_string();
        let help = &self.help;

        let all_fields = self.fields.all_fields();
        let params = all_fields.iter().filter_map(|field| {
            if !field.attrs.nest {
                return Some((field.validate_param(self), field.describe_param(self)));
            }
            None
        });
        let (param_validation, mut params): (Vec<_>, Vec<_>) = params.unzip();

        if let ConfigContainerFields::Enum { tag, variants } = &self.fields {
            // Add the tag field description
            let default = variants.iter().find_map(|variant| {
                variant
                    .attrs
                    .default
                    .then(|| variant.name(self.attrs.rename_all))
            });
            let expected_variants = variants
                .iter()
                .flat_map(|variant| variant.expected_variants(self.attrs.rename_all));
            let tag = ConfigField::from_tag(&cr, tag, expected_variants, default.as_deref());
            params.push(tag.describe_param(self));
        }

        let nested_configs = all_fields.iter().filter_map(|field| {
            if field.attrs.nest {
                return Some(field.describe_nested_config(self));
            }
            None
        });

        quote! {
            impl #cr::DescribeConfig for #name {
                const DESCRIPTION: #cr::metadata::ConfigMetadata = #cr::metadata::ConfigMetadata {
                    ty: #cr::metadata::RustType::of::<#name>(#name_str),
                    help: #help,
                    params: &[#(#params,)*],
                    nested_configs: &[#(#nested_configs,)*],
                };
            }

            #(#param_validation)*
            const _: () = <#name as #cr::DescribeConfig>::DESCRIPTION.assert_valid();
        }
    }

    fn default_fields(fields: &[ConfigField]) -> syn::Result<Vec<proc_macro2::TokenStream>> {
        let fields = fields.iter().map(|field| {
            let name = &field.name;
            let name_span = field.name.span();
            let field_instance = if let Some(default) = &field.attrs.default {
                default.instance(name_span)
            } else if ConfigField::is_option(&field.ty) {
                quote_spanned!(field.ty.span()=> ::core::option::Option::None)
            } else if field.attrs.nest {
                // Attempt to use `Default` impl
                quote_spanned!(field.ty.span()=> ::core::default::Default::default())
            } else {
                let msg = "Cannot derive(Default): field does not have a default value";
                return Err(syn::Error::new(name_span, msg));
            };
            Ok(quote_spanned!(name_span=> #name: #field_instance))
        });
        fields.collect()
    }

    fn derive_default(&self) -> syn::Result<proc_macro2::TokenStream> {
        let instance = match &self.fields {
            ConfigContainerFields::Struct(fields) => {
                let fields = Self::default_fields(fields)?;
                quote!(Self { #(#fields,)* })
            }
            ConfigContainerFields::Enum { variants, .. } => {
                let default_variant = variants
                    .iter()
                    .find(|variant| variant.attrs.default)
                    .ok_or_else(|| {
                        let msg = "Cannot derive(Default): enum does not have a variant marked with #[config(default)]";
                        syn::Error::new(self.name.span(), msg)
                    })?;
                let fields = Self::default_fields(&default_variant.fields)?;
                let variant_name = &default_variant.name;
                quote!(Self::#variant_name { #(#fields,)* })
            }
        };

        let name = &self.name;
        Ok(quote! {
            impl ::core::default::Default for #name {
                fn default() -> Self {
                    #instance
                }
            }
        })
    }

    fn derive_all(&self) -> syn::Result<proc_macro2::TokenStream> {
        let describe_impl = self.derive_describe_config();
        let default_impl = if self.attrs.derive_default {
            Some(self.derive_default()?)
        } else {
            None
        };
        Ok(quote!(#describe_impl #default_impl))
    }
}

pub(crate) fn impl_describe_config(input: TokenStream) -> TokenStream {
    let input: DeriveInput = syn::parse(input).unwrap();
    let trait_impl = match ConfigContainer::new(&input) {
        Ok(trait_impl) => trait_impl,
        Err(err) => return err.into_compile_error().into(),
    };
    match trait_impl.derive_all() {
        Ok(derived) => derived.into(),
        Err(err) => err.into_compile_error().into(),
    }
}
