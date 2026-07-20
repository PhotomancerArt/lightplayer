//! Synthesise `__render_samples_rgba16`: point loop over packed Q16.16 positions.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{
    CalleeRef, IrType, LpirModule, LpirOp, builder::FunctionBuilder, lpir_module::VMCTX_VREG,
};
use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};
use lpvm::{DEFAULT_INVOCATION_FUEL, VMCTX_OFFSET_FUEL};

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
    // Per-invocation fuel metering (lpvm-native): constant tank size stored
    // into the vmctx header per sample; `i` doubles as the invocation index.
    let fuel_budget = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 {
        dst: fuel_budget,
        value: DEFAULT_INVOCATION_FUEL as i32,
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

        // Re-arm the per-invocation fuel tank (vmctx+0) and record the
        // current sample index (vmctx+4). After the bounds-check Break so an
        // exiting iteration does not re-arm: an exhausted sample's trap
        // escapes via the wrapper's own back-edge check (which observes fuel
        // 0) before the next reset runs. The resets never write the trap
        // slot — the trap code stays the authoritative host signal. The
        // wrapper's own loop back-edge costs each sample's tank a couple of
        // units; that overhead is part of the budget.
        fb.push(LpirOp::Store {
            base: VMCTX_VREG,
            offset: VMCTX_OFFSET_FUEL as u32,
            value: fuel_budget,
        });
        fb.push(LpirOp::Store {
            base: VMCTX_VREG,
            offset: VMCTX_OFFSET_FUEL as u32 + 4,
            value: i,
        });

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

#[cfg(test)]
mod tests {
    use super::*;
    use lpir::builder::FunctionBuilder;
    use lpir::lpir_module::VMCTX_VREG;

    /// Per-invocation fuel resets: the wrapper stores the tank constant to
    /// vmctx+0 and the sample index to vmctx+4, after the bounds-check Break
    /// and before the call to the user render fn.
    #[test]
    fn synth_body_arms_per_invocation_fuel_before_render_call() {
        let (mut ir, mut meta) = make_stub_render_module();
        let name = synthesise_render_samples_rgba16(&mut ir, &mut meta, 0).expect("synth");
        let synth_fn = ir
            .functions
            .values()
            .find(|f| f.name == name)
            .expect("synth fn");
        let body: Vec<&LpirOp> = synth_fn.body.iter().collect();

        let fuel_store = body
            .iter()
            .position(|op| {
                matches!(
                    op,
                    LpirOp::Store { base, offset, .. }
                        if *base == VMCTX_VREG && *offset == VMCTX_OFFSET_FUEL as u32
                )
            })
            .expect("fuel reset store to vmctx+0");
        let idx_store = body
            .iter()
            .position(|op| {
                matches!(
                    op,
                    LpirOp::Store { base, offset, .. }
                        if *base == VMCTX_VREG && *offset == VMCTX_OFFSET_FUEL as u32 + 4
                )
            })
            .expect("sample index store to vmctx+4");
        let brk = body
            .iter()
            .position(|op| matches!(op, LpirOp::Break))
            .expect("bounds-check Break");
        let call = body
            .iter()
            .position(|op| matches!(op, LpirOp::Call { .. }))
            .expect("call to render");

        assert!(
            brk < fuel_store && fuel_store < call && brk < idx_store && idx_store < call,
            "fuel/index stores must sit between the bounds-check Break and the render call \
             (break {brk}, fuel {fuel_store}, idx {idx_store}, call {call})"
        );
        assert!(
            body.iter().any(|op| matches!(
                op,
                LpirOp::IconstI32 { value, .. } if *value == DEFAULT_INVOCATION_FUEL as i32
            )),
            "DEFAULT_INVOCATION_FUEL constant must be materialised"
        );
    }

    fn make_stub_render_module() -> (LpirModule, LpsModuleSig) {
        let return_ir = alloc::vec![IrType::F32; 4];
        let mut fb = FunctionBuilder::new("render", return_ir.as_slice());
        let _px = fb.add_param(IrType::F32);
        let _py = fb.add_param(IrType::F32);
        let one_bits = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 {
            dst: one_bits,
            value: 65536,
        });
        let rets: Vec<_> = (0..4)
            .map(|_| {
                let r = fb.alloc_vreg(IrType::F32);
                fb.push(LpirOp::FfromI32Bits {
                    dst: r,
                    src: one_bits,
                });
                r
            })
            .collect();
        fb.push_return(rets.as_slice());

        let mut module = LpirModule::new();
        append_local_function(&mut module, fb.finish());

        let meta = LpsModuleSig {
            functions: alloc::vec![LpsFnSig {
                name: String::from("render"),
                return_type: LpsType::Vec4,
                parameters: alloc::vec![FnParam {
                    name: String::from("pos"),
                    ty: LpsType::Vec2,
                    qualifier: ParamQualifier::In,
                }],
                kind: LpsFnKind::UserDefined,
            }],
            ..Default::default()
        };
        (module, meta)
    }
}
