use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Result, Visibility, parse2};

use crate::attr;

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
    let shape_id = if let Some(shape_id) = container_attrs.shape_id {
        quote! { ::lpc_model::SlotShapeId::from_static_name(#shape_id) }
    } else {
        quote! {
            ::lpc_model::SlotShapeId::from_static_name(
                concat!(module_path!(), "::", stringify!(#ident)),
            )
        }
    };

    match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => crate::slotted_record::derive_record(ident, shape_id, fields),
            Fields::Unnamed(fields) => {
                crate::slotted_wrapper::derive_wrapper(ident, shape_id, fields)
            }
            Fields::Unit => Err(syn::Error::new_spanned(
                ident,
                "Slotted derive requires named fields or a single-field tuple wrapper",
            )),
        },
        Data::Enum(data) => crate::slotted_enum::derive_enum(ident, data),
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "Slotted derive only supports structs and enums",
        )),
    }
}

pub(crate) fn validate_slot_name(name: &str, span: proc_macro2::Span) -> Result<()> {
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

pub(crate) fn require_public(vis: &Visibility, ident: &syn::Ident) -> Result<()> {
    if matches!(vis, Visibility::Public(_)) {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(
            ident,
            "Slotted derive requires public fields; use a separate slot data struct or a custom impl for private runtime state",
        ))
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}
