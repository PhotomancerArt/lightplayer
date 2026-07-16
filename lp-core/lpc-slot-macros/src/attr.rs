use proc_macro2::TokenStream;
use syn::{
    Attribute, Expr, LitStr, Result, Token, parenthesized, parse::Parse, parse::ParseStream,
};

pub(crate) struct ContainerAttrs {
    pub(crate) shape_id: Option<LitStr>,
    pub(crate) default_policy: Option<SlotPolicyAttr>,
    pub(crate) enum_encoding: Option<EnumEncodingAttr>,
    pub(crate) rename_all: Option<RenameAllAttr>,
}

pub(crate) struct FieldAttrs {
    pub(crate) name: Option<LitStr>,
    pub(crate) shape: FieldShapeAttr,
    pub(crate) direction: FieldDirectionAttr,
    pub(crate) merge: FieldMergeAttr,
    pub(crate) policy: Option<SlotPolicyAttr>,
    pub(crate) default_bind: Option<LitStr>,
}

pub(crate) struct VariantAttrs {
    pub(crate) name: Option<LitStr>,
    pub(crate) is_default: bool,
}

pub(crate) enum FieldShapeAttr {
    Infer,
    Value(Expr),
    Leaf(Expr),
    Record,
    Map { key: LitStr, value_ref: LitStr },
    OptionRef(LitStr),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldDirectionAttr {
    Local,
    Consumed,
    Produced,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldMergeAttr {
    Latest,
    Error,
    ByKey,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotPolicyAttr {
    ReadOnlyPersisted,
    WritablePersisted,
    ReadOnlyTransient,
    WritableTransient,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnumEncodingAttr {
    Tagged,
    External,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenameAllAttr {
    SnakeCase,
}

pub(crate) fn parse_container(attrs: &[Attribute]) -> Result<ContainerAttrs> {
    let mut parsed = ContainerAttrs {
        shape_id: None,
        default_policy: None,
        enum_encoding: None,
        rename_all: None,
    };
    for attr in slot_attrs(attrs) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("shape_id") {
                let value = meta.value()?;
                parsed.shape_id = Some(value.parse()?);
                Ok(())
            } else if meta.path.is_ident("default_policy") {
                let value = meta.value()?;
                let value: LitStr = value.parse()?;
                parsed.default_policy = Some(parse_policy(&value)?);
                Ok(())
            } else if meta.path.is_ident("enum_encoding") {
                let value = meta.value()?;
                let value: LitStr = value.parse()?;
                parsed.enum_encoding = Some(parse_enum_encoding(&value)?);
                Ok(())
            } else if meta.path.is_ident("rename_all") {
                let value = meta.value()?;
                let value: LitStr = value.parse()?;
                parsed.rename_all = Some(parse_rename_all(&value)?);
                Ok(())
            } else if meta.path.is_ident("root") {
                Ok(())
            } else if meta.path.is_ident("view") {
                Ok(())
            } else {
                Err(meta.error("unsupported slot container attribute"))
            }
        })?;
    }
    Ok(parsed)
}

pub(crate) fn parse_field(attrs: &[Attribute]) -> Result<FieldAttrs> {
    let mut name = None;
    let mut shape = None;
    let mut direction = FieldDirectionAttr::Local;
    let mut merge = FieldMergeAttr::Latest;
    let mut policy = None;
    let mut default_bind: Option<LitStr> = None;
    for attr in slot_attrs(attrs) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("default_bind") {
                let value = meta.value()?;
                let value: LitStr = value.parse()?;
                // Only bus endpoints may be defaults (ADR 2026-07-09); the
                // full grammar is validated by the model's shape-walk test —
                // this macro cannot depend on lpc-model (cycle).
                if !value.value().starts_with("bus:") {
                    return Err(syn::Error::new(
                        value.span(),
                        "default_bind must be a `bus:<channel>` endpoint",
                    ));
                }
                default_bind = Some(value);
                Ok(())
            } else if meta.path.is_ident("name") {
                let value = meta.value()?;
                name = Some(value.parse()?);
                Ok(())
            } else if meta.path.is_ident("value") {
                let value = meta.value()?;
                shape = Some(FieldShapeAttr::Value(value.parse()?));
                Ok(())
            } else if meta.path.is_ident("leaf") {
                let value = meta.value()?;
                shape = Some(FieldShapeAttr::Leaf(value.parse()?));
                Ok(())
            } else if meta.path.is_ident("record") {
                shape = Some(FieldShapeAttr::Record);
                Ok(())
            } else if meta.path.is_ident("consumed") {
                if direction != FieldDirectionAttr::Local {
                    return Err(meta.error("slot field can only have one direction"));
                }
                direction = FieldDirectionAttr::Consumed;
                Ok(())
            } else if meta.path.is_ident("produced") {
                if direction != FieldDirectionAttr::Local {
                    return Err(meta.error("slot field can only have one direction"));
                }
                direction = FieldDirectionAttr::Produced;
                Ok(())
            } else if meta.path.is_ident("merge") {
                let value = meta.value()?;
                let value: LitStr = value.parse()?;
                merge = parse_merge(&value)?;
                Ok(())
            } else if meta.path.is_ident("policy") {
                let value = meta.value()?;
                let value: LitStr = value.parse()?;
                policy = Some(parse_policy(&value)?);
                Ok(())
            } else if meta.path.is_ident("option_ref") {
                let value = meta.value()?;
                shape = Some(FieldShapeAttr::OptionRef(value.parse()?));
                Ok(())
            } else if meta.path.is_ident("map") {
                let content;
                parenthesized!(content in meta.input);
                let map = content.parse::<MapArgs>()?;
                shape = Some(FieldShapeAttr::Map {
                    key: map.key,
                    value_ref: map.value_ref,
                });
                Ok(())
            } else {
                Err(meta.error("unsupported slot field attribute"))
            }
        })?;
    }
    Ok(FieldAttrs {
        name,
        shape: shape.unwrap_or(FieldShapeAttr::Infer),
        direction,
        merge,
        policy,
        default_bind,
    })
}

