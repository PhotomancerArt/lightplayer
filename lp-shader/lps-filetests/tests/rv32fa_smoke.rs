//! Smoke test for the native RV32 backend (`rv32fa.q32`, `lpvm-native-fa`).
//!
//! This test verifies that the fastalloc native emulation backend can compile
//! and execute simple LPIR modules end-to-end.
//!
//! Note: The native backend currently does not support imports (builtin functions).
//! This test constructs LPIR directly without imports.

use lpir::{FloatMode, IrFunction, IrType, LpirModule, LpirOp, VReg, VRegRange};
use lps_shared::{FnParam, LpsFnSig, LpsModuleSig, LpsType};
use lpvm::{LpsValueF32, LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_native_fa::{NativeCompileOptions, NativeEmuEngine};

fn v(n: u32) -> VReg {
    VReg(n)
}

/// Build a simple LPIR module: returns a + b (integer).
fn build_iadd_module() -> (LpirModule, LpsModuleSig) {
    let func = IrFunction {
        name: "test_iadd".to_string(),
        is_entry: false,
        vmctx_vreg: v(0),
        param_count: 2,
        return_types: vec![IrType::I32],
        vreg_types: vec![IrType::I32, IrType::I32, IrType::I32], // v0 = vmctx, v1/v2 = params
        slots: vec![],
        body: vec![
            LpirOp::Iadd {
                dst: v(2),
                lhs: v(1), // first param (a)
                rhs: v(2), // second param (b), overwritten with result
            },
            LpirOp::Return {
                values: VRegRange { start: 0, count: 1 },
            },
        ],
        vreg_pool: vec![v(2)],
    };

    let module = LpirModule {
        imports: vec![],
        functions: vec![func],
    };

    let sig = LpsModuleSig {
        functions: vec![LpsFnSig {
            name: "test_iadd".to_string(),
            parameters: vec![
                FnParam {
                    name: "a".to_string(),
                    ty: LpsType::Int,
                    qualifier: lps_shared::ParamQualifier::In,
                },
                FnParam {
                    name: "b".to_string(),
                    ty: LpsType::Int,
                    qualifier: lps_shared::ParamQualifier::In,
                },
            ],
            return_type: LpsType::Int,
        }],
    };

    (module, sig)
}

/// Smoke test: compile and execute a simple iadd function via native backend.
#[test]
fn rv32fa_native_emulator_compiles_and_runs_iadd() {
    let (ir, sig) = build_iadd_module();

    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };

    let engine = NativeEmuEngine::new(opts);
    let module = engine.compile(&ir, &sig).expect("compile should succeed");
    let mut instance = module.instantiate().expect("instantiate should succeed");

    // Call test_iadd(5, 3)
    let result = instance
        .call("test_iadd", &[LpsValueF32::I32(5), LpsValueF32::I32(3)])
        .expect("call should succeed");

    // Verify result is 8
    match result {
        LpsValueF32::I32(v) => assert_eq!(v, 8, "5 + 3 = 8"),
        other => panic!("expected I32, got {:?}", other),
    }

    let n = instance
        .last_guest_instruction_count()
        .expect("guest instruction count after successful call");
    assert!(n > 0, "expected non-zero guest instructions");
}

/// Smoke test: compile and execute via Q32 flat call interface.
#[test]
fn rv32fa_native_emulator_call_q32_flat() {
    let (ir, sig) = build_iadd_module();

    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };

    let engine = NativeEmuEngine::new(opts);
    let module = engine.compile(&ir, &sig).expect("compile should succeed");
    let mut instance = module.instantiate().expect("instantiate should succeed");

    // Call with flat i32 args
    let results = instance
        .call_q32("test_iadd", &[5i32, 3i32])
        .expect("call_q32 should succeed");

    assert_eq!(results.len(), 1, "one return value");
    assert_eq!(results[0], 8i32, "5 + 3 = 8");

    let n = instance
        .last_guest_instruction_count()
        .expect("guest instruction count after successful call_q32");
    assert!(n > 0, "expected non-zero guest instructions");
}
