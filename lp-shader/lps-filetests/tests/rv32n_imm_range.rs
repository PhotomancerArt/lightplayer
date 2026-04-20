//! Regression tests for `lpvm-native` immediate-range handling.
//!
//! `LpirOp::IaddImm` / `IsubImm` / `ImulImm` accept an `i32` immediate, but
//! RV32 `addi` only encodes a signed 12-bit immediate. The lowering pass
//! used to forward the LPIR immediate straight into `VInst::AluRRI` /
//! `encode_addi`, which silently truncated to the low 12 bits and produced
//! "addi rd, rs, 0" for `imm == 65536` (`Q_ONE` in the texture render synth).
//!
//! These tests pin the pre-fix bug and guard against regressions: each
//! test builds a tiny LPIR function consisting of one `I*Imm` op and a
//! return, runs it through `NativeEmuEngine` in `FloatMode::Q32`, and
//! checks the result against `i32`-wrapping LPIR semantics.

use std::collections::BTreeMap;

use lpir::builder::FunctionBuilder;
use lpir::{FloatMode, FuncId, IrType, LpirModule, LpirOp};
use lps_shared::{FnParam, LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};
use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_native::{NativeCompileOptions, NativeEmuEngine};

const IMM12_MAX: i32 = 2047;
const IMM12_MIN: i32 = -2048;

#[test]
fn iadd_imm_q_one_pos_x_pattern() {
    let r = run_imm_op(
        |x| LpirOp::IaddImm {
            dst: x,
            src: x,
            imm: 65536,
        },
        1000,
    );
    assert_eq!(
        r,
        1000i32.wrapping_add(65536),
        "iadd_imm imm=65536 truncated"
    );
    let r0 = run_imm_op(
        |x| LpirOp::IaddImm {
            dst: x,
            src: x,
            imm: 65536,
        },
        0,
    );
    assert_eq!(r0, 65536, "iadd_imm imm=65536 from 0");
}

#[test]
fn iadd_imm_negative_large() {
    let r = run_imm_op(
        |x| LpirOp::IaddImm {
            dst: x,
            src: x,
            imm: -65536,
        },
        200_000,
    );
    assert_eq!(r, 200_000i32.wrapping_add(-65536));
}

#[test]
fn iadd_imm_just_over_imm12() {
    let r = run_imm_op(
        |x| LpirOp::IaddImm {
            dst: x,
            src: x,
            imm: IMM12_MAX + 1,
        },
        0,
    );
    assert_eq!(r, IMM12_MAX + 1, "iadd_imm boundary imm=2048");
}

#[test]
fn iadd_imm_just_under_neg_imm12() {
    let r = run_imm_op(
        |x| LpirOp::IaddImm {
            dst: x,
            src: x,
            imm: IMM12_MIN - 1,
        },
        0,
    );
    assert_eq!(r, IMM12_MIN - 1, "iadd_imm boundary imm=-2049");
}

#[test]
fn iadd_imm_in_range_smoke() {
    let r = run_imm_op(
        |x| LpirOp::IaddImm {
            dst: x,
            src: x,
            imm: 100,
        },
        7,
    );
    assert_eq!(r, 107);
}

#[test]
fn isub_imm_large() {
    let r = run_imm_op(
        |x| LpirOp::IsubImm {
            dst: x,
            src: x,
            imm: 70_000,
        },
        100_000,
    );
    assert_eq!(r, 30_000);
}

#[test]
fn isub_imm_just_over_imm12() {
    // The lower-side fix has to handle `-imm` overflowing imm12 too:
    // `imm == 2048` would become `addi rd, rs, -2048` (fits), but the
    // boundary case `imm == -2048` would need `addi rd, rs, 2048`
    // (does NOT fit).
    let r = run_imm_op(
        |x| LpirOp::IsubImm {
            dst: x,
            src: x,
            imm: -2048,
        },
        0,
    );
    assert_eq!(r, 2048, "isub_imm imm=-2048 (negation overflows imm12)");
}

#[test]
fn imul_imm_large() {
    // `ImulImm` has no native RV32 encoding (no `muli`), so the lowering
    // must materialize the constant and use `mul`.
    let r = run_imm_op(
        |x| LpirOp::ImulImm {
            dst: x,
            src: x,
            imm: 65536,
        },
        3,
    );
    assert_eq!(r, 3i32.wrapping_mul(65536));
}

#[test]
fn imul_imm_in_range_smoke() {
    let r = run_imm_op(
        |x| LpirOp::ImulImm {
            dst: x,
            src: x,
            imm: 5,
        },
        7,
    );
    assert_eq!(r, 35);
}

#[test]
fn ishl_imm_in_range() {
    // Shift immediates are masked to 5 bits both in the LPIR interp and
    // the RV32 encoder, so this is a smoke test that the path still works.
    let r = run_imm_op(
        |x| LpirOp::IshlImm {
            dst: x,
            src: x,
            imm: 4,
        },
        3,
    );
    assert_eq!(r, 48);
}

fn run_imm_op<F>(make_op: F, x: i32) -> i32
where
    F: FnOnce(lpir::VReg) -> LpirOp,
{
    let (ir, sig) = build_unary_imm_module(make_op);
    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    let engine = NativeEmuEngine::new(opts);
    let module = engine.compile(&ir, &sig).expect("compile should succeed");
    let mut instance = module.instantiate().expect("instantiate should succeed");
    let result = instance
        .call_q32("test_imm_op", &[x])
        .expect("call_q32 should succeed");
    assert_eq!(result.len(), 1, "one return value");
    result[0]
}

fn build_unary_imm_module<F>(make_op: F) -> (LpirModule, LpsModuleSig)
where
    F: FnOnce(lpir::VReg) -> LpirOp,
{
    let mut fb = FunctionBuilder::new("test_imm_op", &[IrType::I32]);
    let x = fb.add_param(IrType::I32);
    fb.push(make_op(x));
    fb.push_return(&[x]);
    let func = fb.finish();

    let module = LpirModule {
        imports: vec![],
        functions: BTreeMap::from([(FuncId(0), func)]),
    };

    let sig = LpsModuleSig {
        functions: vec![LpsFnSig {
            name: "test_imm_op".to_string(),
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
    };

    (module, sig)
}
