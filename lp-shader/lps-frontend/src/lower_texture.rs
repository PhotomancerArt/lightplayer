//! GLSL `texelFetch` → Naga [`naga::Expression::ImageLoad`] and
//! GLSL `texture(sampler2D, vec2)` → [`naga::Expression::ImageSample`] (filtered sampling builtins).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{IrType, LpirOp, TexelFetchBoundsMode, VMCTX_VREG, VReg};
use lps_shared::{LpsType, TextureShapeHint, TextureStorageFormat};
use naga::{
    AddressSpace, Expression, Function, GlobalVariable, Handle, Literal, Module, SampleLevel,
    ScalarKind, SwizzleComponent, TypeInner, VectorSize,
};

use crate::lower_ctx::{LowerCtx, VRegVec};
use crate::lower_error::LowerError;
use crate::naga_util::expr_type_inner;

/// Byte offset of `ptr` within a [`LpsType::Texture2D`] uniform (std430).
const TEXTURE_DESC_PTR_OFFSET: u32 = 0;
/// Byte offset of `width` (i32 extent).
const TEXTURE_DESC_WIDTH_OFFSET: u32 = 4;
/// Byte offset of `height` (i32 extent).
const TEXTURE_DESC_HEIGHT_OFFSET: u32 = 8;
/// Byte offset of `row_stride` (bytes between rows).
const TEXTURE_DESC_ROW_STRIDE_OFFSET: u32 = 12;

struct TextureDescriptorVRegs {
    ptr: VReg,
    width: VReg,
    height: VReg,
    row_stride: VReg,
}

struct TexelFetchCoords {
    x: VReg,
    y: VReg,
}

/// Lower Naga `ImageLoad` emitted from GLSL `texelFetch` (sampled image, explicit LOD).
pub(crate) fn lower_image_load_texel_fetch(
    ctx: &mut LowerCtx<'_>,
    image_expr: Handle<Expression>,
    coordinate_expr: Handle<Expression>,
    level_expr: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let (gv, name) = resolve_direct_texture2d_uniform(ctx, image_expr, "texelFetch")?;
    if !matches!(
        ctx.module.global_variables[gv].space,
        AddressSpace::Uniform | AddressSpace::Handle
    ) {
        return Err(LowerError::UnsupportedExpression(format!(
            "texelFetch: `{name}` must be uniform or handle-backed"
        )));
    }
    validate_texture_binding_spec_exists(ctx, &name, "texelFetch")?;

    classify_lod(ctx, level_expr, &name)?;

    let spec = ctx.texture_specs.get(name.as_str()).ok_or_else(|| {
        LowerError::Internal(String::from(
            "texelFetch: texture spec missing after validation",
        ))
    })?;
    let bpp = i32::try_from(spec.format.bytes_per_pixel()).map_err(|_| {
        LowerError::Internal(String::from("texelFetch: bytes_per_pixel does not fit i32"))
    })?;

    let desc = load_texture_descriptor_vregs(ctx, gv)?;
    let coords = lower_texel_fetch_coords(ctx, coordinate_expr)?;
    let coords = emit_clamp_texel_coords(
        &mut ctx.fb,
        coords,
        desc.width,
        desc.height,
        ctx.texel_fetch_bounds,
    )?;
    let texel_addr = emit_texel_byte_address(ctx, &desc, coords, bpp)?;
    emit_texel_fetch_vec4_unorm(ctx, texel_addr, spec.format)
}

fn validate_texture_binding_spec_exists(
    ctx: &LowerCtx<'_>,
    name: &str,
    op: &str,
) -> Result<(), LowerError> {
    if ctx.texture_specs.contains_key(name) {
        return Ok(());
    }
    Err(LowerError::UnsupportedExpression(format!(
        "{op} `{name}`: no texture binding spec for sampler uniform `{name}`"
    )))
}

