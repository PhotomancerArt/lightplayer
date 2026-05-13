use alloc::vec::Vec;

use lpir::{IrType, LpirOp, TexelFetchBoundsMode, VReg};
use lps_shared::{TextureFilter, TextureShapeHint, TextureStorageFormat, TextureWrap};

use crate::hir::{HirExpr, HirTextureOperand, ImportKey};
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue, lower_expr};

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

pub(in crate::lower) fn lower_texel_fetch(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    sampler: &HirTextureOperand,
    coord: &HirExpr,
    lod: &HirExpr,
) -> Result<LowerValue, Diagnostic> {
    validate_lod_zero(lod)?;
    let spec = ctx
        .texture_specs
        .get(sampler.path.as_str())
        .ok_or_else(|| {
            Diagnostic::error(
                span,
                alloc::format!(
                    "texelFetch `{}`: no texture binding spec for sampler uniform `{}`",
                    sampler.path,
                    sampler.path
                ),
            )
        })?;
    let bpp = i32::try_from(spec.format.bytes_per_pixel())
        .map_err(|_| Diagnostic::error(span, "texelFetch: bytes_per_pixel does not fit i32"))?;
    let desc = load_texture_descriptor_vregs(ctx, sampler.descriptor_byte_offset);
    let coord = lower_expr(ctx, coord)?;
    if coord.lanes.len() != 2 {
        return Err(Diagnostic::error(
            span,
            "texelFetch coordinate must be ivec2",
        ));
    }
    let coords = TexelFetchCoords {
        x: coord.lanes[0],
        y: coord.lanes[1],
    };
    let coords =
        emit_clamp_texel_coords(ctx, coords, desc.width, desc.height, ctx.texel_fetch_bounds);
    let texel_addr = emit_texel_byte_address(ctx, &desc, coords, bpp);
    let lanes = emit_texel_fetch_vec4_unorm(ctx, texel_addr, spec.format);
    Ok(LowerValue {
        ty: lps_shared::LpsType::Vec4,
        lanes,
    })
}

pub(in crate::lower) fn lower_texture_sample(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    sampler: &HirTextureOperand,
    coord: &HirExpr,
    import: &ImportKey,
) -> Result<LowerValue, Diagnostic> {
    let spec = *ctx
        .texture_specs
        .get(sampler.path.as_str())
        .ok_or_else(|| {
            Diagnostic::error(
                span,
                alloc::format!(
                    "texture `{}`: no texture binding spec for sampler uniform `{}`",
                    sampler.path,
                    sampler.path
                ),
            )
        })?;
    let callee = *ctx.import_map.get(import).ok_or_else(|| {
        Diagnostic::error(
            span,
            alloc::format!("missing texture import for {import:?}"),
        )
    })?;
    let desc = load_texture_descriptor_vregs(ctx, sampler.descriptor_byte_offset);
    let uv = lower_expr(ctx, coord)?;
    if uv.lanes.len() != 2 {
        return Err(Diagnostic::error(span, "texture coordinate must be vec2"));
    }
    let u_q32 = spill_f32_q32_lane_as_i32_vreg(ctx, uv.lanes[0]);
    let filter_abi = iconst(ctx, filter_abi(spec.filter));
    let wrap_x_abi = iconst(ctx, wrap_abi(spec.wrap_x));

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
            let v_q32 = spill_f32_q32_lane_as_i32_vreg(ctx, uv.lanes[1]);
            let wrap_y_abi = iconst(ctx, wrap_abi(spec.wrap_y));
            args.push(desc.height);
            args.push(desc.row_stride);
            args.push(u_q32);
            args.push(v_q32);
            args.push(filter_abi);
            args.push(wrap_x_abi);
            args.push(wrap_y_abi);
        }
        TextureShapeHint::HeightOne => {
            args.push(desc.row_stride);
            args.push(u_q32);
            args.push(filter_abi);
            args.push(wrap_x_abi);
        }
    }

    ctx.fb.push_call(callee, &args, &[]);
    let mut lanes = Vec::new();
    for i in 0..4u32 {
        let raw = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Load {
            dst: raw,
            base: out_addr,
            offset: i * 4,
        });
        let lane = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::FfromI32Bits {
            dst: lane,
            src: raw,
        });
        lanes.push(lane);
    }
    Ok(LowerValue {
        ty: lps_shared::LpsType::Vec4,
        lanes,
    })
}

fn validate_lod_zero(lod: &HirExpr) -> Result<(), Diagnostic> {
    match &lod.kind {
        crate::hir::HirExprKind::IntLiteral(0) => Ok(()),
        crate::hir::HirExprKind::UIntLiteral(0) => Ok(()),
        crate::hir::HirExprKind::IntLiteral(v) => Err(Diagnostic::error(
            lod.span,
            alloc::format!("texelFetch lod must be literal 0, got {v}"),
        )),
        crate::hir::HirExprKind::UIntLiteral(v) => Err(Diagnostic::error(
            lod.span,
            alloc::format!("texelFetch lod must be literal 0, got {v}"),
        )),
        _ => Err(Diagnostic::error(
            lod.span,
            "texelFetch dynamic lod is not supported",
        )),
    }
}

