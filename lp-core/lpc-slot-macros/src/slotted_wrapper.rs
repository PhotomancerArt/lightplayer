use proc_macro2::TokenStream;
use quote::quote;
use syn::Result;

pub(crate) fn derive_wrapper(
    ident: syn::Ident,
    shape_id: TokenStream,
    fields: syn::FieldsUnnamed,
) -> Result<TokenStream> {
    if fields.unnamed.len() != 1 {
        return Err(syn::Error::new_spanned(
            fields,
            "Slotted tuple wrappers must contain exactly one field",
        ));
    }

    let field_ty = &fields.unnamed[0].ty;

    Ok(quote! {
        impl ::lpc_model::SlotAccess for #ident {
            fn shape_id(&self) -> ::lpc_model::SlotShapeId {
                <Self as ::lpc_model::StaticSlotShape>::SHAPE_ID
            }

            fn data(&self) -> ::lpc_model::SlotDataAccess<'_> {
                <#field_ty as ::lpc_model::FieldSlot>::slot_field_data(&self.0)
            }

            fn as_any(&self) -> &dyn ::core::any::Any {
                self
            }

            fn into_any(
                self: ::lpc_model::__private::Box<Self>,
            ) -> ::lpc_model::__private::Box<dyn ::core::any::Any> {
                self
            }
        }

        impl ::lpc_model::StaticSlotShape for #ident {
            const SHAPE_ID: ::lpc_model::SlotShapeId = #shape_id;
            const STATIC_SLOT_SHAPE_DESCRIPTOR: Option<&'static ::lpc_model::StaticSlotShapeDescriptor> =
                <#field_ty as ::lpc_model::FieldSlot>::STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR;

            fn shape_name() -> Option<&'static str> {
                Some(concat!(module_path!(), "::", stringify!(#ident)))
            }

            fn slot_shape() -> ::lpc_model::SlotShape {
                <#field_ty as ::lpc_model::FieldSlot>::slot_field_shape()
            }
        }

        impl ::lpc_model::StaticSlotAccess for #ident {}

        impl ::lpc_model::SlotMapValueAccess for #ident {
            fn slot_data(&self) -> ::lpc_model::SlotDataAccess<'_> {
                <#field_ty as ::lpc_model::FieldSlot>::slot_field_data(&self.0)
            }
        }

        impl ::lpc_model::SlotMapValueMutAccess for #ident {
            fn slot_data_mut(&mut self) -> ::lpc_model::SlotDataMutAccess<'_> {
                <#field_ty as ::lpc_model::FieldSlotMut>::slot_field_data_mut(&mut self.0)
            }
        }

        impl ::lpc_model::FieldSlot for #ident {
            const STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR: Option<&'static ::lpc_model::StaticSlotShapeDescriptor> =
                <#field_ty as ::lpc_model::FieldSlot>::STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR;

            fn slot_field_shape() -> ::lpc_model::SlotShape {
                <#field_ty as ::lpc_model::FieldSlot>::slot_field_shape()
            }

            fn slot_field_data(&self) -> ::lpc_model::SlotDataAccess<'_> {
                <#field_ty as ::lpc_model::FieldSlot>::slot_field_data(&self.0)
            }
        }

        impl ::lpc_model::FieldSlotMut for #ident {
            fn slot_field_data_mut(&mut self) -> ::lpc_model::SlotDataMutAccess<'_> {
                <#field_ty as ::lpc_model::FieldSlotMut>::slot_field_data_mut(&mut self.0)
            }
        }

        impl ::lpc_model::SlotMutAccess for #ident {
            fn data_mut(&mut self) -> ::lpc_model::SlotDataMutAccess<'_> {
                <#field_ty as ::lpc_model::FieldSlotMut>::slot_field_data_mut(&mut self.0)
            }
        }
    })
}
