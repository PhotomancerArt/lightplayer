//! Synthesise `__render_samples_rgba16`: point loop over packed Q16.16 positions.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{
    CalleeRef, IrType, LpirModule, LpirOp, builder::FunctionBuilder, lpir_module::VMCTX_VREG,
};
use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};

use super::render_texture::{SynthError, append_local_function, emit_globals_reset};

pub const RENDER_SAMPLES_RGBA16_FN: &str = "__render_samples_rgba16";

/// Append `__render_samples_rgba16` to `module` and `meta` in lockstep.
pub fn synthesise_render_samples_rgba16(
    module: &mut LpirModule,
    meta: &mut LpsModuleSig,
    render_fn_index: usize,
) -> Result<String, SynthError> {
    let render_sig = meta
        .functions
        .get(render_fn_index)
        .ok_or(SynthError::InvalidRenderFnIndex)?;
    let render_name = render_sig.name.as_str();
    let (&render_id, render_fn) = module
        .functions
        .iter()
        .find(|(_, f)| f.name == render_name)
        .ok_or(SynthError::RenderFunctionMissing)?;
    let render_callee = CalleeRef::Local(render_id);

    if render_fn.return_types.len() != 4 {
        return Err(SynthError::RenderFunctionMissing);
    }

    let needs_reset = meta.globals_size() > 0;

    let name = String::from(RENDER_SAMPLES_RGBA16_FN);
    let mut fb = FunctionBuilder::new(name.as_str(), &[]);
    let points_ptr = fb.add_param(IrType::Pointer);
    let out_ptr = fb.add_param(IrType::Pointer);
    let count = fb.add_param(IrType::I32);

    let i = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 { dst: i, value: 0 });
    let points_off = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 {
        dst: points_off,
        value: 0,
    });
    let out_off = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 {
        dst: out_off,
        value: 0,
    });

    fb.push_loop();
    {
        let done = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IgeS {
            dst: done,
            lhs: i,
            rhs: count,
        });
        fb.push_if(done);
        fb.push(LpirOp::Break);
        fb.end_if();

        if needs_reset {
            emit_globals_reset(&mut fb, meta);
        }

        let point_base = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::Iadd {
            dst: point_base,
            lhs: points_ptr,
            rhs: points_off,
        });
        let pos_x = fb.alloc_vreg(IrType::I32);
        let pos_y = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::Load {
            dst: pos_x,
            base: point_base,
            offset: 0,
        });
        fb.push(LpirOp::Load {
            dst: pos_y,
            base: point_base,
            offset: 4,
        });

        let pos_x_f = fb.alloc_vreg(IrType::F32);
        let pos_y_f = fb.alloc_vreg(IrType::F32);
        fb.push(LpirOp::FfromI32Bits {
            dst: pos_x_f,
            src: pos_x,
        });
        fb.push(LpirOp::FfromI32Bits {
            dst: pos_y_f,
            src: pos_y,
        });

        let color: Vec<_> = (0..4).map(|_| fb.alloc_vreg(IrType::F32)).collect();
        fb.push_call(
            render_callee,
            &[VMCTX_VREG, pos_x_f, pos_y_f],
            color.as_slice(),
        );

        let sample_base = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::Iadd {
            dst: sample_base,
            lhs: out_ptr,
            rhs: out_off,
        });
        for (ch, src) in color.iter().copied().enumerate() {
            let unorm = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::FtoUnorm16 { dst: unorm, src });
            fb.push(LpirOp::Store16 {
                base: sample_base,
                offset: (ch as u32) * 2,
                value: unorm,
            });
        }

        fb.push(LpirOp::IaddImm {
            dst: points_off,
            src: points_off,
            imm: 8,
        });
        fb.push(LpirOp::IaddImm {
            dst: out_off,
            src: out_off,
            imm: 8,
        });
        fb.push(LpirOp::IaddImm {
            dst: i,
            src: i,
            imm: 1,
        });
    }
    fb.end_loop();
    fb.push_return(&[]);

    append_local_function(module, fb.finish());

    meta.functions.push(LpsFnSig {
        name: name.clone(),
        return_type: LpsType::Void,
        parameters: vec![
            FnParam {
                name: String::from("__points_ptr"),
                ty: LpsType::UInt,
                qualifier: ParamQualifier::In,
            },
            FnParam {
                name: String::from("__out_ptr"),
                ty: LpsType::UInt,
                qualifier: ParamQualifier::In,
            },
            FnParam {
                name: String::from("__count"),
                ty: LpsType::Int,
                qualifier: ParamQualifier::In,
            },
        ],
        kind: LpsFnKind::Synthetic,
    });

    Ok(name)
}
