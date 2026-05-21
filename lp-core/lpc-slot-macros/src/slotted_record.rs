use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Result;

use crate::{
    attr,
    slotted::{require_public, validate_slot_name},
};

pub(crate) fn derive_record(
    ident: syn::Ident,
    shape_id: TokenStream,
    fields: syn::FieldsNamed,
    container_attrs: attr::ContainerAttrs,
) -> Result<TokenStream> {
    let mut shape_fields = Vec::new();
    let mut static_shape_options = Vec::new();
    let mut static_shape_fields = Vec::new();
    let mut static_shape_bindings = Vec::new();
    let mut access_arms = Vec::new();
    let mut mut_access_arms = Vec::new();
    let mut access_index = 0usize;

    for field in fields.named {
        let Some(field_ident) = field.ident else {
            continue;
        };
        require_public(&field.vis, &field_ident)?;
        let field_attr = attr::parse_field(&field.attrs)?;

        let field_name = field_attr
            .name
            .as_ref()
            .map_or_else(|| field_ident.to_string(), syn::LitStr::value);
        validate_slot_name(&field_name, field_ident.span())?;

        let field_ty = field.ty;

        let shape = attr::field_shape_tokens(&field_attr.shape, &field_ty);
        let static_shape = attr::field_static_shape_tokens(&field_attr.shape, &field_ty);
        let static_shape_binding = format_ident!("__field_shape_{}", static_shape_bindings.len());
        let semantics = attr::field_semantics_tokens(field_attr.direction, field_attr.merge);
        let selected_policy = field_attr.policy.or(container_attrs.default_policy);
        let policy = selected_policy
            .map(attr::field_policy_tokens)
            .unwrap_or_else(|| quote! { ::lpc_model::SlotPolicy::default() });
        let static_policy = selected_policy
            .map(attr::field_policy_tokens)
            .unwrap_or_else(|| quote! { ::lpc_model::SlotPolicy::writable_persisted() });
        shape_fields.push(quote! {
            ::lpc_model::slot::shape::field_with_semantics_and_policy(
                #field_name,
                #shape,
                #semantics,
                #policy,
            )
        });
        static_shape_options.push(static_shape);
        static_shape_fields.push(quote! {
            ::lpc_model::StaticSlotFieldShape {
                name: #field_name,
                shape: #static_shape_binding,
                semantics: #semantics,
                policy: #static_policy,
            }
        });
        static_shape_bindings.push(static_shape_binding);

        if let Some(access) = attr::field_access_tokens(&field_attr.shape, &field_ty, &field_ident)
        {
            let index = syn::Index::from(access_index);
            access_arms.push(quote! {
                #index => Some(#access),
            });
            if selected_policy.is_none_or(|policy| !attr::policy_is_read_only(policy))
                && let Some(mut_access) =
                    attr::field_mut_access_tokens(&field_attr.shape, &field_ty, &field_ident)
            {
                mut_access_arms.push(quote! {
                    #index => Some(#mut_access),
                });
            }
            access_index += 1;
        }
    }

    let static_impls = quote! {
        impl ::lpc_model::SlotAccess for #ident {
            fn shape_id(&self) -> ::lpc_model::SlotShapeId {
                <Self as ::lpc_model::StaticSlotShape>::SHAPE_ID
            }

            fn data(&self) -> ::lpc_model::SlotDataAccess<'_> {
                ::lpc_model::SlotDataAccess::Record(self)
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
            const SHAPE_ID: ::lpc_model::SlotShapeId =
                #shape_id;
            const STATIC_SLOT_SHAPE_DESCRIPTOR: Option<&'static ::lpc_model::StaticSlotShapeDescriptor> =
                <Self as ::lpc_model::SlotRecordShape>::STATIC_SLOT_RECORD_SHAPE_DESCRIPTOR;

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
            const STATIC_SLOT_RECORD_SHAPE_DESCRIPTOR: Option<&'static ::lpc_model::StaticSlotShapeDescriptor> =
                match (#(#static_shape_options,)*) {
                    (#(Some(#static_shape_bindings),)*) => {
                        Some(&::lpc_model::StaticSlotShapeDescriptor::Record {
                            meta: ::lpc_model::StaticSlotMeta::EMPTY,
                            fields: &[
                                #(#static_shape_fields),*
                            ],
                        })
                    }
                    _ => None,
                };

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

        impl ::lpc_model::SlotRecordMutAccess for #ident {
            fn field_mut(&mut self, index: usize) -> Option<::lpc_model::SlotDataMutAccess<'_>> {
                match index {
                    #(#mut_access_arms)*
                    _ => None,
                }
            }
        }

        impl ::lpc_model::SlotMapValueAccess for #ident {
            fn slot_data(&self) -> ::lpc_model::SlotDataAccess<'_> {
                ::lpc_model::SlotDataAccess::Record(self)
            }
        }

        impl ::lpc_model::SlotMapValueMutAccess for #ident {
            fn slot_data_mut(&mut self) -> ::lpc_model::SlotDataMutAccess<'_> {
                ::lpc_model::SlotDataMutAccess::Record(self)
            }
        }

        impl ::lpc_model::FieldSlot for #ident {
            const STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR: Option<&'static ::lpc_model::StaticSlotShapeDescriptor> =
                <Self as ::lpc_model::SlotRecordShape>::STATIC_SLOT_RECORD_SHAPE_DESCRIPTOR;

            fn slot_field_shape() -> ::lpc_model::SlotShape {
                <Self as ::lpc_model::SlotRecordShape>::slot_record_shape()
            }

            fn slot_field_data(&self) -> ::lpc_model::SlotDataAccess<'_> {
                ::lpc_model::SlotDataAccess::Record(self)
            }
        }

        impl ::lpc_model::FieldSlotMut for #ident {
            fn slot_field_data_mut(&mut self) -> ::lpc_model::SlotDataMutAccess<'_> {
                ::lpc_model::SlotDataMutAccess::Record(self)
            }
        }

        impl ::lpc_model::SlotMutAccess for #ident {
            fn data_mut(&mut self) -> ::lpc_model::SlotDataMutAccess<'_> {
                ::lpc_model::SlotDataMutAccess::Record(self)
            }
        }

        #static_impls
    })
}
