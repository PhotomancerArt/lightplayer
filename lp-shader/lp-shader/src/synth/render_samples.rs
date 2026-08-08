//! Synthesise `__render_samples_rgba16`: point loop over packed Q16.16 positions.
//!
//! # Packed point layout (the host↔guest contract)
//!
//! The `points` buffer is **tightly packed Q16.16 words, one lane group per
//! point, in the declared space's lane count**
//! ([`ShaderEntrySpace::coord_lanes`]):
//!
//! | declared space | stride | word layout |
//! |---|---|---|
//! | `TwoD` (`render_2d`) | 8 bytes | `[x0, y0, x1, y1, …]` |
//! | `OneD` (`render_1d`) | 4 bytes | `[t0, t1, t2, …]` |
//!
//! Both are pixel-space coordinates in the same Q16.16 frame currency the
//! render-texture walk uses — *not* normalized UVs, and not scaled by
//! `outputSize`. The 1D layout is the packing space-tagged sample requests
//! must produce.
//!
//! The buffer *allocation* is unchanged and stays pair-sized
//! ([`crate::LpsEngine::alloc_sample_points`] reserves `count × 8` bytes in
//! both spaces): a 1D batch simply leaves the second half of the allocation
//! unread, which keeps one allocation shape across a space change and costs
//! only the slack. A 1D writer fills `points[0..count]`.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{
    CalleeRef, FloatMode, IrType, LpirModule, LpirOp, builder::FunctionBuilder,
    lpir_module::VMCTX_VREG,
};
use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};
use lpvm::{DEFAULT_INVOCATION_FUEL, VMCTX_OFFSET_FUEL};

use super::render_texture::{
    Q16CoordDecoder, SynthError, append_local_function, emit_globals_reset,
};
use crate::entry_space::ShaderEntrySpace;

pub const RENDER_SAMPLES_RGBA16_FN: &str = "__render_samples_rgba16";

