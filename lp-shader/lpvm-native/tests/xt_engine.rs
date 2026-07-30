//! `NativeEmuEngine` on Xtensa: does the *same LPIR* produce the *same answers*
//! through the Xtensa backend as through the proven rv32 one?
//!
//! That question is the whole point of this engine. Every test here compiles one
//! module twice — once for `IsaTarget::Rv32imac`, once for `IsaTarget::Xtensa` —
//! and compares. rv32 is the oracle, so a divergence is a real Xtensa defect
//! rather than a disagreement about what the answer should be.
//!
//! The **builtin-calling** cases matter most: they exercise the base-image link
//! (symbol resolution out of the cross-compiled builtins ELF, `l32r` literal-pool
//! patching, the `callx8` sequence into guest builtin code). An engine that faked
//! any of that would pass the arithmetic tests and get these silently wrong.
//!
//! Skips loudly when the Xtensa builtins image has not been built — it is a
//! gitignored cross-target artifact needing the esp toolchain.

#![cfg(feature = "emu-xt")]

use lp_collection::VecMap;
use lpir::builder::FunctionBuilder;
use lpir::lpir_module::ImportDecl;
use lpir::{FloatMode, FuncId, IrType, LpirModule, LpirOp};
use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};
use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_native::isa::IsaTarget;
use lpvm_native::native_options::NativeCompileOptions;
use lpvm_native::rt_emu::NativeEmuEngine;

/// Q16.16 one.
const Q_ONE: i32 = 65536;

/// True when the Xtensa base image is available; otherwise print the skip note
/// once and let the caller return.
fn image_available() -> bool {
    if lps_builtins_xt_image::is_available() {
        return true;
    }
    eprintln!(
        "SKIP: Xtensa builtins image not built — run {}",
        lps_builtins_xt_image::BUILD_COMMAND
    );
    false
}

fn opts() -> NativeCompileOptions {
    NativeCompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    }
}

/// Call `name(args)` on both ISAs and return `(rv32, xtensa)` results.
fn both(ir: &LpirModule, sig: &LpsModuleSig, name: &str, args: &[i32]) -> (Vec<i32>, Vec<i32>) {
    let mut out = Vec::new();
    for isa in [IsaTarget::Rv32imac, IsaTarget::Xtensa] {
        let engine = NativeEmuEngine::new_for_isa(opts(), isa);
        let module = engine
            .compile(ir, sig)
            .unwrap_or_else(|e| panic!("{isa:?} compile: {e}"));
        let mut inst = module
            .instantiate()
            .unwrap_or_else(|e| panic!("{isa:?} instantiate: {e}"));
        out.push(
            inst.call_q32(name, args)
                .unwrap_or_else(|e| panic!("{isa:?} call `{name}`: {e}")),
        );
    }
    (out.remove(0), out.remove(0))
}

/// Assert both ISAs agree, and report which side differs when they do.
fn agree(ir: &LpirModule, sig: &LpsModuleSig, name: &str, args: &[i32], what: &str) -> Vec<i32> {
    let (rv32, xt) = both(ir, sig, name, args);
    assert_eq!(
        xt, rv32,
        "{what}: Xtensa returned {xt:?}, rv32 (the oracle) returned {rv32:?}"
    );
    rv32
}

/// `f(x) = <build>` over one float parameter, in Q32.
fn unary_float_module(
    imports: Vec<ImportDecl>,
    build: impl FnOnce(&mut FunctionBuilder, lpir::VReg),
) -> (LpirModule, LpsModuleSig) {
    let mut fb = FunctionBuilder::new("f", &[IrType::F32]);
    let x = fb.add_param(IrType::F32);
    build(&mut fb, x);
    let func = fb.finish();
    let module = LpirModule {
        imports,
        functions: VecMap::from([(FuncId(0), func)]),
    };
    let sig = LpsModuleSig {
        functions: vec![LpsFnSig {
            name: "f".to_string(),
            parameters: vec![FnParam {
                name: "x".to_string(),
                ty: LpsType::Float,
                qualifier: ParamQualifier::In,
            }],
            return_type: LpsType::Float,
            kind: LpsFnKind::UserDefined,
        }],
        uniforms_type: None,
        globals_type: None,
        ..Default::default()
    };
    (module, sig)
}