/// GLSL `texture(sampler2D, vec2)` with implicit / base LOD only → texture sampler builtin.
pub(crate) fn lower_image_sample_texture(
    ctx: &mut LowerCtx<'_>,
    image: Handle<Expression>,
    _sampler: Handle<Expression>,
    gather: Option<SwizzleComponent>,
    coordinate: Handle<Expression>,
    array_index: Option<Handle<Expression>>,
    offset: Option<Handle<Expression>>,
    level: SampleLevel,
    depth_ref: Option<Handle<Expression>>,
    clamp_to_edge: bool,
) -> Result<VRegVec, LowerError> {
    if gather.is_some() {
        return Err(LowerError::UnsupportedExpression(String::from(
            "texture: gather is not supported",
        )));
    }
    if offset.is_some() {
        return Err(LowerError::UnsupportedExpression(String::from(
            "texture: offset qualifier is not supported",
        )));
    }
    if depth_ref.is_some() {
        return Err(LowerError::UnsupportedExpression(String::from(
            "texture: depth-compare sampling is not supported",
        )));
    }
    if clamp_to_edge {
        return Err(LowerError::UnsupportedExpression(String::from(
            "texture: clamp-to-edge coordinate mode is not supported",
        )));
    }

    let (gv, name) = resolve_direct_texture2d_uniform(ctx, image, "texture")?;
    if !matches!(
        ctx.module.global_variables[gv].space,
        AddressSpace::Uniform | AddressSpace::Handle
    ) {
        return Err(LowerError::UnsupportedExpression(format!(
            "texture `{name}`: sampler uniform must be uniform or handle-backed"
        )));
    }
    validate_texture_binding_spec_exists(ctx, &name, "texture")?;

    if array_index.is_some() {
        return Err(LowerError::UnsupportedExpression(format!(
            "texture `{name}`: arrayed / layered sampling is not supported"
        )));
    }
    validate_image_sample_level(ctx, level, &name)?;

    let spec = ctx.texture_specs.get(name.as_str()).ok_or_else(|| {
        LowerError::Internal(String::from(
            "texture: texture spec missing after validation",
        ))
    })?;

    let import_name = match (spec.format, spec.shape_hint) {
        (TextureStorageFormat::Rgba16Unorm, TextureShapeHint::General2D) => {
            "texture2d_rgba16_unorm"
        }
        (TextureStorageFormat::Rgba16Unorm, TextureShapeHint::HeightOne) => {
            "texture1d_rgba16_unorm"
        }
        (TextureStorageFormat::R16Unorm, TextureShapeHint::General2D) => "texture2d_r16_unorm",
        (TextureStorageFormat::R16Unorm, TextureShapeHint::HeightOne) => "texture1d_r16_unorm",
        (TextureStorageFormat::Rgb16Unorm, _) => {
            return Err(LowerError::UnsupportedExpression(format!(
                "texture `{name}`: unsupported format Rgb16Unorm for filtered sampling"
            )));
        }
    };
    let key = format!("texture::{import_name}");
    let callee = *ctx.import_map.get(&key).ok_or_else(|| {
        LowerError::UnsupportedExpression(format!(
            "texture `{name}`: no builtin import `{key}` registered (format {:?}, shape {:?})",
            spec.format, spec.shape_hint
        ))
    })?;

    let desc = load_texture_descriptor_vregs(ctx, gv)?;
    let uv = lower_texture_vec2_coords(ctx, coordinate, &name)?;

    let u_q32 = spill_f32_q32_lane_as_i32_vreg(ctx, uv.x)?;

    let filter_abi = iconst(ctx, spec.filter.to_builtin_abi() as i32)?;
    let wrap_x_abi = iconst(ctx, spec.wrap_x.to_builtin_abi() as i32)?;

    let out_slot = ctx.fb.alloc_slot(16);
    let out_addr = ctx.fb.alloc_vreg(IrType::Pointer);
    ctx.fb.push(LpirOp::SlotAddr {
        dst: out_addr,
        slot: out_slot,
    });

    let mut args: Vec<VReg> = Vec::new();
    args.push(out_addr);
    args.push(desc.ptr);
    args.push(desc.width);
    match spec.shape_hint {
        TextureShapeHint::General2D => {
            let v_q32 = spill_f32_q32_lane_as_i32_vreg(ctx, uv.y)?;
            let wrap_y_abi = iconst(ctx, spec.wrap_y.to_builtin_abi() as i32)?;
            args.push(desc.height);
            args.push(desc.row_stride);
            args.push(u_q32);
            args.push(v_q32);
            args.push(filter_abi);
            args.push(wrap_x_abi);
            args.push(wrap_y_abi);
        }
        TextureShapeHint::HeightOne => {
            // `wrap_y` and the normalized v coordinate are intentionally omitted on the height-one path.
            args.push(desc.row_stride);
            args.push(u_q32);
            args.push(filter_abi);
            args.push(wrap_x_abi);
        }
    }

    ctx.fb.push_call(callee, &args, &[]);

    let mut out = VRegVec::new();
    for i in 0..4u32 {
        let raw = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Load {
            dst: raw,
            base: out_addr,
            offset: i.wrapping_mul(4),
        });
        let lane = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::FfromI32Bits {
            dst: lane,
            src: raw,
        });
        out.push(lane);
    }
    Ok(out)
}