/// Append `__render_samples_rgba16` to `module` and `meta` in lockstep.
///
/// `space` decides the packed point stride and the entry's arity — see the
/// module docs for the layout.
pub fn synthesise_render_samples_rgba16(
    module: &mut LpirModule,
    meta: &mut LpsModuleSig,
    render_fn_index: usize,
    float_mode: FloatMode,
    space: ShaderEntrySpace,
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
    // Hoisted out of the sample loop: `points` is Q16.16 in every mode, and
    // only the hand-off to `render` differs. See `Q16CoordDecoder`.
    let coords = Q16CoordDecoder::new(&mut fb, float_mode);

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
        // Loads first, then decodes — the op order the Q32 wrapper has
        // always emitted, so a 2D shader's codegen is byte-identical to
        // what it was before 1D existed.
        let words: Vec<_> = (0..space.coord_lanes())
            .map(|lane| {
                let word = fb.alloc_vreg(IrType::I32);
                fb.push(LpirOp::Load {
                    dst: word,
                    base: point_base,
                    offset: (lane as u32) * 4,
                });
                word
            })
            .collect();
        let mut call_args = alloc::vec![VMCTX_VREG];
        for word in words {
            call_args.push(coords.decode(&mut fb, word));
        }

        let color: Vec<_> = (0..4).map(|_| fb.alloc_vreg(IrType::F32)).collect();
        fb.push_call(render_callee, call_args.as_slice(), color.as_slice());

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
            imm: (space.coord_lanes() as i32) * 4,
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
        let name = synthesise_render_samples_rgba16(
            &mut ir,
            &mut meta,
            0,
            FloatMode::Q32,
            ShaderEntrySpace::TwoD,
        )
        .expect("synth");
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

    /// The Q32 wrapper is *unchanged* by f32 existing: its coordinate decode is
    /// still the single reinterpret, and no scale constant is materialised.
    ///
    /// Asserted on the IR rather than on rendered pixels because that is where
    /// "Q32 stays bit-identical" is actually decided — a stray extra op here
    /// would move every Q32 filetest snapshot downstream.
    #[test]
    fn q32_decodes_points_with_a_bare_reinterpret() {
        let (mut ir, mut meta) = make_stub_render_module();
        let name = synthesise_render_samples_rgba16(
            &mut ir,
            &mut meta,
            0,
            FloatMode::Q32,
            ShaderEntrySpace::TwoD,
        )
        .expect("synth");
        let body = synth_body(&ir, &name);

        assert_eq!(
            body.iter()
                .filter(|op| matches!(op, LpirOp::FfromI32Bits { .. }))
                .count(),
            2,
            "one reinterpret per coordinate"
        );
        assert!(
            !body
                .iter()
                .any(|op| matches!(op, LpirOp::ItofS { .. } | LpirOp::FconstF32 { .. })),
            "Q32 must not gain a numeric conversion or a scale constant"
        );
    }

    /// Float mode converts instead of reinterpreting: `points` is still Q16.16,
    /// so each coordinate becomes `ItofS(word) * 2^-16` against one hoisted
    /// constant. Reinterpreting here is the defect
    /// (`docs/defects/2026-08-02-f32-shader-cannot-render-a-frame.md`).
    #[test]
    fn f32_decodes_points_by_scaling_the_fixed_point_word() {
        let (mut ir, mut meta) = make_stub_render_module();
        let name = synthesise_render_samples_rgba16(
            &mut ir,
            &mut meta,
            0,
            FloatMode::F32,
            ShaderEntrySpace::TwoD,
        )
        .expect("synth");
        let body = synth_body(&ir, &name);

        assert!(
            !body
                .iter()
                .any(|op| matches!(op, LpirOp::FfromI32Bits { .. })),
            "a bit reinterpret of a Q16.16 word is meaningless in Float mode"
        );
        assert_eq!(
            body.iter()
                .filter(|op| matches!(op, LpirOp::ItofS { .. }))
                .count(),
            2,
            "one int→float conversion per coordinate"
        );
        let scales: Vec<f32> = body
            .iter()
            .filter_map(|op| match op {
                LpirOp::FconstF32 { value, .. } => Some(*value),
                _ => None,
            })
            .collect();
        assert_eq!(
            scales,
            alloc::vec![1.0 / 65536.0],
            "exactly one hoisted 2^-16 scale, shared by both coordinates"
        );
    }

    fn synth_body(ir: &LpirModule, name: &str) -> Vec<LpirOp> {
        ir.functions
            .values()
            .find(|f| f.name == name)
            .expect("synth fn")
            .body
            .iter()
            .cloned()
            .collect()
    }

    fn make_stub_render_module() -> (LpirModule, LpsModuleSig) {
        make_stub_entry_module(ShaderEntrySpace::TwoD)
    }

    fn make_stub_entry_module(space: ShaderEntrySpace) -> (LpirModule, LpsModuleSig) {
        let return_ir = alloc::vec![IrType::F32; 4];
        let mut fb = FunctionBuilder::new(space.entry_name(), return_ir.as_slice());
        for _ in 0..space.coord_lanes() {
            let _p = fb.add_param(IrType::F32);
        }
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
                name: String::from(space.entry_name()),
                return_type: LpsType::Vec4,
                parameters: alloc::vec![FnParam {
                    name: String::from("pos"),
                    ty: match space {
                        ShaderEntrySpace::TwoD => LpsType::Vec2,
                        ShaderEntrySpace::OneD => LpsType::Float,
                    },
                    qualifier: ParamQualifier::In,
                }],
                kind: LpsFnKind::UserDefined,
            }],
            ..Default::default()
        };
        (module, meta)
    }

    /// The 1D sample loop walks **tightly packed single words**: one load at
    /// offset 0 per point and a 4-byte stride, versus the 2D pair's two
    /// loads and 8-byte stride. This is the layout space-tagged sample
    /// requests must produce.
    #[test]
    fn one_d_decodes_single_lane_points_at_a_four_byte_stride() {
        let (mut ir, mut meta) = make_stub_entry_module(ShaderEntrySpace::OneD);
        let name = synthesise_render_samples_rgba16(
            &mut ir,
            &mut meta,
            0,
            FloatMode::Q32,
            ShaderEntrySpace::OneD,
        )
        .expect("synth");
        let body = synth_body(&ir, &name);

        assert_eq!(
            body.iter()
                .filter(|op| matches!(op, LpirOp::Load { .. }))
                .count(),
            1,
            "one coordinate word per point"
        );
        assert!(
            body.iter().any(|op| matches!(
                op,
                LpirOp::Load { offset, .. } if *offset == 0
            )),
            "the single lane sits at the point's own offset"
        );
        assert!(
            body.iter()
                .any(|op| matches!(op, LpirOp::IaddImm { imm, .. } if *imm == 4)),
            "points advance one word per sample"
        );
        assert!(
            !body
                .iter()
                .any(|op| matches!(op, LpirOp::Load { offset, .. } if *offset == 4)),
            "no second lane is read"
        );
    }
}
