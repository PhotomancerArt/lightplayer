use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Result, parse2};

use crate::attr::{self, FieldShapeAttr};

pub(crate) fn derive(input: TokenStream) -> TokenStream {
    match derive_inner(input) {
        Ok(tokens) => tokens,
        Err(err) => err.to_compile_error(),
    }
}

fn derive_inner(input: TokenStream) -> Result<TokenStream> {
    let input = parse2::<DeriveInput>(input)?;
    let ident = input.ident;
    let container_attrs = attr::parse_container(&input.attrs)?;
    let fields = named_fields(input.data)?;

    let mut shape_fields = Vec::new();
    let mut access_arms = Vec::new();
    let mut access_index = 0usize;

    for field in fields.named {
        let Some(field_ident) = field.ident else {
            continue;
        };
        let field_attr = attr::parse_field(&field.attrs)?;
        if matches!(field_attr.shape, FieldShapeAttr::Skip) {
            continue;
        }

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

        if let Some(access) = attr::field_access_tokens(&field_attr.shape, &field_ty, &field_ident)
        {
            let index = syn::Index::from(access_index);
            access_arms.push(quote! {
                #index => Some(#access),
            });
            access_index += 1;
        }
    }

    let shape_id = if let Some(shape_id) = container_attrs.shape_id {
        quote! { ::lpc_model::SlotShapeId::from_static_name(#shape_id) }
    } else {
        quote! {
            ::lpc_model::SlotShapeId::from_static_name(
                concat!(module_path!(), "::", stringify!(#ident)),
            )
        }
    };

    let static_impls = quote! {
        impl ::lpc_model::SlotAccess for #ident {
            fn shape_id(&self) -> ::lpc_model::SlotShapeId {
                <Self as ::lpc_model::StaticSlotShape>::SHAPE_ID
            }

            fn data(&self) -> ::lpc_model::SlotDataAccess<'_> {
                ::lpc_model::SlotDataAccess::Record(self)
            }
        }

        impl ::lpc_model::StaticSlotShape for #ident {
            const SHAPE_ID: ::lpc_model::SlotShapeId =
                #shape_id;

            fn shape_name() -> Option<&'static str> {
                Some(concat!(module_path!(), "::", stringify!(#ident)))
            }

            fn slot_shape() -> ::lpc_model::SlotShape {
                <Self as ::lpc_model::SlotRecordShape>::slot_record_shape()
            }
        }

        impl ::lpc_model::StaticSlotAccess for #ident {}
    };

    Ok(quote! {
        impl ::lpc_model::SlotRecordShape for #ident {
            fn slot_record_shape() -> ::lpc_model::SlotShape {
                ::lpc_model::slot::shape::record(::lpc_model::__private::Vec::from([
                    #(#shape_fields),*
                ]))
            }
        }

        impl ::lpc_model::SlotRecordAccess for #ident {
            fn field(&self, index: usize) -> Option<::lpc_model::SlotDataAccess<'_>> {
                match index {
                    #(#access_arms)*
                    _ => None,
                }
            }
        }

        impl ::lpc_model::SlotMapValueAccess for #ident {
            fn slot_data(&self) -> ::lpc_model::SlotDataAccess<'_> {
                ::lpc_model::SlotDataAccess::Record(self)
            }
        }

        impl ::lpc_model::FieldSlot for #ident {
            fn slot_field_shape() -> ::lpc_model::SlotShape {
                <Self as ::lpc_model::SlotRecordShape>::slot_record_shape()
            }

            fn slot_field_data(&self) -> ::lpc_model::SlotDataAccess<'_> {
                ::lpc_model::SlotDataAccess::Record(self)
            }
        }

        #static_impls
    })
}

fn named_fields(data: Data) -> Result<syn::FieldsNamed> {
    match data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => Ok(fields),
            other => Err(syn::Error::new_spanned(
                other,
                "SlotRecord derive requires named struct fields",
            )),
        },
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "SlotRecord derive only supports structs",
        )),
    }
}

fn validate_slot_name(name: &str, span: proc_macro2::Span) -> Result<()> {
    if name.is_empty() {
        return Err(syn::Error::new(span, "slot field name cannot be empty"));
    }

    let mut chars = name.chars();
    let first = chars.next().expect("checked non-empty");
    if !is_ident_start(first) {
        return Err(syn::Error::new(
            span,
            "slot field name must start with '_' or an ASCII letter",
        ));
    }
    for c in chars {
        if !is_ident_continue(c) {
            return Err(syn::Error::new(
                span,
                "slot field name must contain only ASCII letters, digits, or '_'",
            ));
        }
    }

    Ok(())
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}