fn validate_image_sample_level(
    ctx: &LowerCtx<'_>,
    level: SampleLevel,
    sampler_name: &str,
) -> Result<(), LowerError> {
    match level {
        SampleLevel::Auto | SampleLevel::Zero => Ok(()),
        SampleLevel::Exact(expr) => {
            match integer_literal_from_expression(ctx.module, ctx.func, expr) {
                Some(0) => Ok(()),
                Some(v) => Err(LowerError::UnsupportedExpression(format!(
                    "texture `{sampler_name}`: explicit LOD/gradient sampling is not supported (nonzero LOD {v})"
                ))),
                None => Err(LowerError::UnsupportedExpression(format!(
                    "texture `{sampler_name}`: explicit LOD/gradient sampling is not supported"
                ))),
            }
        }
        SampleLevel::Bias(_) => Err(LowerError::UnsupportedExpression(format!(
            "texture `{sampler_name}`: explicit LOD/gradient sampling is not supported (LOD bias)"
        ))),
        SampleLevel::Gradient { .. } => Err(LowerError::UnsupportedExpression(format!(
            "texture `{sampler_name}`: explicit LOD/gradient sampling is not supported (textureGrad)"
        ))),
    }
}

struct TexVec2 {
    x: VReg,
    y: VReg,
}

fn lower_texture_vec2_coords(
    ctx: &mut LowerCtx<'_>,
    coordinate_expr: Handle<Expression>,
    sampler_name: &str,
) -> Result<TexVec2, LowerError> {
    let inner = expr_type_inner(ctx.module, ctx.func, coordinate_expr)?;
    let is_vec2_f = match inner {
        TypeInner::Vector { size, scalar } => {
            size == VectorSize::Bi && scalar.kind == ScalarKind::Float
        }
        _ => false,
    };
    if !is_vec2_f {
        return Err(LowerError::UnsupportedExpression(format!(
            "texture `{sampler_name}`: coordinate must be vec2"
        )));
    }
    let lanes = ctx.ensure_expr_vec(coordinate_expr)?;
    if lanes.len() != 2 {
        return Err(LowerError::UnsupportedExpression(format!(
            "texture `{sampler_name}`: coordinate must be vec2"
        )));
    }
    Ok(TexVec2 {
        x: lanes[0],
        y: lanes[1],
    })
}

/// Q32 / F32 lane as raw `i32` bits for sampler builtins (spill through stack slot).
fn spill_f32_q32_lane_as_i32_vreg(ctx: &mut LowerCtx<'_>, f: VReg) -> Result<VReg, LowerError> {
    let slot = ctx.fb.alloc_slot(4);
    let addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
    ctx.fb.push(LpirOp::Store {
        base: addr,
        offset: 0,
        value: f,
    });
    let raw = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Load {
        dst: raw,
        base: addr,
        offset: 0,
    });
    Ok(raw)
}

fn iconst(ctx: &mut LowerCtx<'_>, value: i32) -> Result<VReg, LowerError> {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 { dst, value });
    Ok(dst)
}

fn load_texture_descriptor_vregs(
    ctx: &mut LowerCtx<'_>,
    gv: Handle<GlobalVariable>,
) -> Result<TextureDescriptorVRegs, LowerError> {
    let info = ctx.global_map.get(&gv).ok_or_else(|| {
        LowerError::Internal(format!(
            "texelFetch: GlobalVariable {gv:?} not in global_map (descriptor load)"
        ))
    })?;
    let base = info.byte_offset;
    let mut load_lane = |offset: u32| -> Result<VReg, LowerError> {
        let dst = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Load {
            dst,
            base: VMCTX_VREG,
            offset: base.wrapping_add(offset),
        });
        Ok(dst)
    };
    Ok(TextureDescriptorVRegs {
        ptr: load_lane(TEXTURE_DESC_PTR_OFFSET)?,
        width: load_lane(TEXTURE_DESC_WIDTH_OFFSET)?,
        height: load_lane(TEXTURE_DESC_HEIGHT_OFFSET)?,
        row_stride: load_lane(TEXTURE_DESC_ROW_STRIDE_OFFSET)?,
    })
}

