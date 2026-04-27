//! GLSL `texelFetch` → Naga [`naga::Expression::ImageLoad`] contract (M3a).
//!
//! Data-path lowering (descriptor loads, storage reads) is M3b; this module only
//! resolves operands, validates LOD, and returns an intentional placeholder error
//! for valid fetches.

use alloc::format;
use alloc::string::String;

use lps_shared::LpsType;
use naga::{AddressSpace, Expression, Function, GlobalVariable, Handle, Literal, Module};

use crate::lower_ctx::{LowerCtx, VRegVec};
use crate::lower_error::LowerError;

/// Lower Naga `ImageLoad` emitted from GLSL `texelFetch` (sampled image, explicit LOD).
pub(crate) fn lower_image_load_texel_fetch(
    ctx: &mut LowerCtx<'_>,
    image_expr: Handle<Expression>,
    level_expr: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let (gv, name) = resolve_direct_texture2d_uniform(ctx, image_expr)?;
    if !matches!(
        ctx.module.global_variables[gv].space,
        AddressSpace::Uniform | AddressSpace::Handle
    ) {
        return Err(LowerError::UnsupportedExpression(format!(
            "texelFetch: `{name}` must be uniform or handle-backed"
        )));
    }
    validate_texture_spec_presence(ctx, &name)?;

    classify_lod(ctx, level_expr, &name)?;

    Err(LowerError::UnsupportedExpression(format!(
        "texelFetch for texture uniform `{name}` recognized; data path is implemented in M3b"
    )))
}

fn validate_texture_spec_presence(ctx: &LowerCtx<'_>, name: &str) -> Result<(), LowerError> {
    if ctx.texture_specs.contains_key(name) {
        return Ok(());
    }
    Err(LowerError::UnsupportedExpression(format!(
        "texelFetch: no texture binding spec for sampler uniform `{name}`"
    )))
}

fn resolve_direct_texture2d_uniform(
    ctx: &LowerCtx<'_>,
    image_expr: Handle<Expression>,
) -> Result<(Handle<GlobalVariable>, String), LowerError> {
    let root = peel_load_chain(ctx.func, image_expr);
    let Expression::GlobalVariable(gv) = &ctx.func.expressions[root] else {
        return Err(LowerError::UnsupportedExpression(String::from(
            "texelFetch: texture operand must be a uniform sampler (direct sampler2D global)",
        )));
    };
    let Some(info) = ctx.global_map.get(gv) else {
        return Err(LowerError::Internal(format!(
            "texelFetch: GlobalVariable {gv:?} not in global_map"
        )));
    };
    if !matches!(info.ty, LpsType::Texture2D) {
        return Err(LowerError::UnsupportedExpression(String::from(
            "texelFetch: operand is not a Texture2D uniform",
        )));
    }
    let gv_rec = &ctx.module.global_variables[*gv];
    let Some(name) = gv_rec.name.as_ref().filter(|n| !n.is_empty()) else {
        return Err(LowerError::UnsupportedExpression(String::from(
            "texelFetch: sampler uniform has no name",
        )));
    };
    Ok((*gv, String::from(name.as_str())))
}

fn classify_lod(
    ctx: &LowerCtx<'_>,
    level_expr: Handle<Expression>,
    sampler_name: &str,
) -> Result<(), LowerError> {
    match lod_signed_integer_literal(ctx.module, ctx.func, level_expr) {
        LodLiteral::Zero => Ok(()),
        LodLiteral::Nonzero(v) => Err(LowerError::UnsupportedExpression(format!(
            "texelFetch `{sampler_name}`: lod must be literal 0, got nonzero lod {v}"
        ))),
        LodLiteral::Dynamic => Err(LowerError::UnsupportedExpression(String::from(
            "texelFetch: dynamic lod is not supported",
        ))),
    }
}

enum LodLiteral {
    Zero,
    Nonzero(i64),
    Dynamic,
}

fn lod_signed_integer_literal(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
) -> LodLiteral {
    match integer_literal_from_expression(module, func, expr) {
        Some(0) => LodLiteral::Zero,
        Some(v) => LodLiteral::Nonzero(v),
        None => LodLiteral::Dynamic,
    }
}

fn integer_literal_from_expression(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
) -> Option<i64> {
    match &func.expressions[expr] {
        Expression::Literal(lit) => int_from_literal(lit),
        Expression::Constant(c) => {
            let init = module.constants[*c].init;
            match &module.global_expressions[init] {
                Expression::Literal(lit) => int_from_literal(lit),
                _ => None,
            }
        }
        _ => None,
    }
}

fn int_from_literal(lit: &Literal) -> Option<i64> {
    match *lit {
        Literal::I32(v) => Some(v as i64),
        Literal::U32(v) => Some(v as i64),
        _ => None,
    }
}

fn peel_load_chain(func: &Function, mut h: Handle<Expression>) -> Handle<Expression> {
    while let Expression::Load { pointer } = &func.expressions[h] {
        h = *pointer;
    }
    h
}

#[cfg(test)]
mod texel_fetch_naga_shape_tests {
    use naga::Expression;

    use crate::NagaModule;
    use crate::compile;

    #[test]
    fn texel_fetch_glsl_maps_to_expression_image_load_with_level() {
        let glsl = r#"
uniform sampler2D inputColor;
vec4 render(vec2 pos) {
    return texelFetch(inputColor, ivec2(0, 0), 0);
}
"#;
        let NagaModule { module, functions } = compile(glsl).expect("compile");
        let h = functions
            .iter()
            .find(|(_, i)| i.name == "render")
            .expect("render")
            .0;
        let func = &module.functions[h];
        let mut found_image_load_texel_fetch = false;
        for (_, e) in func.expressions.iter() {
            if let Expression::ImageLoad {
                sample: None,
                level: Some(_),
                array_index,
                ..
            } = e
            {
                assert!(
                    array_index.is_none(),
                    "unexpected array/layer dimension on 2D texture texelFetch: {e:?}"
                );
                found_image_load_texel_fetch = true;
            }
        }
        assert!(
            found_image_load_texel_fetch,
            "expected non-array 2D texelFetch → ImageLoad {{ level: Some(..), .. }}"
        );
    }
}
