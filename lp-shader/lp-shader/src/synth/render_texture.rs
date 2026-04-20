//! Synthesise `__render_texture_<format>`: nested y/x loops, incremental offsets (Shape B), F32 → unorm16.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{
    CalleeRef, FuncId, IrFunction, IrType, LpirModule, LpirOp, builder::FunctionBuilder,
    lpir_module::VMCTX_VREG,
};
use lps_shared::{
    FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier, TextureStorageFormat,
};

/// `render_fn_index` was out of bounds for [`LpsModuleSig::functions`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SynthError {
    InvalidRenderFnIndex,
    /// No IR function matched the signature name at `render_fn_index`.
    RenderFunctionMissing,
}

/// Name suffix for the synthesised entry (e.g. `"__render_texture_rgba16"`).
pub fn render_texture_fn_name(format: TextureStorageFormat) -> &'static str {
    match format {
        TextureStorageFormat::R16Unorm => "__render_texture_r16",
        TextureStorageFormat::Rgb16Unorm => "__render_texture_rgb16",
        TextureStorageFormat::Rgba16Unorm => "__render_texture_rgba16",
    }
}

/// Append `__render_texture_<format>` to `module` and `meta` in lockstep; returns the function name.
pub fn synthesise_render_texture(
    module: &mut LpirModule,
    meta: &mut LpsModuleSig,
    render_fn_index: usize,
    format: TextureStorageFormat,
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

    let channels = format.channel_count();
    let bytes_per_px = format.bytes_per_pixel() as i32;
    if render_fn.return_types.len() != channels {
        return Err(SynthError::RenderFunctionMissing);
    }

    let has_globals = meta.globals_size() > 0;
    let needs_reset = has_globals && module_globals_mutated(module);

    const Q_HALF: i32 = 32768;
    const Q_ONE: i32 = 65536;

    let name = String::from(render_texture_fn_name(format));
    let mut fb = FunctionBuilder::new(name.as_str(), &[]);
    let tex_ptr = fb.add_param(IrType::Pointer);
    let width = fb.add_param(IrType::I32);
    let height = fb.add_param(IrType::I32);

    let pos_y = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 {
        dst: pos_y,
        value: Q_HALF,
    });
    let px_off = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 {
        dst: px_off,
        value: 0,
    });
    let y = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 { dst: y, value: 0 });

    let pos_x = fb.alloc_vreg(IrType::I32);
    let x = fb.alloc_vreg(IrType::I32);

    fb.push_loop();
    {
        let cmp_y = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IgeS {
            dst: cmp_y,
            lhs: y,
            rhs: height,
        });
        fb.push_if(cmp_y);
        fb.push(LpirOp::Break);
        fb.end_if();

        fb.push(LpirOp::IconstI32 {
            dst: pos_x,
            value: Q_HALF,
        });
        fb.push(LpirOp::IconstI32 { dst: x, value: 0 });

        fb.push_loop();
        {
            let cmp_x = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IgeS {
                dst: cmp_x,
                lhs: x,
                rhs: width,
            });
            fb.push_if(cmp_x);
            fb.push(LpirOp::Break);
            fb.end_if();

            if needs_reset {
                emit_globals_reset(&mut fb, meta);
            }

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

            let color: Vec<_> = (0..channels).map(|_| fb.alloc_vreg(IrType::F32)).collect();
            fb.push_call(
                render_callee,
                &[VMCTX_VREG, pos_x_f, pos_y_f],
                color.as_slice(),
            );

            let base = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::Iadd {
                dst: base,
                lhs: tex_ptr,
                rhs: px_off,
            });

            for ch in 0..channels {
                let unorm = fb.alloc_vreg(IrType::I32);
                fb.push(LpirOp::FtoUnorm16 {
                    dst: unorm,
                    src: color[ch],
                });
                fb.push(LpirOp::Store16 {
                    base,
                    offset: (ch as u32) * 2,
                    value: unorm,
                });
            }

            fb.push(LpirOp::IaddImm {
                dst: px_off,
                src: px_off,
                imm: bytes_per_px,
            });
            fb.push(LpirOp::IaddImm {
                dst: pos_x,
                src: pos_x,
                imm: Q_ONE,
            });
            fb.push(LpirOp::IaddImm {
                dst: x,
                src: x,
                imm: 1,
            });
        }
        fb.end_loop();

        fb.push(LpirOp::IaddImm {
            dst: pos_y,
            src: pos_y,
            imm: Q_ONE,
        });
        fb.push(LpirOp::IaddImm {
            dst: y,
            src: y,
            imm: 1,
        });
    }
    fb.end_loop();
    fb.push_return(&[]);

    let ir_fn = fb.finish();
    append_local_function(module, ir_fn);

    meta.functions.push(LpsFnSig {
        name: name.clone(),
        return_type: LpsType::Void,
        parameters: vec![
            FnParam {
                name: String::from("__tex_ptr"),
                ty: LpsType::UInt,
                qualifier: ParamQualifier::In,
            },
            FnParam {
                name: String::from("__width"),
                ty: LpsType::Int,
                qualifier: ParamQualifier::In,
            },
            FnParam {
                name: String::from("__height"),
                ty: LpsType::Int,
                qualifier: ParamQualifier::In,
            },
        ],
        kind: LpsFnKind::Synthetic,
    });

    Ok(name)
}