fn emit_clamp_texel_coords(
    fb: &mut lpir::FunctionBuilder,
    coords: TexelFetchCoords,
    width: VReg,
    height: VReg,
    mode: TexelFetchBoundsMode,
) -> Result<TexelFetchCoords, LowerError> {
    match mode {
        TexelFetchBoundsMode::Unchecked => Ok(coords),
        TexelFetchBoundsMode::ClampToEdge => {
            let x = clamp_signed_coord_to_extent(fb, coords.x, width)?;
            let y = clamp_signed_coord_to_extent(fb, coords.y, height)?;
            Ok(TexelFetchCoords { x, y })
        }
    }
}

/// Clamp `v` to `[0, extent - 1]` using signed compares and `Select`.
fn clamp_signed_coord_to_extent(
    fb: &mut lpir::FunctionBuilder,
    v: VReg,
    extent: VReg,
) -> Result<VReg, LowerError> {
    let zero = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 {
        dst: zero,
        value: 0,
    });

    let lt0 = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IltS {
        dst: lt0,
        lhs: v,
        rhs: zero,
    });
    let after_low = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::Select {
        dst: after_low,
        cond: lt0,
        if_true: zero,
        if_false: v,
    });

    let max_v = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IsubImm {
        dst: max_v,
        src: extent,
        imm: 1,
    });
    let gt_max = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IgtS {
        dst: gt_max,
        lhs: after_low,
        rhs: max_v,
    });
    let out = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::Select {
        dst: out,
        cond: gt_max,
        if_true: max_v,
        if_false: after_low,
    });
    Ok(out)
}

fn emit_texel_byte_address(
    ctx: &mut LowerCtx<'_>,
    desc: &TextureDescriptorVRegs,
    coords: TexelFetchCoords,
    bytes_per_pixel: i32,
) -> Result<VReg, LowerError> {
    let row_off = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Imul {
        dst: row_off,
        lhs: coords.y,
        rhs: desc.row_stride,
    });
    let col_off = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::ImulImm {
        dst: col_off,
        src: coords.x,
        imm: bytes_per_pixel,
    });
    let texel_off = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iadd {
        dst: texel_off,
        lhs: row_off,
        rhs: col_off,
    });
    let addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iadd {
        dst: addr,
        lhs: desc.ptr,
        rhs: texel_off,
    });
    Ok(addr)
}

