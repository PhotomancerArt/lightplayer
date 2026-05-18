use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Data, DeriveInput, Expr, Fields, Ident, LitFloat, Result, Token, Type, Visibility,
    parenthesized, parse::Parse, parse::ParseStream, parse2,
};

pub(crate) fn derive(input: TokenStream) -> TokenStream {
    match derive_inner(input) {
        Ok(tokens) => tokens,
        Err(err) => err.to_compile_error(),
    }
}

fn derive_inner(input: TokenStream) -> Result<TokenStream> {
    let input = parse2::<DeriveInput>(input)?;
    let ident = input.ident;
    let attrs = SlotValueAttrs::parse(&input.attrs)?;
    let editor = attrs.editor.unwrap_or(EditorSpec::Plain).tokens();
    let shape_id = ident.to_string();

    let (lp_type, to_lp_value, from_lp_value) = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let field = fields.unnamed.into_iter().next().expect("checked len");
                require_public(&field.vis, &ident)?;
                let ty = field.ty;
                (
                    lp_type_tokens(&ty)?,
                    quote! { <#ty as ::lpc_model::ToLpValue>::to_lp_value(&self.0) },
                    quote! {
                        Ok(Self(<#ty as ::lpc_model::FromLpValue>::from_lp_value(value)?))
                    },
                )
            }
            Fields::Named(fields) => {
                let mut model_fields = Vec::new();
                let mut to_fields = Vec::new();
                let mut from_fields = Vec::new();
                for (index, field) in fields.named.into_iter().enumerate() {
                    require_public(&field.vis, &ident)?;
                    let field_ident = field.ident.expect("named field");
                    let field_name = field_ident.to_string();
                    let ty = field.ty;
                    let field_ty = lp_type_tokens(&ty)?;
                    let index = syn::Index::from(index);
                    model_fields.push(quote! {
                        ::lpc_model::ModelStructMember {
                            name: ::lpc_model::__private::String::from(#field_name),
                            ty: #field_ty,
                        }
                    });
                    to_fields.push(quote! {
                        (
                            ::lpc_model::__private::String::from(#field_name),
                            <#ty as ::lpc_model::ToLpValue>::to_lp_value(&self.#field_ident),
                        )
                    });
                    from_fields.push(quote! {
                        #field_ident: match fields.get(#index) {
                            Some((name, value)) if name == #field_name => {
                                <#ty as ::lpc_model::FromLpValue>::from_lp_value(value)?
                            }
                            _ => {
                                return Err(::lpc_model::ValueRootError::new(
                                    concat!("expected ", stringify!(#ident), ".", #field_name),
                                ));
                            }
                        }
                    });
                }
                let field_count = from_fields.len();

                (
                    quote! {
                        ::lpc_model::LpType::Struct {
                            name: Some(::lpc_model::__private::String::from(stringify!(#ident))),
                            fields: ::lpc_model::__private::Vec::from([
                                #(#model_fields),*
                            ]),
                        }
                    },
                    quote! {
                        ::lpc_model::LpValue::Struct {
                            name: Some(::lpc_model::__private::String::from(stringify!(#ident))),
                            fields: ::lpc_model::__private::Vec::from([
                                #(#to_fields),*
                            ]),
                        }
                    },
                    quote! {
                        let ::lpc_model::LpValue::Struct { name, fields } = value else {
                            return Err(::lpc_model::ValueRootError::new(
                                concat!("expected ", stringify!(#ident), " struct"),
                            ));
                        };
                        if name.as_deref() != Some(stringify!(#ident)) || fields.len() != #field_count {
                            return Err(::lpc_model::ValueRootError::new(
                                concat!("expected ", stringify!(#ident), " struct"),
                            ));
                        }
                        Ok(Self {
                            #(#from_fields),*
                        })
                    },
                )
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "SlotValue derive supports public tuple newtypes and public named-field structs",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "SlotValue derive only supports structs",
            ));
        }
    };

    Ok(quote! {
        impl ::lpc_model::ToLpValue for #ident {
            fn to_lp_value(&self) -> ::lpc_model::LpValue {
                #to_lp_value
            }
        }

        impl ::lpc_model::FromLpValue for #ident {
            fn from_lp_value(value: &::lpc_model::LpValue) -> Result<Self, ::lpc_model::ValueRootError> {
                #from_lp_value
            }
        }

        impl ::lpc_model::SlotValue for #ident {
            const SHAPE_ID: ::lpc_model::SlotShapeId =
                ::lpc_model::SlotShapeId::from_static_name(#shape_id);

            fn value_shape() -> ::lpc_model::SlotValueShape {
                ::lpc_model::SlotValueShape {
                    id: Self::SHAPE_ID,
                    ty: #lp_type,
                    meta: ::lpc_model::SlotMeta::empty(),
                    editor: #editor,
                }
            }
        }
    })
}

fn require_public(vis: &Visibility, ident: &Ident) -> Result<()> {
    if matches!(vis, Visibility::Public(_)) {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(
            ident,
            "SlotValue derive requires public fields",
        ))
    }
}

fn lp_type_tokens(ty: &Type) -> Result<TokenStream> {
    if type_is_path(ty, "String") {
        return Ok(quote! { ::lpc_model::LpType::String });
    }
    if type_is_path(ty, "i32") {
        return Ok(quote! { ::lpc_model::LpType::I32 });
    }
    if type_is_path(ty, "u32") {
        return Ok(quote! { ::lpc_model::LpType::U32 });
    }
    if type_is_path(ty, "f32") {
        return Ok(quote! { ::lpc_model::LpType::F32 });
    }
    if type_is_path(ty, "bool") {
        return Ok(quote! { ::lpc_model::LpType::Bool });
    }
    if array_is_f32_len(ty, 2) {
        return Ok(quote! { ::lpc_model::LpType::Vec2 });
    }
    if array_is_f32_len(ty, 3) {
        return Ok(quote! { ::lpc_model::LpType::Vec3 });
    }
    Err(syn::Error::new_spanned(
        ty,
        "SlotValue derive cannot infer an LpType for this field yet",
    ))
}

fn type_is_path(ty: &Type, expected: &str) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == expected)
}

fn array_is_f32_len(ty: &Type, expected_len: usize) -> bool {
    let Type::Array(array) = ty else {
        return false;
    };
    if !type_is_path(&array.elem, "f32") {
        return false;
    }
    let Expr::Lit(expr) = &array.len else {
        return false;
    };
    let syn::Lit::Int(len) = &expr.lit else {
        return false;
    };
    len.base10_parse::<usize>()
        .is_ok_and(|len| len == expected_len)
}

struct SlotValueAttrs {
    editor: Option<EditorSpec>,
}

impl SlotValueAttrs {
    fn parse(attrs: &[syn::Attribute]) -> Result<Self> {
        let mut parsed = Self { editor: None };
        for attr in attrs
            .iter()
            .filter(|attr| attr.path().is_ident("slot_value"))
        {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("editor") {
                    let value = meta.value()?;
                    parsed.editor = Some(value.parse()?);
                    Ok(())
                } else {
                    Err(meta.error("unsupported slot_value attribute"))
                }
            })?;
        }
        Ok(parsed)
    }
}

enum EditorSpec {
    Plain,
    Path,
    NodeRef,
    Dimensions,
    Affine2d,
    Resource,
    RuntimeBufferResource,
    VisualProduct,
    ControlProduct,
    Xy,
    Slider {
        min: LitFloat,
        max: LitFloat,
        step: Option<LitFloat>,
    },
    Number {
        min: Option<LitFloat>,
        max: Option<LitFloat>,
        step: Option<LitFloat>,
    },
}

impl EditorSpec {
    fn tokens(&self) -> TokenStream {
        match self {
            Self::Plain => quote! { ::lpc_model::ValueEditorHint::Plain },
            Self::Path => quote! { ::lpc_model::ValueEditorHint::Path },
            Self::NodeRef => quote! { ::lpc_model::ValueEditorHint::NodeRef },
            Self::Dimensions => quote! { ::lpc_model::ValueEditorHint::Dimensions },
            Self::Affine2d => quote! { ::lpc_model::ValueEditorHint::Affine2d },
            Self::Resource => quote! { ::lpc_model::ValueEditorHint::Resource },
            Self::RuntimeBufferResource => {
                quote! { ::lpc_model::ValueEditorHint::RuntimeBufferResource }
            }
            Self::VisualProduct => quote! { ::lpc_model::ValueEditorHint::VisualProduct },
            Self::ControlProduct => quote! { ::lpc_model::ValueEditorHint::ControlProduct },
            Self::Xy => quote! { ::lpc_model::ValueEditorHint::Xy },
            Self::Slider { min, max, step } => {
                let step = option_f32_tokens(step);
                quote! {
                    ::lpc_model::ValueEditorHint::Slider {
                        min: ::lpc_model::OrderedF32(#min),
                        max: ::lpc_model::OrderedF32(#max),
                        step: #step,
                    }
                }
            }
            Self::Number { min, max, step } => {
                let min = option_f32_tokens(min);
                let max = option_f32_tokens(max);
                let step = option_f32_tokens(step);
                quote! {
                    ::lpc_model::ValueEditorHint::Number {
                        min: #min,
                        max: #max,
                        step: #step,
                    }
                }
            }
        }
    }
}

fn option_f32_tokens(value: &Option<LitFloat>) -> TokenStream {
    match value {
        Some(value) => quote! { Some(::lpc_model::OrderedF32(#value)) },
        None => quote! { None },
    }
}

impl Parse for EditorSpec {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let ident: Ident = input.parse()?;
        match ident.to_string().as_str() {
            "plain" => Ok(Self::Plain),
            "path" => Ok(Self::Path),
            "node_ref" => Ok(Self::NodeRef),
            "dimensions" => Ok(Self::Dimensions),
            "affine2d" => Ok(Self::Affine2d),
            "resource" => Ok(Self::Resource),
            "runtime_buffer_resource" => Ok(Self::RuntimeBufferResource),
            "visual_product" => Ok(Self::VisualProduct),
            "control_product" => Ok(Self::ControlProduct),
            "xy" => Ok(Self::Xy),
            "slider" => {
                let content;
                parenthesized!(content in input);
                let args = NumberArgs::parse(&content)?;
                Ok(Self::Slider {
                    min: args
                        .min
                        .ok_or_else(|| content.error("slider editor requires min"))?,
                    max: args
                        .max
                        .ok_or_else(|| content.error("slider editor requires max"))?,
                    step: args.step,
                })
            }
            "number" => {
                let content;
                parenthesized!(content in input);
                let args = NumberArgs::parse(&content)?;
                Ok(Self::Number {
                    min: args.min,
                    max: args.max,
                    step: args.step,
                })
            }
            _ => Err(syn::Error::new_spanned(ident, "unsupported editor hint")),
        }
    }
}

struct NumberArgs {
    min: Option<LitFloat>,
    max: Option<LitFloat>,
    step: Option<LitFloat>,
}

impl NumberArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut args = Self {
            min: None,
            max: None,
            step: None,
        };
        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let value: LitFloat = input.parse()?;
            match ident.to_string().as_str() {
                "min" => args.min = Some(value),
                "max" => args.max = Some(value),
                "step" => args.step = Some(value),
                _ => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "unsupported editor argument",
                    ));
                }
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(args)
    }
}
