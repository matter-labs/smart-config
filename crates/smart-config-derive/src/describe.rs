//! `DescribeConfig` derive macro implementation.

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{spanned::Spanned, DeriveInput, LitStr, Type};

use crate::utils::{
    wrap_in_option, Alias, ConfigContainer, ConfigContainerFields, ConfigEnumVariant, ConfigField,
    DefaultValue, RenameRule, Validation,
};

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

impl Validation {
    fn wrap(&self, cr: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let expr = &self.expr;
        if let Some(description) = &self.description {
            quote_spanned! {expr.span()=>
                #cr::validation::_private::WithDescription::new(#expr, #description)
            }
        } else {
            quote!(#expr)
        }
    }
}

impl Alias {
    fn describe(&self, cr: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let lit = &self.lit;
        let maybe_deprecated = self.is_deprecated.then(|| quote!(.deprecated()));
        quote_spanned!(lit.span()=> (#lit, #cr::metadata::AliasOptions::new()#maybe_deprecated))
    }
}

impl ConfigField {
    /// **Important.** The ordering of deserializer wrappers is important! E.g., the post-validation wrapper
    /// must come last.
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
        if let Some(deserialize_if) = &self.attrs.deserialize_if {
            let ty = Self::unwrap_option(&self.ty).expect("checked during field parsing");
            let wrapped = deserialize_if.wrap(cr);
            // A reference is required to convert to `&dyn Validate<_>`. `()`s are here to correctly handle some validation expressions
            // (e.g., `a..b` ranges; unless a range is parenthesized, `&` will be interpreted as a part of the range start).
            deserializer = quote_spanned! {deserialize_if.expr.span()=>
                #cr::de::_private::DeserializeIf::<#ty, _>::new(#deserializer, &(#wrapped))
            };
        }
        if let Some(default_fn) = &default_fn {
            deserializer = quote!(#cr::de::WithDefault::new(#deserializer, #default_fn));
        }
        if !self.attrs.validations.is_empty() {
            let validations = self.attrs.validations.iter().map(|val| {
                let wrapped = val.wrap(cr);
                // A reference is required to convert to `&dyn Validate<_>`. `()`s are here to correctly handle some validation expressions
                // (e.g., `a..b` ranges; unless a range is parenthesized, `&` will be interpreted as a part of the range start).
                quote_spanned!(val.expr.span()=> &(#wrapped))
            });
            deserializer =
                quote!(#cr::de::_private::Validated::new(#deserializer, &[#(#validations,)*]));
        }

        deserializer
    }

    fn validate_names(&self, parent: &ConfigContainer) -> proc_macro2::TokenStream {
        let name_span = self.name_span();
        let param_name = self.param_name();
        let name_validation_span = self.attrs.rename.as_ref().map_or(name_span, LitStr::span);
        let cr = parent.cr(name_validation_span);
        let name_validation = quote_spanned! {name_validation_span=>
            const _: () = #cr::metadata::_private::assert_param_name(#param_name);
        };

        let aliases = self.attrs.aliases.iter();
        let aliases_validation = aliases.map(|alias| {
            let alias = &alias.lit;
            let cr = parent.cr(alias.span());
            quote_spanned! {alias.span()=>
                const _: () = #cr::metadata::_private::assert_param_name(#alias);
            }
        });

        quote! {
            #name_validation
            #(#aliases_validation)*
        }
    }

    fn describe_param(
        &self,
        parent: &ConfigContainer,
        variant_idx: Option<usize>,
    ) -> proc_macro2::TokenStream {
        let name = &self.name;
        let name_span = self.name_span();
        let help = &self.docs;
        let param_name = self.param_name();

        let ty = &self.ty;
        let ty_in_code = if let Some(text) = ty.span().source_text() {
            quote!(#text)
        } else {
            quote!(::core::stringify!(#ty))
        };

        let default_value = DefaultValue::boxed(self.attrs.default.as_ref(), name_span, ty);
        let default_value = wrap_in_option(default_value.map(|val| quote!(|| #val)));
        let example_value = self.attrs.example.as_ref().map(
            |example| quote_spanned!(example.span()=> || ::std::boxed::Box::<#ty>::new(#example)),
        );
        let example_value = wrap_in_option(example_value);

        let fallback = self
            .attrs
            .fallback
            .as_ref()
            .map(quote::ToTokens::to_token_stream);
        let fallback = wrap_in_option(fallback);

        let cr = parent.cr(name_span);
        let deserializer = self.deserializer(&cr);
        let tag_variant = wrap_in_option(variant_idx.map(|idx| quote!(&TAG_VARIANTS[#idx])));

        let aliases = self.attrs.aliases.iter().map(|alias| alias.describe(&cr));

        quote_spanned! {name_span=> {
            let deserializer = #deserializer;

            #cr::metadata::ParamMetadata {
                name: #param_name,
                aliases: &[#(#aliases,)*],
                help: #help,
                rust_field_name: ::core::stringify!(#name),
                rust_type: #cr::metadata::RustType::of::<#ty>(#ty_in_code),
                expecting: #cr::de::_private::extract_expected_types::<#ty, _>(&deserializer),
                tag_variant: #tag_variant,
                deserializer: &#cr::de::_private::Erased::<#ty, _>::new(deserializer),
                default_value: #default_value,
                example_value: #example_value,
                fallback: #fallback,
            }
        }}
    }

    fn describe_nested_config(
        &self,
        parent: &ConfigContainer,
        variant_idx: Option<usize>,
    ) -> proc_macro2::TokenStream {
        let cr = parent.cr(self.name_span());
        let name = &self.name;
        let aliases = self.attrs.aliases.iter().map(|alias| alias.describe(&cr));
        let ty = Self::unwrap_option(&self.ty).unwrap_or(&self.ty);
        let config_name = if self.attrs.flatten {
            String::new()
        } else {
            self.param_name()
        };
        let tag_variant = wrap_in_option(variant_idx.map(|idx| quote!(&TAG_VARIANTS[#idx])));

        quote_spanned! {self.name_span()=>
            #cr::metadata::NestedConfigMetadata {
                name: #config_name,
                aliases: &[#(#aliases,)*],
                rust_field_name: ::core::stringify!(#name),
                tag_variant: #tag_variant,
                meta: &<#ty as #cr::DescribeConfig>::DESCRIPTION,
            }
        }
    }
}

impl ConfigEnumVariant {
    fn describe(
        &self,
        cr: &proc_macro2::TokenStream,
        rename_rule: Option<RenameRule>,
    ) -> proc_macro2::TokenStream {
        let name = self.name(rename_rule);
        let rust_name = &self.name;
        let aliases = self.attrs.aliases.iter();
        let help = &self.attrs.help;

        quote_spanned! {self.name.span()=>
            #cr::metadata::ConfigVariant {
                name: #name,
                aliases: &[#(#aliases,)*],
                rust_name: ::core::stringify!(#rust_name),
                help: #help,
            }
        }
    }

    fn visit_match_arm(
        &self,
        variant_idx: usize,
        start_param_offset: &mut usize,
        start_config_offset: &mut usize,
    ) -> proc_macro2::TokenStream {
        let name = &self.name;
        let mut bindings = vec![];
        let mut params = vec![];
        let mut nested_configs = vec![];
        for (i, field) in self.fields.iter().enumerate() {
            let is_config = field.attrs.nest;
            let field = &field.name;
            let field_binding = quote::format_ident!("__{i}");

            bindings.push(quote_spanned!(field.span()=> #field: #field_binding));
            if is_config {
                nested_configs.push(quote_spanned! {field.span()=>
                    visitor.visit_nested_config(#start_config_offset, #field_binding)
                });
                *start_config_offset += 1;
            } else {
                params.push(quote_spanned! {field.span()=>
                    visitor.visit_param(#start_param_offset, #field_binding)
                });
                *start_param_offset += 1;
            }
        }

        quote_spanned! {name.span()=>
            Self::#name { #(#bindings,)* .. } => {
                visitor.visit_tag(#variant_idx);
                #(#params;)*
                #(#nested_configs;)*
            }
        }
    }
}

impl ConfigContainer {
    fn derive_visit_config(&self) -> proc_macro2::TokenStream {
        let name = &self.name;
        let cr = self.cr(name.span());

        let visit_impl = match &self.fields {
            ConfigContainerFields::Struct(fields) => {
                let params = fields.iter().filter(|field| !field.attrs.nest);
                let params = params.enumerate().map(|(i, field)| {
                    let field = &field.name;
                    quote_spanned!(field.span()=> visitor.visit_param(#i, &self.#field))
                });

                let nested_configs = fields.iter().filter(|field| field.attrs.nest);
                let nested_configs = nested_configs.enumerate().map(|(i, field)| {
                    let field = &field.name;
                    quote_spanned!(field.span()=> visitor.visit_nested_config(#i, &self.#field))
                });

                quote!(#(#params;)* #(#nested_configs;)*)
            }
            ConfigContainerFields::Enum { variants, .. } => {
                let mut start_param_offset = 0;
                let mut start_nested_config_offset = 0;
                let match_arms = variants.iter().enumerate().map(|(variant_idx, variant)| {
                    variant.visit_match_arm(
                        variant_idx,
                        &mut start_param_offset,
                        &mut start_nested_config_offset,
                    )
                });
                quote! {
                    match self {
                        #(#match_arms)*
                    }
                }
            }
        };

        quote! {
            impl #cr::visit::VisitConfig for #name {
                fn visit_config(&self, visitor: &mut dyn #cr::visit::ConfigVisitor) {
                    #visit_impl
                }
            }
        }
    }

    fn erase_validation(
        validation: &Validation,
        cr: &proc_macro2::TokenStream,
        ty: &impl quote::ToTokens,
    ) -> proc_macro2::TokenStream {
        let validation = validation.wrap(cr);
        quote! {
            &#cr::validation::_private::ErasedValidation::<#ty, _>::new(#validation)
        }
    }

    fn derive_describe_config(&self) -> proc_macro2::TokenStream {
        let name = &self.name;
        let cr = self.cr(name.span());
        let name_str = name.to_string();
        let help = &self.help;

        let all_fields = self.fields.all_fields();
        let validations = all_fields
            .iter()
            .filter(|(_, field)| !field.attrs.flatten)
            .map(|(_, field)| field.validate_names(self));

        let is_enum = matches!(&self.fields, ConfigContainerFields::Enum { .. });
        let params = all_fields
            .iter()
            .filter(|(_, field)| !field.attrs.nest)
            .map(|(variant_idx, field)| {
                field.describe_param(self, is_enum.then_some(*variant_idx))
            });
        let mut params: Vec<_> = params.collect();

        let mut tag_variants_const = None;
        let mut tag_description = None;
        if let ConfigContainerFields::Enum { tag, variants } = &self.fields {
            // Add the tag field description
            let (default_variant_idx, default) = variants
                .iter()
                .enumerate()
                .find_map(|(i, variant)| {
                    variant
                        .attrs
                        .default
                        .then(|| (i, variant.name(self.attrs.rename_all)))
                })
                .unzip();

            let expected_variants = variants
                .iter()
                .flat_map(|variant| variant.expected_variants(self.attrs.rename_all));

            let tag_span = tag.span();
            let tag = ConfigField::from_tag(&cr, tag, expected_variants, default.as_deref());
            let tag_index = params.len();
            params.push(tag.describe_param(self, None));

            let tag_variants = variants
                .iter()
                .map(|variant| variant.describe(&cr, self.attrs.rename_all));

            tag_variants_const = Some(quote_spanned! {tag_span=>
                const TAG_VARIANTS: &[#cr::metadata::ConfigVariant] = &[#(#tag_variants,)*];
            });
            let default_variant =
                wrap_in_option(default_variant_idx.map(|i| quote!(&TAG_VARIANTS[#i])));
            tag_description = Some(quote_spanned! {tag_span=>
                #cr::metadata::ConfigTag {
                    param: &PARAMS[#tag_index],
                    variants: TAG_VARIANTS,
                    default_variant: #default_variant,
                }
            });
        }
        let tag_description = wrap_in_option(tag_description);

        let nested_configs = all_fields.iter().filter_map(|(variant_idx, field)| {
            if field.attrs.nest {
                return Some(field.describe_nested_config(self, is_enum.then_some(*variant_idx)));
            }
            None
        });

        let config_validations = self
            .attrs
            .validations
            .iter()
            .map(|val| Self::erase_validation(val, &cr, &name));

        quote! {
            impl #cr::DescribeConfig for #name {
                const DESCRIPTION: #cr::metadata::ConfigMetadata = {
                    const PARAMS: &[#cr::metadata::ParamMetadata] = &[#(#params,)*];
                    #tag_variants_const

                    #cr::metadata::ConfigMetadata {
                        ty: #cr::metadata::RustType::of::<#name>(#name_str),
                        help: #help,
                        params: PARAMS,
                        tag: #tag_description,
                        nested_configs: &[#(#nested_configs,)*],
                        deserializer: |ctx| {
                            use #cr::metadata::_private::DeserializeBoxedConfig as _;
                            let receiver = &::core::marker::PhantomData::<#name>;
                            receiver.deserialize_boxed_config(ctx)
                        },
                        visitor: #cr::metadata::_private::box_config_visitor::<Self>(),
                        validations: &[#(#config_validations,)*],
                    }
                };
            }

            #(#validations)*
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
        let visit_impl = self.derive_visit_config();
        let describe_impl = self.derive_describe_config();
        let default_impl = if self.attrs.derive_default {
            Some(self.derive_default()?)
        } else {
            None
        };
        Ok(quote!(#visit_impl #describe_impl #default_impl))
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