fn resolve_direct_texture2d_uniform(
    ctx: &LowerCtx<'_>,
    image_expr: Handle<Expression>,
    fn_name: &str,
) -> Result<(Handle<GlobalVariable>, String), LowerError> {
    let root = peel_load_chain(ctx.func, image_expr);
    let Expression::GlobalVariable(gv) = &ctx.func.expressions[root] else {
        return Err(LowerError::UnsupportedExpression(format!(
            "{fn_name}: sampled image must refer to a direct sampler2D uniform (not a function parameter or indirect value)"
        )));
    };
    let Some(info) = ctx.global_map.get(gv) else {
        return Err(LowerError::Internal(format!(
            "{fn_name}: GlobalVariable {gv:?} not in global_map"
        )));
    };
    if !matches!(info.ty, LpsType::Texture2D) {
        return Err(LowerError::UnsupportedExpression(format!(
            "{fn_name}: operand is not a Texture2D uniform"
        )));
    }
    let gv_rec = &ctx.module.global_variables[*gv];
    let Some(name) = gv_rec.name.as_ref().filter(|n| !n.is_empty()) else {
        return Err(LowerError::UnsupportedExpression(format!(
            "{fn_name}: sampler uniform has no name"
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

fn lower_texel_fetch_coords(
    ctx: &mut LowerCtx<'_>,
    coordinate_expr: Handle<Expression>,
) -> Result<TexelFetchCoords, LowerError> {
    let inner = expr_type_inner(ctx.module, ctx.func, coordinate_expr)?;
    let is_ivec2 = match inner {
        TypeInner::Vector { size, scalar } => {
            size == VectorSize::Bi && scalar.kind == ScalarKind::Sint
        }
        _ => false,
    };
    if !is_ivec2 {
        return Err(LowerError::UnsupportedExpression(String::from(
            "texelFetch: coordinate must be ivec2",
        )));
    }

    let lanes = ctx.ensure_expr_vec(coordinate_expr)?;
    if lanes.len() != 2 {
        return Err(LowerError::UnsupportedExpression(String::from(
            "texelFetch: coordinate must be ivec2",
        )));
    }
    Ok(TexelFetchCoords {
        x: lanes[0],
        y: lanes[1],
    })
}

fn emit_texel_fetch_vec4_unorm(
    ctx: &mut LowerCtx<'_>,
    texel_addr: VReg,
    format: TextureStorageFormat,
) -> Result<VRegVec, LowerError> {
    let mut out = VRegVec::new();
    match format {
        TextureStorageFormat::R16Unorm => {
            out.push(emit_unorm16_channel_load(ctx, texel_addr, 0)?);
            out.push(f32_const(ctx, 0.0)?);
            out.push(f32_const(ctx, 0.0)?);
            out.push(f32_const(ctx, 1.0)?);
        }
        TextureStorageFormat::Rgb16Unorm => {
            out.push(emit_unorm16_channel_load(ctx, texel_addr, 0)?);
            out.push(emit_unorm16_channel_load(ctx, texel_addr, 1)?);
            out.push(emit_unorm16_channel_load(ctx, texel_addr, 2)?);
            out.push(f32_const(ctx, 1.0)?);
        }
        TextureStorageFormat::Rgba16Unorm => {
            out.push(emit_unorm16_channel_load(ctx, texel_addr, 0)?);
            out.push(emit_unorm16_channel_load(ctx, texel_addr, 1)?);
            out.push(emit_unorm16_channel_load(ctx, texel_addr, 2)?);
            out.push(emit_unorm16_channel_load(ctx, texel_addr, 3)?);
        }
    }
    Ok(out)
}

fn emit_unorm16_channel_load(
    ctx: &mut LowerCtx<'_>,
    texel_addr: VReg,
    channel_index: u32,
) -> Result<VReg, LowerError> {
    let offset = channel_index.wrapping_mul(2);
    let raw = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Load16U {
        dst: raw,
        base: texel_addr,
        offset,
    });
    let converted = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Unorm16toF {
        dst: converted,
        src: raw,
    });
    Ok(converted)
}

fn f32_const(ctx: &mut LowerCtx<'_>, value: f32) -> Result<VReg, LowerError> {
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::FconstF32 { dst, value });
    Ok(dst)
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

#[cfg(test)]
mod texture_sampling_tests {
    use alloc::collections::BTreeMap;
    use alloc::format;
    use alloc::string::String;

    use naga::{Expression, SampleLevel};

    use lpir::{print_module, validate_module};
    use lps_shared::{
        TextureBindingSpec, TextureFilter, TextureShapeHint, TextureStorageFormat, TextureWrap,
    };

    use crate::LowerOptions;
    use crate::NagaModule;
    use crate::compile;
    use crate::lower_with_options;

    fn rgba16_general2d_spec() -> TextureBindingSpec {
        TextureBindingSpec {
            format: TextureStorageFormat::Rgba16Unorm,
            filter: TextureFilter::Nearest,
            wrap_x: TextureWrap::ClampToEdge,
            wrap_y: TextureWrap::Repeat,
            shape_hint: TextureShapeHint::General2D,
        }
    }

    fn rgba16_height_one_spec() -> TextureBindingSpec {
        TextureBindingSpec {
            format: TextureStorageFormat::Rgba16Unorm,
            filter: TextureFilter::Linear,
            wrap_x: TextureWrap::MirrorRepeat,
            wrap_y: TextureWrap::ClampToEdge,
            shape_hint: TextureShapeHint::HeightOne,
        }
    }

    #[test]
    fn texture_vec2_glsl_maps_to_naga_image_sample_implicit_lod() {
        let glsl = r#"
uniform sampler2D tex;
vec4 render(vec2 pos) {
    return texture(tex, pos);
}
"#;
        let NagaModule { module, functions } = compile(glsl).expect("compile");
        let h = functions
            .iter()
            .find(|(_, i)| i.name == "render")
            .expect("render")
            .0;
        let func = &module.functions[h];
        let mut ok = false;
        for (_, e) in func.expressions.iter() {
            if let Expression::ImageSample {
                gather: None,
                array_index: None,
                offset: None,
                depth_ref: None,
                clamp_to_edge: false,
                level,
                ..
            } = e
            {
                match level {
                    SampleLevel::Auto | SampleLevel::Zero => ok = true,
                    _ => {}
                }
            }
        }
        assert!(
            ok,
            "expected texture(sampler2D, vec2) → ImageSample with implicit/base LOD"
        );
    }

    #[test]
    fn general2d_rgba16_texture_lowers_to_texture2d_builtin_call() {
        let glsl = r#"
uniform sampler2D tex;
vec4 render(vec2 pos) {
    return texture(tex, pos);
}
"#;
        let naga = compile(glsl).expect("compile");
        let mut specs = BTreeMap::new();
        specs.insert(String::from("tex"), rgba16_general2d_spec());
        let opts = LowerOptions {
            texture_specs: specs,
            ..Default::default()
        };
        let (ir, _) = lower_with_options(&naga, &opts).expect("lower");
        validate_module(&ir).expect("validate");
        let s = print_module(&ir);
        assert!(
            s.contains("call @texture::texture2d_rgba16_unorm"),
            "expected @texture::texture2d_rgba16_unorm in:\n{s}"
        );
    }

    #[test]
    fn height_one_rgba16_texture_lowers_to_texture1d_builtin_call() {
        let glsl = r#"
uniform sampler2D tex;
vec4 render(vec2 pos) {
    return texture(tex, vec2(pos.x, 0.25));
}
"#;
        let naga = compile(glsl).expect("compile");
        let mut specs = BTreeMap::new();
        specs.insert(String::from("tex"), rgba16_height_one_spec());
        let opts = LowerOptions {
            texture_specs: specs,
            ..Default::default()
        };
        let (ir, _) = lower_with_options(&naga, &opts).expect("lower");
        validate_module(&ir).expect("validate");
        let s = print_module(&ir);
        assert!(
            s.contains("call @texture::texture1d_rgba16_unorm"),
            "expected @texture::texture1d_rgba16_unorm in:\n{s}"
        );
    }

    #[test]
    fn texture_errors_when_texture_binding_spec_missing() {
        let glsl = r#"
uniform sampler2D tex;
vec4 render(vec2 pos) {
    return texture(tex, pos);
}
"#;
        let naga = compile(glsl).expect("compile");
        let err = lower_with_options(&naga, &LowerOptions::default()).expect_err("lower");
        let msg = format!("{err}");
        assert!(
            msg.contains("texture `tex`") && msg.contains("no texture binding spec"),
            "{msg}"
        );
    }

    #[test]
    fn texture_errors_for_rgb16_format_until_builtin_exists() {
        let glsl = r#"
uniform sampler2D tex;
vec4 render(vec2 pos) {
    return texture(tex, pos);
}
"#;
        let naga = compile(glsl).expect("compile");
        let mut specs = BTreeMap::new();
        specs.insert(
            String::from("tex"),
            TextureBindingSpec {
                format: TextureStorageFormat::Rgb16Unorm,
                filter: TextureFilter::Nearest,
                wrap_x: TextureWrap::ClampToEdge,
                wrap_y: TextureWrap::ClampToEdge,
                shape_hint: TextureShapeHint::General2D,
            },
        );
        let opts = LowerOptions {
            texture_specs: specs,
            ..Default::default()
        };
        let err = lower_with_options(&naga, &opts).expect_err("lower");
        let msg = format!("{err}");
        assert!(
            msg.contains("texture `tex`") && msg.contains("Rgb16Unorm"),
            "expected rgb16 diagnostic, got: {msg}"
        );
    }
}

#[cfg(test)]
mod texel_fetch_clamp_lowering_tests {
    use lpir::{FunctionBuilder, IrType, LpirOp, TexelFetchBoundsMode};

    use super::TexelFetchCoords;
    use super::emit_clamp_texel_coords;

    fn select_count_for_mode(mode: TexelFetchBoundsMode) -> usize {
        let mut fb = FunctionBuilder::new("clamp_shape", &[]);
        let x = fb.alloc_vreg(IrType::I32);
        let y = fb.alloc_vreg(IrType::I32);
        let w = fb.alloc_vreg(IrType::I32);
        let h = fb.alloc_vreg(IrType::I32);
        let coords = TexelFetchCoords { x, y };
        emit_clamp_texel_coords(&mut fb, coords, w, h, mode).expect("emit_clamp_texel_coords");
        let f = fb.finish();
        f.body
            .iter()
            .filter(|op| matches!(op, LpirOp::Select { .. }))
            .count()
    }

    #[test]
    fn clamp_to_edge_emits_four_select_ops_for_x_and_y() {
        let n = select_count_for_mode(TexelFetchBoundsMode::ClampToEdge);
        assert_eq!(n, 4, "expected 2 Select per axis");
    }

    #[test]
    fn unchecked_mode_emits_no_select_for_clamp() {
        let n = select_count_for_mode(TexelFetchBoundsMode::Unchecked);
        assert_eq!(n, 0);
    }
}