fn load_texture_descriptor_vregs(
    ctx: &mut LowerCtx<'_>,
    descriptor_base_byte_offset: u32,
) -> TextureDescriptorVRegs {
    let mut load_lane = |offset: u32| {
        let dst = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Load {
            dst,
            base: ctx.vmctx,
            offset: descriptor_base_byte_offset.saturating_add(offset),
        });
        dst
    };
    TextureDescriptorVRegs {
        ptr: load_lane(0),
        width: load_lane(4),
        height: load_lane(8),
        row_stride: load_lane(12),
    }
}

fn emit_clamp_texel_coords(
    ctx: &mut LowerCtx<'_>,
    coords: TexelFetchCoords,
    width: VReg,
    height: VReg,
    mode: TexelFetchBoundsMode,
) -> TexelFetchCoords {
    match mode {
        TexelFetchBoundsMode::Unchecked => coords,
        TexelFetchBoundsMode::ClampToEdge => TexelFetchCoords {
            x: clamp_signed_coord_to_extent(ctx, coords.x, width),
            y: clamp_signed_coord_to_extent(ctx, coords.y, height),
        },
    }
}

fn clamp_signed_coord_to_extent(ctx: &mut LowerCtx<'_>, v: VReg, extent: VReg) -> VReg {
    let zero = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 {
        dst: zero,
        value: 0,
    });
    let lt0 = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IltS {
        dst: lt0,
        lhs: v,
        rhs: zero,
    });
    let after_low = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst: after_low,
        cond: lt0,
        if_true: zero,
        if_false: v,
    });

    let max_v = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IsubImm {
        dst: max_v,
        src: extent,
        imm: 1,
    });
    let gt_max = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IgtS {
        dst: gt_max,
        lhs: after_low,
        rhs: max_v,
    });
    let out = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst: out,
        cond: gt_max,
        if_true: max_v,
        if_false: after_low,
    });
    out
}

fn emit_texel_byte_address(
    ctx: &mut LowerCtx<'_>,
    desc: &TextureDescriptorVRegs,
    coords: TexelFetchCoords,
    bytes_per_pixel: i32,
) -> VReg {
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
    addr
}

fn emit_texel_fetch_vec4_unorm(
    ctx: &mut LowerCtx<'_>,
    texel_addr: VReg,
    format: TextureStorageFormat,
) -> Vec<VReg> {
    match format {
        TextureStorageFormat::R16Unorm => alloc::vec![
            emit_unorm16_channel_load(ctx, texel_addr, 0),
            f32_const(ctx, 0.0),
            f32_const(ctx, 0.0),
            f32_const(ctx, 1.0),
        ],
        TextureStorageFormat::Rgb16Unorm => alloc::vec![
            emit_unorm16_channel_load(ctx, texel_addr, 0),
            emit_unorm16_channel_load(ctx, texel_addr, 1),
            emit_unorm16_channel_load(ctx, texel_addr, 2),
            f32_const(ctx, 1.0),
        ],
        TextureStorageFormat::Rgba16Unorm => alloc::vec![
            emit_unorm16_channel_load(ctx, texel_addr, 0),
            emit_unorm16_channel_load(ctx, texel_addr, 1),
            emit_unorm16_channel_load(ctx, texel_addr, 2),
            emit_unorm16_channel_load(ctx, texel_addr, 3),
        ],
    }
}

fn emit_unorm16_channel_load(ctx: &mut LowerCtx<'_>, texel_addr: VReg, channel_index: u32) -> VReg {
    let raw = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Load16U {
        dst: raw,
        base: texel_addr,
        offset: channel_index * 2,
    });
    let converted = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Unorm16toF {
        dst: converted,
        src: raw,
    });
    converted
}

fn spill_f32_q32_lane_as_i32_vreg(ctx: &mut LowerCtx<'_>, f: VReg) -> VReg {
    let slot = ctx.fb.alloc_slot(4);
    let addr = ctx.fb.alloc_vreg(IrType::Pointer);
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
    raw
}

fn iconst(ctx: &mut LowerCtx<'_>, value: i32) -> VReg {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 { dst, value });
    dst
}

fn f32_const(ctx: &mut LowerCtx<'_>, value: f32) -> VReg {
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::FconstF32 { dst, value });
    dst
}

fn filter_abi(filter: TextureFilter) -> i32 {
    filter.to_builtin_abi() as i32
}

fn wrap_abi(wrap: TextureWrap) -> i32 {
    wrap.to_builtin_abi() as i32
}