pub(crate) fn parse_variant(attrs: &[Attribute]) -> Result<VariantAttrs> {
    let mut name = None;
    let mut is_default = false;
    for attr in attrs {
        if attr.path().is_ident("default") {
            is_default = true;
            continue;
        }
        if !attr.path().is_ident("slot") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let value = meta.value()?;
                name = Some(value.parse()?);
                Ok(())
            } else {
                Err(meta.error("unsupported slot variant attribute"))
            }
        })?;
    }
    Ok(VariantAttrs { name, is_default })
}

pub(crate) fn field_shape_tokens(attr: &FieldShapeAttr, ty: &syn::Type) -> TokenStream {
    match attr {
        FieldShapeAttr::Infer => {
            quote::quote! { <#ty as ::lpc_model::FieldSlot>::slot_field_shape() }
        }
        FieldShapeAttr::Value(expr) => {
            quote::quote! { ::lpc_model::slot::shape::value(#expr) }
        }
        FieldShapeAttr::Leaf(expr) => {
            quote::quote! { ::lpc_model::slot::shape::leaf(#expr) }
        }
        FieldShapeAttr::Record => {
            quote::quote! { <#ty as ::lpc_model::SlotRecordShape>::slot_record_shape() }
        }
        FieldShapeAttr::Map { key, value_ref } => {
            let key_tokens = map_key_tokens(key);
            quote::quote! {
                ::lpc_model::slot::shape::map(
                    #key_tokens,
                    ::lpc_model::slot::shape::reference(::lpc_model::slot::shape::id(#value_ref)),
                )
            }
        }
        FieldShapeAttr::OptionRef(value_ref) => {
            quote::quote! {
                ::lpc_model::slot::shape::option(
                    ::lpc_model::slot::shape::reference(::lpc_model::slot::shape::id(#value_ref)),
                )
            }
        }
    }
}

pub(crate) fn field_static_shape_tokens(attr: &FieldShapeAttr, ty: &syn::Type) -> TokenStream {
    match attr {
        FieldShapeAttr::Infer => {
            quote::quote! {
                <#ty as ::lpc_model::FieldSlot>::STATIC_SLOT_FIELD_SHAPE_DESCRIPTOR
            }
        }
        FieldShapeAttr::Value(expr) => {
            let static_ty = static_lp_type_from_lp_type_expr(expr);
            quote::quote! {
                Some(&::lpc_model::StaticSlotShapeDescriptor::Value {
                    shape: ::lpc_model::StaticSlotValueShape::new(
                        ::lpc_model::SlotShapeId::new(0),
                        #static_ty,
                    ),
                })
            }
        }
        FieldShapeAttr::Leaf(_) => {
            quote::quote! {
                None
            }
        }
        FieldShapeAttr::Record => {
            quote::quote! {
                <#ty as ::lpc_model::SlotRecordShape>::STATIC_SLOT_RECORD_SHAPE_DESCRIPTOR
            }
        }
        FieldShapeAttr::Map { key, value_ref } => {
            let key_tokens = map_key_tokens(key);
            quote::quote! {
                Some(&::lpc_model::StaticSlotShapeDescriptor::Map {
                    meta: ::lpc_model::StaticSlotMeta::EMPTY,
                    key: #key_tokens,
                    value: &::lpc_model::StaticSlotShapeDescriptor::Ref {
                        id: ::lpc_model::SlotShapeId::from_static_name(#value_ref),
                    },
                })
            }
        }
        FieldShapeAttr::OptionRef(value_ref) => {
            quote::quote! {
                Some(&::lpc_model::StaticSlotShapeDescriptor::Option {
                    meta: ::lpc_model::StaticSlotMeta::EMPTY,
                    some: &::lpc_model::StaticSlotShapeDescriptor::Ref {
                        id: ::lpc_model::SlotShapeId::from_static_name(#value_ref),
                    },
                })
            }
        }
    }
}

pub(crate) fn field_access_tokens(
    attr: &FieldShapeAttr,
    ty: &syn::Type,
    field_ident: &syn::Ident,
) -> Option<TokenStream> {
    match attr {
        FieldShapeAttr::Infer => Some(
            quote::quote! { <#ty as ::lpc_model::FieldSlot>::slot_field_data(&self.#field_ident) },
        ),
        FieldShapeAttr::Value(_) | FieldShapeAttr::Leaf(_) => {
            Some(quote::quote! { ::lpc_model::SlotDataAccess::Value(&self.#field_ident) })
        }
        FieldShapeAttr::Record => {
            Some(quote::quote! { ::lpc_model::SlotDataAccess::Record(&self.#field_ident) })
        }
        FieldShapeAttr::Map { .. } => {
            Some(quote::quote! { ::lpc_model::SlotDataAccess::Map(&self.#field_ident) })
        }
        FieldShapeAttr::OptionRef(_) => {
            Some(quote::quote! { ::lpc_model::SlotDataAccess::Option(&self.#field_ident) })
        }
    }
}

pub(crate) fn field_mut_access_tokens(
    attr: &FieldShapeAttr,
    ty: &syn::Type,
    field_ident: &syn::Ident,
) -> Option<TokenStream> {
    match attr {
        FieldShapeAttr::Infer => Some(
            quote::quote! { <#ty as ::lpc_model::FieldSlotMut>::slot_field_data_mut(&mut self.#field_ident) },
        ),
        FieldShapeAttr::Value(_) | FieldShapeAttr::Leaf(_) => {
            Some(quote::quote! { ::lpc_model::SlotDataMutAccess::Value(&mut self.#field_ident) })
        }
        FieldShapeAttr::Record => {
            Some(quote::quote! { ::lpc_model::SlotDataMutAccess::Record(&mut self.#field_ident) })
        }
        FieldShapeAttr::Map { .. } => {
            Some(quote::quote! { ::lpc_model::SlotDataMutAccess::Map(&mut self.#field_ident) })
        }
        FieldShapeAttr::OptionRef(_) => {
            Some(quote::quote! { ::lpc_model::SlotDataMutAccess::Option(&mut self.#field_ident) })
        }
    }
}

pub(crate) fn field_binding_access_tokens(
    attr: &FieldShapeAttr,
    ty: &syn::Type,
    field_ident: &syn::Ident,
) -> Option<TokenStream> {
    match attr {
        FieldShapeAttr::Infer => {
            Some(quote::quote! { <#ty as ::lpc_model::FieldSlot>::slot_field_data(#field_ident) })
        }
        FieldShapeAttr::Value(_) | FieldShapeAttr::Leaf(_) => {
            Some(quote::quote! { ::lpc_model::SlotDataAccess::Value(#field_ident) })
        }
        FieldShapeAttr::Record => {
            Some(quote::quote! { ::lpc_model::SlotDataAccess::Record(#field_ident) })
        }
        FieldShapeAttr::Map { .. } => {
            Some(quote::quote! { ::lpc_model::SlotDataAccess::Map(#field_ident) })
        }
        FieldShapeAttr::OptionRef(_) => {
            Some(quote::quote! { ::lpc_model::SlotDataAccess::Option(#field_ident) })
        }
    }
}

pub(crate) fn field_binding_mut_access_tokens(
    attr: &FieldShapeAttr,
    ty: &syn::Type,
    field_ident: &syn::Ident,
) -> Option<TokenStream> {
    match attr {
        FieldShapeAttr::Infer => Some(
            quote::quote! { <#ty as ::lpc_model::FieldSlotMut>::slot_field_data_mut(#field_ident) },
        ),
        FieldShapeAttr::Value(_) | FieldShapeAttr::Leaf(_) => {
            Some(quote::quote! { ::lpc_model::SlotDataMutAccess::Value(#field_ident) })
        }
        FieldShapeAttr::Record => {
            Some(quote::quote! { ::lpc_model::SlotDataMutAccess::Record(#field_ident) })
        }
        FieldShapeAttr::Map { .. } => {
            Some(quote::quote! { ::lpc_model::SlotDataMutAccess::Map(#field_ident) })
        }
        FieldShapeAttr::OptionRef(_) => {
            Some(quote::quote! { ::lpc_model::SlotDataMutAccess::Option(#field_ident) })
        }
    }
}

pub(crate) fn field_semantics_tokens(
    direction: FieldDirectionAttr,
    merge: FieldMergeAttr,
) -> TokenStream {
    let direction_tokens = match direction {
        FieldDirectionAttr::Local => quote::quote! { ::lpc_model::SlotDirection::Local },
        FieldDirectionAttr::Consumed => quote::quote! { ::lpc_model::SlotDirection::Consumed },
        FieldDirectionAttr::Produced => quote::quote! { ::lpc_model::SlotDirection::Produced },
    };
    let merge_tokens = match merge {
        FieldMergeAttr::Latest => quote::quote! { ::lpc_model::SlotMerge::Latest },
        FieldMergeAttr::Error => quote::quote! { ::lpc_model::SlotMerge::Error },
        FieldMergeAttr::ByKey => quote::quote! { ::lpc_model::SlotMerge::ByKey },
    };
    quote::quote! {
        ::lpc_model::SlotSemantics::new(#direction_tokens, #merge_tokens)
    }
}

pub(crate) fn field_policy_tokens(policy: SlotPolicyAttr) -> TokenStream {
    match policy {
        SlotPolicyAttr::ReadOnlyPersisted => {
            quote::quote! { ::lpc_model::SlotPolicy::read_only_persisted() }
        }
        SlotPolicyAttr::WritablePersisted => {
            quote::quote! { ::lpc_model::SlotPolicy::writable_persisted() }
        }
        SlotPolicyAttr::ReadOnlyTransient => {
            quote::quote! { ::lpc_model::SlotPolicy::read_only_transient() }
        }
        SlotPolicyAttr::WritableTransient => {
            quote::quote! { ::lpc_model::SlotPolicy::writable_transient() }
        }
    }
}

/// Whether a field with this policy omits its dynamic mut-access arm.
///
/// Only read-only **transient** fields (produced state) drop mut access: they
/// are never authored, so nothing legitimate writes them dynamically. A
/// read-only **persisted** field is still authored JSON — the dynamic reader
/// must be able to deserialize it — and its write protection is mutate-time
/// policy enforcement (`resolve_slot_policy`), not a codec-level hole.
pub(crate) fn policy_is_read_only_transient(policy: SlotPolicyAttr) -> bool {
    matches!(policy, SlotPolicyAttr::ReadOnlyTransient)
}

fn slot_attrs(attrs: &[Attribute]) -> impl Iterator<Item = &Attribute> {
    attrs.iter().filter(|attr| attr.path().is_ident("slot"))
}

struct MapArgs {
    key: LitStr,
    value_ref: LitStr,
}

impl Parse for MapArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut key = None;
        let mut value_ref = None;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            if ident == "key" {
                key = Some(input.parse()?);
            } else if ident == "value_ref" {
                value_ref = Some(input.parse()?);
            } else {
                return Err(syn::Error::new_spanned(ident, "unsupported map argument"));
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            key: key.ok_or_else(|| input.error("missing map key"))?,
            value_ref: value_ref.ok_or_else(|| input.error("missing map value_ref"))?,
        })
    }
}

fn map_key_tokens(key: &LitStr) -> TokenStream {
    match key.value().as_str() {
        "string" => quote::quote! { ::lpc_model::SlotMapKeyShape::String },
        "i32" => quote::quote! { ::lpc_model::SlotMapKeyShape::I32 },
        "u32" => quote::quote! { ::lpc_model::SlotMapKeyShape::U32 },
        other => {
            let message = format!("unsupported slot map key shape: {other}");
            quote::quote! { compile_error!(#message) }
        }
    }
}

fn static_lp_type_from_lp_type_expr(expr: &Expr) -> TokenStream {
    let Expr::Path(path) = expr else {
        return quote::quote! {
            compile_error!("static descriptor generation supports only simple LpType path expressions")
        };
    };
    let Some(segment) = path.path.segments.last() else {
        return quote::quote! {
            compile_error!("static descriptor generation supports only simple LpType path expressions")
        };
    };
    let ident = &segment.ident;
    quote::quote! { ::lpc_model::StaticLpType::#ident }
}

fn parse_merge(value: &LitStr) -> Result<FieldMergeAttr> {
    match value.value().as_str() {
        "latest" => Ok(FieldMergeAttr::Latest),
        "error" => Ok(FieldMergeAttr::Error),
        "by_key" => Ok(FieldMergeAttr::ByKey),
        _ => Err(syn::Error::new_spanned(
            value,
            "unsupported slot merge policy; expected \"latest\", \"error\", or \"by_key\"",
        )),
    }
}

fn parse_policy(value: &LitStr) -> Result<SlotPolicyAttr> {
    match value.value().as_str() {
        "read_only_persisted" => Ok(SlotPolicyAttr::ReadOnlyPersisted),
        "writable_persisted" => Ok(SlotPolicyAttr::WritablePersisted),
        "read_only_transient" => Ok(SlotPolicyAttr::ReadOnlyTransient),
        "writable_transient" => Ok(SlotPolicyAttr::WritableTransient),
        _ => Err(syn::Error::new_spanned(
            value,
            "unsupported slot policy; expected \"read_only_persisted\", \"writable_persisted\", \"read_only_transient\", or \"writable_transient\"",
        )),
    }
}

fn parse_enum_encoding(value: &LitStr) -> Result<EnumEncodingAttr> {
    match value.value().as_str() {
        "tagged" => Ok(EnumEncodingAttr::Tagged),
        "external" => Ok(EnumEncodingAttr::External),
        _ => Err(syn::Error::new_spanned(
            value,
            "unsupported slot enum encoding; expected \"tagged\" or \"external\"",
        )),
    }
}

fn parse_rename_all(value: &LitStr) -> Result<RenameAllAttr> {
    match value.value().as_str() {
        "snake_case" => Ok(RenameAllAttr::SnakeCase),
        _ => Err(syn::Error::new_spanned(
            value,
            "unsupported slot rename_all policy; expected \"snake_case\"",
        )),
    }
}