fn module_globals_mutated(_module: &LpirModule) -> bool {
    true
}

fn emit_globals_reset(fb: &mut FunctionBuilder, meta: &LpsModuleSig) {
    let globals_addr = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IaddImm {
        dst: globals_addr,
        src: VMCTX_VREG,
        imm: i32::try_from(meta.globals_offset()).expect("globals_offset fits i32"),
    });
    let snapshot_addr = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IaddImm {
        dst: snapshot_addr,
        src: VMCTX_VREG,
        imm: i32::try_from(meta.snapshot_offset()).expect("snapshot_offset fits i32"),
    });
    let n_bytes = u32::try_from(meta.globals_size()).expect("globals_size fits u32");
    fb.push(LpirOp::Memcpy {
        dst_addr: globals_addr,
        src_addr: snapshot_addr,
        size: n_bytes,
    });
}

fn append_local_function(module: &mut LpirModule, func: IrFunction) {
    let next_id = module
        .functions
        .keys()
        .map(|k| k.0)
        .max()
        .map(|m| m.saturating_add(1))
        .unwrap_or(0);
    module.functions.insert(FuncId(next_id), func);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpir::{CalleeRef, IrType, LpirModule, LpirOp};
    use lpvm::validate_render_texture_sig_ir;

    #[test]
    fn synth_rgba16_appends_function_and_sig_in_lockstep() {
        let (mut ir, mut meta) = make_stub_render_module(LpsType::Vec4);
        let n_before = ir.functions.len();
        let name =
            synthesise_render_texture(&mut ir, &mut meta, 0, TextureStorageFormat::Rgba16Unorm)
                .expect("synth");
        assert_eq!(name, "__render_texture_rgba16");
        assert_eq!(ir.functions.len(), n_before + 1);
        assert_eq!(meta.functions.len(), n_before + 1);
        let last = meta.functions.last().expect("sig");
        assert_eq!(last.name, name);
        assert_eq!(last.kind, LpsFnKind::Synthetic);
    }

    #[test]
    fn synth_r16_picks_correct_name_and_arity() {
        let (mut ir, mut meta) = make_stub_render_module(LpsType::Float);
        let name = synthesise_render_texture(&mut ir, &mut meta, 0, TextureStorageFormat::R16Unorm)
            .expect("synth");
        assert_eq!(name, "__render_texture_r16");
        let synth_fn = ir
            .functions
            .values()
            .find(|f| f.name == name)
            .expect("ir fn");
        assert_eq!(synth_fn.param_count, 3);
        assert!(synth_fn.return_types.is_empty());
    }

    #[test]
    fn synth_signature_passes_phase_2_validator() {
        let (mut ir, mut meta) = make_stub_render_module(LpsType::Vec4);
        let name =
            synthesise_render_texture(&mut ir, &mut meta, 0, TextureStorageFormat::Rgba16Unorm)
                .expect("synth");
        let synth_ir = ir
            .functions
            .values()
            .find(|f| f.name == name)
            .expect("synth ir");
        validate_render_texture_sig_ir(synth_ir).expect("synth must satisfy validator");
    }

    /// `__render_texture_*` must lower each channel with [`LpirOp::FtoUnorm16`], not a long
    /// expand-in-IR sequence (correctness vs Q32 half-scale is covered by `lp-shader` e2e tests).
    #[test]
    fn synth_body_uses_fto_unorm16_per_channel() {
        let (mut ir, mut meta) = make_stub_render_module(LpsType::Float);
        let name = synthesise_render_texture(&mut ir, &mut meta, 0, TextureStorageFormat::R16Unorm)
            .expect("synth");
        let synth_fn = ir
            .functions
            .values()
            .find(|f| f.name == name)
            .expect("synth fn");
        let fto = synth_fn
            .body
            .iter()
            .filter(|op| matches!(op, LpirOp::FtoUnorm16 { .. }))
            .count();
        assert_eq!(fto, 1, "expected one FtoUnorm16 for R16");
    }

    /// Inliner regression sanity (Q7).
    ///
    /// Today, on this branch (no inliner integrated), `__render_texture`
    /// must contain exactly **one** `Call` op targeting the user `render`
    /// function. When the LPIR inliner integration milestone lands and
    /// fuses `render` into the loop body, this test will start failing —
    /// at which point the assertion should be **inverted**:
    ///
    /// - assert zero `Call` ops in the synthesised body, and
    /// - assert presence of inlined-body ops (the user `render`'s
    ///   control flow / arithmetic now appearing inside this function).
    ///
    /// See: docs/plans/2026-04-17-lp-shader-textures-stage-v/00-notes.md (Q7)
    ///      docs/roadmaps/2026-04-16-lp-shader-textures/m2-render-frame.md
    #[test]
    fn synthesised_body_calls_render_once_inliner_unintegrated() {
        let (mut ir, mut meta) = make_stub_render_module(LpsType::Vec4);
        let name =
            synthesise_render_texture(&mut ir, &mut meta, 0, TextureStorageFormat::Rgba16Unorm)
                .expect("synth");
        let synth_fn = ir
            .functions
            .values()
            .find(|f| f.name == name)
            .expect("synth fn");
        let render_callee_id = ir
            .functions
            .iter()
            .find(|(_, f)| f.name == "render")
            .map(|(id, _)| *id)
            .expect("render id");
        let calls_to_render = synth_fn.body.iter().filter(|op| {
            matches!(op, LpirOp::Call { callee: CalleeRef::Local(id), .. } if *id == render_callee_id)
        }).count();
        assert_eq!(
            calls_to_render, 1,
            "expected exactly one Call to render in __render_texture body"
        );
    }

    fn make_stub_render_module(return_ty: LpsType) -> (LpirModule, LpsModuleSig) {
        let return_ir = return_ir_types(&return_ty);
        let mut fb = FunctionBuilder::new("render", return_ir.as_slice());
        let _p0 = fb.add_param(IrType::F32);
        let _p1 = fb.add_param(IrType::F32);
        let q_one_bits = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 {
            dst: q_one_bits,
            value: 65536,
        });
        let mut rets: Vec<_> = return_ir
            .iter()
            .map(|_| fb.alloc_vreg(IrType::F32))
            .collect();
        for r in &mut rets {
            fb.push(LpirOp::FfromI32Bits {
                dst: *r,
                src: q_one_bits,
            });
        }
        fb.push_return(rets.as_slice());
        let render_fn = fb.finish();

        let mut module = LpirModule::new();
        super::append_local_function(&mut module, render_fn);

        let meta = LpsModuleSig {
            functions: alloc::vec![LpsFnSig {
                name: String::from("render"),
                return_type: return_ty,
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

    fn return_ir_types(ret: &LpsType) -> Vec<IrType> {
        match ret {
            LpsType::Void => Vec::new(),
            LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => {
                alloc::vec![IrType::F32]
            }
            LpsType::Vec2 | LpsType::IVec2 | LpsType::UVec2 | LpsType::BVec2 => {
                alloc::vec![IrType::F32; 2]
            }
            LpsType::Vec3 | LpsType::IVec3 | LpsType::UVec3 | LpsType::BVec3 => {
                alloc::vec![IrType::F32; 3]
            }
            LpsType::Vec4 | LpsType::IVec4 | LpsType::UVec4 | LpsType::BVec4 => {
                alloc::vec![IrType::F32; 4]
            }
            _ => panic!("stub return_ty not supported"),
        }
    }
}
