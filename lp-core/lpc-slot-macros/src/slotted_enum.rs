use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, Result};

use crate::{attr, slotted::validate_slot_name};

pub(crate) fn derive_enum(
    ident: syn::Ident,
    data: syn::DataEnum,
    container_attrs: attr::ContainerAttrs,
) -> Result<TokenStream> {
    let variant_count = data.variants.len();
    let encoding_tokens = enum_encoding_tokens(container_attrs.enum_encoding);
    let mut only_variant_slot_name = None::<String>;
    let mut variant_shapes = Vec::new();
    let mut variant_arms = Vec::new();
    let mut data_arms = Vec::new();
    let mut data_mut_arms = Vec::new();
    let mut default_arms = Vec::new();
    let mut record_access_arms = Vec::new();
    let mut record_mut_access_arms = Vec::new();
    let mut expected_names = Vec::new();
    let mut default_variant = None::<String>;

    for variant in data.variants {
        let variant_ident = variant.ident;
        let variant_attrs = attr::parse_variant(&variant.attrs)?;

        let slot_name = if let Some(name) = &variant_attrs.name {
            name.value()
        } else {
            variant_slot_name(&variant_ident, container_attrs.rename_all)
        };
        validate_slot_name(&slot_name, variant_ident.span())?;
        if variant_count == 1 {
            only_variant_slot_name = Some(slot_name.clone());
        }
        if variant_attrs.is_default && default_variant.replace(slot_name.clone()).is_some() {
            return Err(syn::Error::new_spanned(
                variant_ident,
                "Slotted enum supports only one #[default] variant",
            ));
        }
        expected_names.push(slot_name.clone());

        match variant.fields {
            Fields::Unit => {
                variant_shapes.push(quote! {
                    ::lpc_model::slot::shape::variant(#slot_name, ::lpc_model::slot::shape::unit())
                });
                variant_arms.push(quote! { Self::#variant_ident => #slot_name, });
                data_arms.push(quote! {
                    Self::#variant_ident => ::lpc_model::SlotDataAccess::Unit(::lpc_model::Revision::default()),
                });
                data_mut_arms.push(quote! {
                    Self::#variant_ident => panic!("unit enum variant data is mutably owned by EnumSlot"),
                });
                default_arms.push(quote! { #slot_name => Ok(Self::#variant_ident), });
                record_access_arms.push(quote! { Self::#variant_ident => None, });
                record_mut_access_arms.push(quote! { Self::#variant_ident => None, });
            }
            Fields::Unnamed(fields) => {
                if fields.unnamed.len() != 1 {
                    return Err(syn::Error::new_spanned(
                        fields,
                        "Slotted enum tuple variants must contain exactly one field",
                    ));
                }
                let field_ty = &fields.unnamed[0].ty;
                variant_shapes.push(quote! {
                    ::lpc_model::slot::shape::variant(
                        #slot_name,
                        <#field_ty as ::lpc_model::FieldSlot>::slot_field_shape(),
                    )
                });
                variant_arms.push(quote! { Self::#variant_ident(_) => #slot_name, });
                data_arms.push(quote! {
                    Self::#variant_ident(value) => <#field_ty as ::lpc_model::FieldSlot>::slot_field_data(value),
                });
                data_mut_arms.push(quote! {
                    Self::#variant_ident(value) => <#field_ty as ::lpc_model::FieldSlotMut>::slot_field_data_mut(value),
                });
                default_arms.push(quote! {
                    #slot_name => Ok(Self::#variant_ident(::core::default::Default::default())),
                });
                record_access_arms.push(quote! { Self::#variant_ident(_) => None, });
                record_mut_access_arms.push(quote! { Self::#variant_ident(_) => None, });
            }
            Fields::Named(fields) => {
                let mut shape_fields = Vec::new();
                let mut access_arms = Vec::new();
                let mut mut_access_arms = Vec::new();
                let mut field_idents = Vec::new();
                let mut field_defaults = Vec::new();

                for (index, field) in fields.named.into_iter().enumerate() {
                    let Some(field_ident) = field.ident else {
                        continue;
                    };
                    let field_attr = attr::parse_field(&field.attrs)?;
                    let field_name = field_attr
                        .name
                        .as_ref()
                        .map_or_else(|| field_ident.to_string(), syn::LitStr::value);
                    validate_slot_name(&field_name, field_ident.span())?;

                    let field_ty = field.ty;
                    let shape = attr::field_shape_tokens(&field_attr.shape, &field_ty);
                    shape_fields.push(quote! {
                        ::lpc_model::slot::shape::field(#field_name, #shape)
                    });

                    if let Some(access) = attr::field_binding_access_tokens(
                        &field_attr.shape,
                        &field_ty,
                        &field_ident,
                    ) {
                        let index = syn::Index::from(index);
                        access_arms.push(quote! { #index => Some(#access), });
                    }
                    if let Some(mut_access) = attr::field_binding_mut_access_tokens(
                        &field_attr.shape,
                        &field_ty,
                        &field_ident,
                    ) {
                        let index = syn::Index::from(index);
                        mut_access_arms.push(quote! { #index => Some(#mut_access), });
                    }

                    field_defaults.push(quote! {
                        #field_ident: ::core::default::Default::default()
                    });
                    field_idents.push(field_ident);
                }

                variant_shapes.push(quote! {
                    ::lpc_model::slot::shape::variant(
                        #slot_name,
                        ::lpc_model::slot::shape::record(::lpc_model::__private::Vec::from([
                            #(#shape_fields),*
                        ])),
                    )
                });
                variant_arms.push(quote! { Self::#variant_ident { .. } => #slot_name, });
                data_arms.push(quote! {
                    Self::#variant_ident { .. } => ::lpc_model::SlotDataAccess::Record(self),
                });
                data_mut_arms.push(quote! {
                    Self::#variant_ident { .. } => ::lpc_model::SlotDataMutAccess::Record(self),
                });
                default_arms.push(quote! {
                    #slot_name => Ok(Self::#variant_ident {
                        #(#field_defaults),*
                    }),
                });
                record_access_arms.push(quote! {
                    Self::#variant_ident { #(#field_idents),* } => match index {
                        #(#access_arms)*
                        _ => None,
                    },
                });
                record_mut_access_arms.push(quote! {
                    Self::#variant_ident { #(#field_idents),* } => match index {
                        #(#mut_access_arms)*
                        _ => None,
                    },
                });
            }
        }
    }

    let default_variant = if let Some(variant) = default_variant {
        variant
    } else if variant_count == 1 {
        only_variant_slot_name.expect("checked one variant")
    } else {
        return Err(syn::Error::new_spanned(
            ident,
            "Slotted enum with multiple variants requires one #[default] variant",
        ));
    };

    let expected_display = expected_names.join(", ");

    Ok(quote! {
        impl ::core::default::Default for #ident {
            fn default() -> Self {
                Self::__slotted_default_variant(#default_variant)
                    .expect("Slotted enum default variant is generated from a valid variant")
            }
        }

        impl #ident {
            fn __slotted_default_variant(variant: &str) -> Result<Self, ::lpc_model::SlotMutationError> {
                match variant {
                    #(#default_arms)*
                    other => {
                        let mut message = ::lpc_model::__private::String::from(concat!(
                            "unknown ",
                            stringify!(#ident),
                            " variant `",
                        ));
                        message.push_str(other);
                        message.push_str(concat!(
                            "`; expected one of: ",
                            #expected_display,
                        ));
                        Err(::lpc_model::SlotMutationError::unknown_variant(message))
                    }
                }
            }
        }

        impl ::lpc_model::SlotEnumShape for #ident {
            fn slot_enum_shape() -> ::lpc_model::SlotShape {
                ::lpc_model::SlotShape::Enum {
                    meta: ::lpc_model::SlotMeta::empty(),
                    encoding: #encoding_tokens,
                    variants: ::lpc_model::__private::Vec::from([
                        #(#variant_shapes),*
                    ]),
                }
            }
        }

        impl ::lpc_model::SlottedEnum for #ident {
            fn variant(&self) -> &str {
                match self {
                    #(#variant_arms)*
                }
            }

            fn data(&self) -> ::lpc_model::SlotDataAccess<'_> {
                match self {
                    #(#data_arms)*
                }
            }
        }

        impl ::lpc_model::SlottedEnumMut for #ident {
            fn data_mut(&mut self) -> ::lpc_model::SlotDataMutAccess<'_> {
                match self {
                    #(#data_mut_arms)*
                }
            }

            fn set_variant_default(&mut self, variant: &str) -> Result<(), ::lpc_model::SlotMutationError> {
                *self = Self::__slotted_default_variant(variant)?;
                Ok(())
            }
        }

        impl ::lpc_model::SlotRecordAccess for #ident {
            fn field(&self, index: usize) -> Option<::lpc_model::SlotDataAccess<'_>> {
                match self {
                    #(#record_access_arms)*
                }
            }
        }

        impl ::lpc_model::SlotRecordMutAccess for #ident {
            fn field_mut(&mut self, index: usize) -> Option<::lpc_model::SlotDataMutAccess<'_>> {
                match self {
                    #(#record_mut_access_arms)*
                }
            }
        }
    })
}

fn enum_encoding_tokens(encoding: Option<attr::EnumEncodingAttr>) -> TokenStream {
    match encoding.unwrap_or(attr::EnumEncodingAttr::Tagged) {
        attr::EnumEncodingAttr::Tagged => quote! { ::lpc_model::SlotEnumEncoding::default() },
        attr::EnumEncodingAttr::External => quote! { ::lpc_model::SlotEnumEncoding::External },
    }
}

fn variant_slot_name(ident: &syn::Ident, rename_all: Option<attr::RenameAllAttr>) -> String {
    let raw = ident.to_string();
    match rename_all {
        Some(attr::RenameAllAttr::SnakeCase) => to_snake_case(&raw),
        None => raw,
    }
}

fn to_snake_case(input: &str) -> String {
    let mut out = String::new();
    let mut prev_was_lower_or_digit = false;
    for ch in input.chars() {
        if ch.is_ascii_uppercase() {
            if prev_was_lower_or_digit {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            prev_was_lower_or_digit = false;
        } else {
            out.push(ch);
            prev_was_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}