fn glsl_import(name: &str, arity: usize) -> ImportDecl {
    ImportDecl {
        module_name: "glsl".to_string(),
        func_name: name.to_string(),
        param_types: vec![IrType::F32; arity],
        return_types: vec![IrType::F32],
        lpfn_glsl_params: None,
        needs_vmctx: false,
        sret: false,
    }
}

/// A shader calling `sin` — the case that proves the base-image link is real.
///
/// It resolves `__lps_sin_q32` out of the cross-compiled builtins ELF, patches
/// the address into an `l32r` literal-pool slot, and reaches it with `callx8`.
/// Nothing about that can be faked into agreeing with rv32.
#[test]
fn sin_matches_rv32_through_the_builtins_image() {
    if !image_available() {
        return;
    }
    let (ir, sig) = unary_float_module(vec![glsl_import("sin", 1)], |fb, x| {
        let callee = LpirModule::callee_ref_import(0);
        let r = fb.alloc_vreg(IrType::F32);
        fb.push_call(callee, &[x], &[r]);
        fb.push_return(&[r]);
    });
    // 0, 0.5, 1.0, 2.0 in Q16.16 — a spread rather than one point, so a
    // constant-folding or literal-pool mistake cannot pass by luck.
    for x in [0, Q_ONE / 2, Q_ONE, 2 * Q_ONE] {
        let got = agree(&ir, &sig, "f", &[x], &format!("sin({x})"));
        assert_eq!(got.len(), 1);
    }
}

/// Two different builtins in one module: each needs its own literal-pool slot,
/// so this catches a patcher that resolves only the first relocation.
#[test]
fn two_builtins_in_one_module_both_resolve() {
    if !image_available() {
        return;
    }
    let (ir, sig) = unary_float_module(
        vec![glsl_import("sin", 1), glsl_import("cos", 1)],
        |fb, x| {
            let sin = LpirModule::callee_ref_import(0);
            let cos = LpirModule::callee_ref_import(1);
            let a = fb.alloc_vreg(IrType::F32);
            let b = fb.alloc_vreg(IrType::F32);
            fb.push_call(sin, &[x], &[a]);
            fb.push_call(cos, &[x], &[b]);
            let s = fb.alloc_vreg(IrType::F32);
            fb.push(LpirOp::Fadd {
                dst: s,
                lhs: a,
                rhs: b,
            });
            fb.push_return(&[s]);
        },
    );
    for x in [0, Q_ONE / 3, Q_ONE] {
        agree(&ir, &sig, "f", &[x], &format!("sin({x}) + cos({x})"));
    }
}

/// Pure integer arithmetic, no builtins: isolates the engine plumbing (image
/// layout, argument staging, result read-back) from the base-image link, so a
/// failure here and a pass above localizes the fault.
#[test]
fn arithmetic_without_builtins_matches_rv32() {
    if !image_available() {
        return;
    }
    let mut fb = FunctionBuilder::new("f", &[IrType::I32]);
    let x = fb.add_param(IrType::I32);
    fb.push(LpirOp::IaddImm {
        dst: x,
        src: x,
        imm: 7,
    });
    fb.push_return(&[x]);
    let func = fb.finish();
    let ir = LpirModule {
        imports: vec![],
        functions: VecMap::from([(FuncId(0), func)]),
    };
    let sig = LpsModuleSig {
        functions: vec![LpsFnSig {
            name: "f".to_string(),
            parameters: vec![FnParam {
                name: "x".to_string(),
                ty: LpsType::Int,
                qualifier: ParamQualifier::In,
            }],
            return_type: LpsType::Int,
            kind: LpsFnKind::UserDefined,
        }],
        uniforms_type: None,
        globals_type: None,
        ..Default::default()
    };
    for x in [0, 1, -1, 1000, i32::MAX - 7] {
        let got = agree(&ir, &sig, "f", &[x], &format!("{x} + 7"));
        assert_eq!(got, vec![x.wrapping_add(7)]);
    }
}
