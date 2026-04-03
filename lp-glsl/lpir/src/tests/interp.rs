//! Interpreter tests: ops, edge-case numerics, control flow, memory, calls.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::interp::{ImportHandler, InterpError, Value, interpret, interpret_with_depth};
use crate::parse::parse_module;

// --- Float arithmetic ---

#[test]
fn interp_fsub() {
    let r = run_f32(
        "func @f(v1:f32, v2:f32) -> f32 {\n  v3:f32 = fsub v1, v2\n  return v3\n}\n",
        "f",
        &[Value::F32(5.0), Value::F32(2.0)],
    );
    assert!((r - 3.0).abs() < 1e-6);
}

#[test]
fn interp_fmul() {
    let r = run_f32(
        "func @f(v1:f32, v2:f32) -> f32 {\n  v3:f32 = fmul v1, v2\n  return v3\n}\n",
        "f",
        &[Value::F32(3.0), Value::F32(4.0)],
    );
    assert!((r - 12.0).abs() < 1e-6);
}

#[test]
fn interp_fdiv() {
    let r = run_f32(
        "func @f(v1:f32, v2:f32) -> f32 {\n  v3:f32 = fdiv v1, v2\n  return v3\n}\n",
        "f",
        &[Value::F32(10.0), Value::F32(4.0)],
    );
    assert!((r - 2.5).abs() < 1e-6);
}

#[test]
fn interp_fneg() {
    let r = run_f32(
        "func @f(v1:f32) -> f32 {\n  v2:f32 = fneg v1\n  return v2\n}\n",
        "f",
        &[Value::F32(3.0)],
    );
    assert!((r + 3.0).abs() < 1e-6);
}

#[test]
fn interp_fabs() {
    let ir = "func @f(v1:f32) -> f32 {\n  v2:f32 = fabs v1\n  return v2\n}\n";
    assert!((run_f32(ir, "f", &[Value::F32(3.0)]) - 3.0).abs() < 1e-6);
    assert!((run_f32(ir, "f", &[Value::F32(-3.0)]) - 3.0).abs() < 1e-6);
    assert!((run_f32(ir, "f", &[Value::F32(0.0)])).abs() < 1e-6);
    assert!(run_f32(ir, "f", &[Value::F32(f32::NAN)]).is_nan());
}

#[test]
fn interp_fsqrt() {
    let ir = "func @f(v1:f32) -> f32 {\n  v2:f32 = fsqrt v1\n  return v2\n}\n";
    assert!((run_f32(ir, "f", &[Value::F32(4.0)]) - 2.0).abs() < 1e-6);
    assert!((run_f32(ir, "f", &[Value::F32(0.0)])).abs() < 1e-6);
    assert!(run_f32(ir, "f", &[Value::F32(-1.0)]).is_nan());
}

#[test]
fn interp_fmin_fmax() {
    let ir_min = "func @f(v1:f32, v2:f32) -> f32 {\n  v3:f32 = fmin v1, v2\n  return v3\n}\n";
    let ir_max = "func @f(v1:f32, v2:f32) -> f32 {\n  v3:f32 = fmax v1, v2\n  return v3\n}\n";
    assert!((run_f32(ir_min, "f", &[Value::F32(1.0), Value::F32(5.0)]) - 1.0).abs() < 1e-6);
    assert!((run_f32(ir_max, "f", &[Value::F32(1.0), Value::F32(5.0)]) - 5.0).abs() < 1e-6);
    let nan = f32::NAN;
    assert!((run_f32(ir_min, "f", &[Value::F32(nan), Value::F32(2.0)]) - 2.0).abs() < 1e-6);
    assert!((run_f32(ir_min, "f", &[Value::F32(2.0), Value::F32(nan)]) - 2.0).abs() < 1e-6);
}

#[test]
fn interp_ffloor_fceil_ftrunc() {
    let ir_floor = "func @f(v1:f32) -> f32 {\n  v2:f32 = ffloor v1\n  return v2\n}\n";
    let ir_ceil = "func @f(v1:f32) -> f32 {\n  v2:f32 = fceil v1\n  return v2\n}\n";
    let ir_trunc = "func @f(v1:f32) -> f32 {\n  v2:f32 = ftrunc v1\n  return v2\n}\n";
    assert!((run_f32(ir_floor, "f", &[Value::F32(1.7)]) - 1.0).abs() < 1e-6);
    assert!((run_f32(ir_floor, "f", &[Value::F32(-1.2)]) - (-2.0)).abs() < 1e-6);
    assert!((run_f32(ir_ceil, "f", &[Value::F32(1.2)]) - 2.0).abs() < 1e-6);
    assert!((run_f32(ir_ceil, "f", &[Value::F32(-1.7)]) - (-1.0)).abs() < 1e-6);
    assert!((run_f32(ir_trunc, "f", &[Value::F32(1.7)]) - 1.0).abs() < 1e-6);
    assert!((run_f32(ir_trunc, "f", &[Value::F32(-1.7)]) - (-1.0)).abs() < 1e-6);
}

#[test]
fn interp_fnearest() {
    let ir = "func @f(v1:f32) -> f32 {\n  v2:f32 = fnearest v1\n  return v2\n}\n";
    assert!((run_f32(ir, "f", &[Value::F32(0.5)]) - 0.0).abs() < 1e-6);
    assert!((run_f32(ir, "f", &[Value::F32(1.5)]) - 2.0).abs() < 1e-6);
    assert!((run_f32(ir, "f", &[Value::F32(2.5)]) - 2.0).abs() < 1e-6);
}

// --- Integer arithmetic ---

#[test]
fn interp_iadd() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = iadd v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(3), Value::I32(7)],
        ),
        10
    );
}

#[test]
fn interp_isub() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = isub v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(10), Value::I32(3)],
        ),
        7
    );
}

#[test]
fn interp_imul() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = imul v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(6), Value::I32(7)],
        ),
        42
    );
}

#[test]
fn interp_idiv_s() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = idiv_s v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(7), Value::I32(2)],
        ),
        3
    );
}

#[test]
fn interp_idiv_u() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = idiv_u v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(-1), Value::I32(2)],
        ),
        (-1i32 as u32 / 2) as i32
    );
}

#[test]
fn interp_irem_s() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = irem_s v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(7), Value::I32(3)],
        ),
        1
    );
}

#[test]
fn interp_irem_u() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = irem_u v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(7), Value::I32(3)],
        ),
        1
    );
}

#[test]
fn interp_ineg() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32) -> i32 {\n  v2:i32 = ineg v1\n  return v2\n}\n",
            "f",
            &[Value::I32(5)],
        ),
        -5
    );
}

#[test]
fn interp_iadd_wrapping() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = iadd v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(i32::MAX), Value::I32(1)],
        ),
        i32::MIN
    );
}

// --- Float comparisons ---

#[test]
fn interp_feq_true() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = feq v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(1.0), Value::F32(1.0)],
        ),
        1
    );
}

#[test]
fn interp_feq_false() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = feq v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(1.0), Value::F32(2.0)],
        ),
        0
    );
}

#[test]
fn interp_fne_true() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = fne v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(1.0), Value::F32(2.0)],
        ),
        1
    );
}

#[test]
fn interp_flt() {
    let ir = "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = flt v1, v2\n  return v3\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::F32(1.0), Value::F32(2.0)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::F32(2.0), Value::F32(1.0)]), 0);
}

#[test]
fn interp_fle() {
    let ir = "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = fle v1, v2\n  return v3\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::F32(1.0), Value::F32(1.0)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::F32(2.0), Value::F32(1.0)]), 0);
}

#[test]
fn interp_fgt() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = fgt v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(2.0), Value::F32(1.0)],
        ),
        1
    );
}

#[test]
fn interp_fge() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = fge v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(2.0), Value::F32(2.0)],
        ),
        1
    );
}

// --- Integer comparisons (signed) ---

#[test]
fn interp_ieq() {
    let ir = "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ieq v1, v2\n  return v3\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::I32(5), Value::I32(5)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::I32(5), Value::I32(6)]), 0);
}

#[test]
fn interp_ine() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ine v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(5), Value::I32(6)],
        ),
        1
    );
}

#[test]
fn interp_ilt_s() {
    let ir = "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ilt_s v1, v2\n  return v3\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::I32(-1), Value::I32(1)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::I32(1), Value::I32(-1)]), 0);
}

#[test]
fn interp_ile_s() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ile_s v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(-1), Value::I32(-1)],
        ),
        1
    );
}

#[test]
fn interp_igt_s() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = igt_s v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(1), Value::I32(-1)],
        ),
        1
    );
}

#[test]
fn interp_ige_s() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ige_s v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(0), Value::I32(0)],
        ),
        1
    );
}

// --- Integer comparisons (unsigned) ---

#[test]
fn interp_ilt_u() {
    let ir = "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ilt_u v1, v2\n  return v3\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::I32(1), Value::I32(2)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::I32(-1), Value::I32(1)]), 0);
}

#[test]
fn interp_ile_u() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ile_u v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(2), Value::I32(2)],
        ),
        1
    );
}

#[test]
fn interp_igt_u() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = igt_u v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(-1), Value::I32(1)],
        ),
        1
    );
}

#[test]
fn interp_ige_u() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ige_u v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(0), Value::I32(0)],
        ),
        1
    );
}

// --- Logic / bitwise ---

#[test]
fn interp_iand() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = iand v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(0xFF), Value::I32(0x0F)],
        ),
        0x0F
    );
}

#[test]
fn interp_ior() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ior v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(0xF0), Value::I32(0x0F)],
        ),
        0xFF
    );
}

#[test]
fn interp_ixor() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ixor v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(0xFF), Value::I32(0x0F)],
        ),
        0xF0
    );
}

#[test]
fn interp_ibnot() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32) -> i32 {\n  v2:i32 = ibnot v1\n  return v2\n}\n",
            "f",
            &[Value::I32(0)],
        ),
        -1
    );
}

#[test]
fn interp_ishl() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ishl v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(1), Value::I32(4)],
        ),
        16
    );
}

#[test]
fn interp_ishr_s() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ishr_s v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(-16), Value::I32(2)],
        ),
        -4
    );
}

#[test]
fn interp_ishr_u() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ishr_u v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(-1), Value::I32(28)],
        ),
        0xF
    );
}

// --- Constants ---

#[test]
fn interp_fconst() {
    let r = run_f32(
        "func @f() -> f32 {\n  v1:f32 = fconst.f32 3.14\n  return v1\n}\n",
        "f",
        &[],
    );
    assert!((r - 3.14).abs() < 1e-5);
}

#[test]
fn interp_iconst() {
    assert_eq!(
        run_i32(
            "func @f() -> i32 {\n  v1:i32 = iconst.i32 42\n  return v1\n}\n",
            "f",
            &[],
        ),
        42
    );
}

#[test]
fn interp_iconst_neg() {
    assert_eq!(
        run_i32(
            "func @f() -> i32 {\n  v1:i32 = iconst.i32 -7\n  return v1\n}\n",
            "f",
            &[],
        ),
        -7
    );
}

// --- Immediate variants ---

#[test]
fn interp_iadd_imm() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32) -> i32 {\n  v2:i32 = iadd_imm v1, 10\n  return v2\n}\n",
            "f",
            &[Value::I32(5)],
        ),
        15
    );
}

#[test]
fn interp_isub_imm() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32) -> i32 {\n  v2:i32 = isub_imm v1, 3\n  return v2\n}\n",
            "f",
            &[Value::I32(10)],
        ),
        7
    );
}

#[test]
fn interp_imul_imm() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32) -> i32 {\n  v2:i32 = imul_imm v1, 4\n  return v2\n}\n",
            "f",
            &[Value::I32(3)],
        ),
        12
    );
}

#[test]
fn interp_ishl_imm() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32) -> i32 {\n  v2:i32 = ishl_imm v1, 2\n  return v2\n}\n",
            "f",
            &[Value::I32(3)],
        ),
        12
    );
}

#[test]
fn interp_ishr_s_imm() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32) -> i32 {\n  v2:i32 = ishr_s_imm v1, 1\n  return v2\n}\n",
            "f",
            &[Value::I32(-4)],
        ),
        -2
    );
}

#[test]
fn interp_ishr_u_imm() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32) -> i32 {\n  v2:i32 = ishr_u_imm v1, 1\n  return v2\n}\n",
            "f",
            &[Value::I32(-2)],
        ),
        0x7FFF_FFFF
    );
}

#[test]
fn interp_ieq_imm() {
    let ir = "func @f(v1:i32) -> i32 {\n  v2:i32 = ieq_imm v1, 42\n  return v2\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::I32(42)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::I32(0)]), 0);
}

// --- Casts ---

#[test]
fn interp_ftoi_sat_s_normal() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32) -> i32 {\n  v2:i32 = ftoi_sat_s v1\n  return v2\n}\n",
            "f",
            &[Value::F32(3.7)],
        ),
        3
    );
}

#[test]
fn interp_ftoi_sat_u_normal() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32) -> i32 {\n  v2:i32 = ftoi_sat_u v1\n  return v2\n}\n",
            "f",
            &[Value::F32(3.7)],
        ),
        3
    );
}

#[test]
fn interp_itof_s() {
    let r = run_f32(
        "func @f(v1:i32) -> f32 {\n  v2:f32 = itof_s v1\n  return v2\n}\n",
        "f",
        &[Value::I32(-1)],
    );
    assert!((r + 1.0).abs() < 1e-5);
}

#[test]
fn interp_itof_u() {
    let r = run_f32(
        "func @f(v1:i32) -> f32 {\n  v2:f32 = itof_u v1\n  return v2\n}\n",
        "f",
        &[Value::I32(-1)],
    );
    assert!((r - 4294967296.0).abs() < 65536.0);
}

// --- Select / copy ---

#[test]
fn interp_select_true() {
    let r = run_f32(
        "func @f(v1:i32, v2:f32, v3:f32) -> f32 {\n  v4:f32 = select v1, v2, v3\n  return v4\n}\n",
        "f",
        &[Value::I32(1), Value::F32(10.0), Value::F32(20.0)],
    );
    assert!((r - 10.0).abs() < 1e-6);
}

#[test]
fn interp_select_false() {
    let r = run_f32(
        "func @f(v1:i32, v2:f32, v3:f32) -> f32 {\n  v4:f32 = select v1, v2, v3\n  return v4\n}\n",
        "f",
        &[Value::I32(0), Value::F32(10.0), Value::F32(20.0)],
    );
    assert!((r - 20.0).abs() < 1e-6);
}

#[test]
fn interp_copy() {
    let r = run_f32(
        "func @f(v1:f32) -> f32 {\n  v2:f32 = copy v1\n  return v2\n}\n",
        "f",
        &[Value::F32(2.5)],
    );
    assert!((r - 2.5).abs() < 1e-6);
}

// --- Edge: division by zero ---

#[test]
fn interp_idiv_s_by_zero() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = idiv_s v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(42), Value::I32(0)],
        ),
        0
    );
}

#[test]
fn interp_idiv_u_by_zero() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = idiv_u v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(42), Value::I32(0)],
        ),
        0
    );
}

#[test]
fn interp_irem_s_by_zero() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = irem_s v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(42), Value::I32(0)],
        ),
        0
    );
}

#[test]
fn interp_irem_u_by_zero() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = irem_u v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(42), Value::I32(0)],
        ),
        0
    );
}

#[test]
fn interp_idiv_s_min_neg1() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = idiv_s v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(i32::MIN), Value::I32(-1)],
        ),
        i32::MIN.wrapping_div(-1)
    );
}

// --- NaN ---

#[test]
fn interp_feq_nan() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = feq v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(f32::NAN), Value::F32(f32::NAN)],
        ),
        0
    );
}

#[test]
fn interp_feq_nan_other() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = feq v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(f32::NAN), Value::F32(1.0)],
        ),
        0
    );
}

#[test]
fn interp_fne_nan() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = fne v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(f32::NAN), Value::F32(f32::NAN)],
        ),
        1
    );
}

#[test]
fn interp_fne_nan_other() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = fne v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(f32::NAN), Value::F32(1.0)],
        ),
        1
    );
}

#[test]
fn interp_flt_nan() {
    let ir = "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = flt v1, v2\n  return v3\n}\n";
    assert_eq!(
        run_i32(ir, "f", &[Value::F32(f32::NAN), Value::F32(1.0)]),
        0
    );
    assert_eq!(
        run_i32(ir, "f", &[Value::F32(1.0), Value::F32(f32::NAN)]),
        0
    );
}

#[test]
fn interp_fle_nan() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = fle v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(f32::NAN), Value::F32(1.0)],
        ),
        0
    );
}

#[test]
fn interp_fgt_nan() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = fgt v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(f32::NAN), Value::F32(1.0)],
        ),
        0
    );
}

#[test]
fn interp_fge_nan() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32, v2:f32) -> i32 {\n  v3:i32 = fge v1, v2\n  return v3\n}\n",
            "f",
            &[Value::F32(f32::NAN), Value::F32(1.0)],
        ),
        0
    );
}

#[test]
fn interp_fadd_nan() {
    let r = run_f32(
        "func @f(v1:f32, v2:f32) -> f32 {\n  v3:f32 = fadd v1, v2\n  return v3\n}\n",
        "f",
        &[Value::F32(f32::NAN), Value::F32(1.0)],
    );
    assert!(r.is_nan());
}

#[test]
fn interp_fmul_nan() {
    let r = run_f32(
        "func @f(v1:f32, v2:f32) -> f32 {\n  v3:f32 = fmul v1, v2\n  return v3\n}\n",
        "f",
        &[Value::F32(f32::NAN), Value::F32(1.0)],
    );
    assert!(r.is_nan());
}

// --- Float division ---

#[test]
fn interp_fdiv_by_zero_pos() {
    let r = run_f32(
        "func @f(v1:f32, v2:f32) -> f32 {\n  v3:f32 = fdiv v1, v2\n  return v3\n}\n",
        "f",
        &[Value::F32(1.0), Value::F32(0.0)],
    );
    assert!(r.is_infinite() && r.is_sign_positive());
}

#[test]
fn interp_fdiv_by_zero_neg() {
    let r = run_f32(
        "func @f(v1:f32, v2:f32) -> f32 {\n  v3:f32 = fdiv v1, v2\n  return v3\n}\n",
        "f",
        &[Value::F32(-1.0), Value::F32(0.0)],
    );
    assert!(r.is_infinite() && r.is_sign_negative());
}

#[test]
fn interp_fdiv_zero_by_zero() {
    let r = run_f32(
        "func @f(v1:f32, v2:f32) -> f32 {\n  v3:f32 = fdiv v1, v2\n  return v3\n}\n",
        "f",
        &[Value::F32(0.0), Value::F32(0.0)],
    );
    assert!(r.is_nan());
}

// --- Saturating casts ---

#[test]
fn interp_ftoi_sat_s_neg() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32) -> i32 {\n  v2:i32 = ftoi_sat_s v1\n  return v2\n}\n",
            "f",
            &[Value::F32(-3.7)],
        ),
        -3
    );
}

#[test]
fn interp_ftoi_sat_s_overflow_pos() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32) -> i32 {\n  v2:i32 = ftoi_sat_s v1\n  return v2\n}\n",
            "f",
            &[Value::F32(1e15)],
        ),
        i32::MAX
    );
}

#[test]
fn interp_ftoi_sat_s_overflow_neg() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32) -> i32 {\n  v2:i32 = ftoi_sat_s v1\n  return v2\n}\n",
            "f",
            &[Value::F32(-1e15)],
        ),
        i32::MIN
    );
}

#[test]
fn interp_ftoi_sat_s_nan() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32) -> i32 {\n  v2:i32 = ftoi_sat_s v1\n  return v2\n}\n",
            "f",
            &[Value::F32(f32::NAN)],
        ),
        0
    );
}

#[test]
fn interp_ftoi_sat_s_inf() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32) -> i32 {\n  v2:i32 = ftoi_sat_s v1\n  return v2\n}\n",
            "f",
            &[Value::F32(f32::INFINITY)],
        ),
        i32::MAX
    );
}

#[test]
fn interp_ftoi_sat_s_neg_inf() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32) -> i32 {\n  v2:i32 = ftoi_sat_s v1\n  return v2\n}\n",
            "f",
            &[Value::F32(f32::NEG_INFINITY)],
        ),
        i32::MIN
    );
}

#[test]
fn interp_ftoi_sat_u_negative() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32) -> i32 {\n  v2:i32 = ftoi_sat_u v1\n  return v2\n}\n",
            "f",
            &[Value::F32(-1.0)],
        ),
        0
    );
}

#[test]
fn interp_ftoi_sat_u_overflow() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32) -> i32 {\n  v2:i32 = ftoi_sat_u v1\n  return v2\n}\n",
            "f",
            &[Value::F32(1e15)],
        ),
        -1
    );
}

#[test]
fn interp_ftoi_sat_u_nan() {
    assert_eq!(
        run_i32(
            "func @f(v1:f32) -> i32 {\n  v2:i32 = ftoi_sat_u v1\n  return v2\n}\n",
            "f",
            &[Value::F32(f32::NAN)],
        ),
        0
    );
}

// --- Shift masking ---

#[test]
fn interp_ishl_mask() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ishl v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(1), Value::I32(32)],
        ),
        1
    );
}

#[test]
fn interp_ishl_mask_33() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ishl v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(1), Value::I32(33)],
        ),
        2
    );
}

#[test]
fn interp_ishr_s_mask() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ishr_s v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(-1), Value::I32(32)],
        ),
        -1
    );
}

#[test]
fn interp_ishr_u_mask() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = ishr_u v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(i32::MIN), Value::I32(32)],
        ),
        i32::MIN
    );
}

#[test]
fn interp_imul_wrapping() {
    assert_eq!(
        run_i32(
            "func @f(v1:i32, v2:i32) -> i32 {\n  v3:i32 = imul v1, v2\n  return v3\n}\n",
            "f",
            &[Value::I32(100_000), Value::I32(100_000)],
        ),
        100_000i32.wrapping_mul(100_000)
    );
}

// --- Control flow: if / else ---

#[test]
fn interp_if_true_branch() {
    let ir = "func @f(v1:i32) -> i32 {
  v2:i32 = iconst.i32 10
  v3:i32 = iconst.i32 20
  if v1 {
    v2 = iconst.i32 99
  }
  return v2
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(1)]), 99);
}

#[test]
fn interp_if_false_branch() {
    let ir = "func @f(v1:i32) -> i32 {
  v2:i32 = iconst.i32 10
  v3:i32 = iconst.i32 20
  if v1 {
    v2 = iconst.i32 99
  }
  return v2
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(0)]), 10);
}

#[test]
fn interp_if_else_true() {
    let ir = "func @f(v1:i32) -> i32 {
  v2:i32 = iconst.i32 1
  v3:i32 = iconst.i32 2
  if v1 {
    return v2
  } else {
    return v3
  }
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(1)]), 1);
}

#[test]
fn interp_if_else_false() {
    let ir = "func @f(v1:i32) -> i32 {
  v2:i32 = iconst.i32 1
  v3:i32 = iconst.i32 2
  if v1 {
    return v2
  } else {
    return v3
  }
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(0)]), 2);
}

#[test]
fn interp_if_else_return_max() {
    let ir = "func @max(v1:f32, v2:f32) -> f32 {
  v3:i32 = fgt v1, v2
  if v3 {
    return v1
  } else {
    return v2
  }
}
";
    assert!((run_f32(ir, "max", &[Value::F32(3.0), Value::F32(2.0)]) - 3.0).abs() < 1e-6);
    assert!((run_f32(ir, "max", &[Value::F32(1.0), Value::F32(4.0)]) - 4.0).abs() < 1e-6);
}

// --- Loops ---

#[test]
fn interp_loop_sum_to_n() {
    let ir = "func @sum_to_n(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  v3:i32 = iconst.i32 0
  loop {
    v4:i32 = ilt_s v3, v1
    br_if_not v4
    v2 = iadd v2, v3
    v5:i32 = iconst.i32 1
    v3 = iadd v3, v5
    continue
  }
  return v2
}
";
    assert_eq!(run_i32(ir, "sum_to_n", &[Value::I32(10)]), 45);
}

#[test]
fn interp_loop_break() {
    let ir = "func @f() -> i32 {
  v1:i32 = iconst.i32 7
  loop {
    break
    v1 = iconst.i32 99
  }
  return v1
}
";
    assert_eq!(run_i32(ir, "f", &[]), 7);
}

#[test]
fn interp_loop_continue() {
    let ir = "func @f() -> i32 {
  v1:i32 = iconst.i32 0
  v2:i32 = iconst.i32 0
  v3:i32 = iconst.i32 3
  loop {
    v4:i32 = ilt_s v2, v3
    br_if_not v4
    v2 = iadd_imm v2, 1
    v5:i32 = ieq_imm v2, 2
    if v5 {
      continue
    }
    v1 = iadd_imm v1, 1
    continue
  }
  return v1
}
";
    assert_eq!(run_i32(ir, "f", &[]), 2);
}

#[test]
fn interp_nested_loops() {
    let ir = "func @nested(v1:i32, v2:i32) -> i32 {
  v3:i32 = iconst.i32 0
  v4:i32 = iconst.i32 0
  loop {
    v5:i32 = ilt_s v4, v1
    br_if_not v5
    v6:i32 = iconst.i32 0
    loop {
      v7:i32 = ilt_s v6, v2
      br_if_not v7
      v3 = iadd v3, v6
      v8:i32 = iconst.i32 1
      v6 = iadd v6, v8
      continue
    }
    v9:i32 = iconst.i32 1
    v4 = iadd v4, v9
    continue
  }
  return v3
}
";
    assert_eq!(run_i32(ir, "nested", &[Value::I32(3), Value::I32(4)]), 18);
}

#[test]
fn interp_br_if_not_exits() {
    let ir = "func @f() -> i32 {
  v1:i32 = iconst.i32 0
  loop {
    v2:i32 = iconst.i32 0
    br_if_not v2
    v1 = iconst.i32 1
    break
  }
  return v1
}
";
    assert_eq!(run_i32(ir, "f", &[]), 0);
}

#[test]
fn interp_br_if_not_continues() {
    let ir = "func @f() -> i32 {
  v1:i32 = iconst.i32 0
  loop {
    v2:i32 = iconst.i32 1
    br_if_not v2
    return v1
    break
  }
  return v1
}
";
    assert_eq!(run_i32(ir, "f", &[]), 0);
}

// --- Switch ---

#[test]
fn interp_switch_case_match() {
    let ir = "func @dispatch(v1:i32) -> f32 {
  v2:f32 = fconst.f32 0.0
  switch v1 {
    case 0 {
      v2 = fconst.f32 1.0
    }
    case 1 {
      v2 = fconst.f32 2.0
    }
    case 2 {
      v2 = fconst.f32 4.0
    }
    default {
      v2 = fconst.f32 -1.0
    }
  }
  return v2
}
";
    assert!((run_f32(ir, "dispatch", &[Value::I32(1)]) - 2.0).abs() < 1e-6);
}

#[test]
fn interp_switch_default() {
    let ir = "func @dispatch(v1:i32) -> f32 {
  v2:f32 = fconst.f32 0.0
  switch v1 {
    case 0 {
      v2 = fconst.f32 1.0
    }
    case 1 {
      v2 = fconst.f32 2.0
    }
    case 2 {
      v2 = fconst.f32 4.0
    }
    default {
      v2 = fconst.f32 -1.0
    }
  }
  return v2
}
";
    assert!((run_f32(ir, "dispatch", &[Value::I32(99)]) + 1.0).abs() < 1e-6);
}

#[test]
fn interp_switch_no_default() {
    let ir = "func @sw(v1:i32) -> i32 {
  v2:i32 = iconst.i32 42
  switch v1 {
    case 0 {
      v2 = iconst.i32 1
    }
  }
  return v2
}
";
    assert_eq!(run_i32(ir, "sw", &[Value::I32(5)]), 42);
}

// --- Early return ---

#[test]
fn interp_early_return() {
    let ir = "func @early_return(v1:f32) -> f32 {
  v2:f32 = fconst.f32 0.0
  v3:i32 = flt v1, v2
  if v3 {
    v4:f32 = fneg v1
    return v4
  }
  return v1
}
";
    assert!((run_f32(ir, "early_return", &[Value::F32(-2.0)]) - 2.0).abs() < 1e-6);
    assert!((run_f32(ir, "early_return", &[Value::F32(3.0)]) - 3.0).abs() < 1e-6);
}

// --- Memory ---

#[test]
fn interp_slot_store_load() {
    let ir = "func @f(v1:f32) -> f32 {
  slot ss0, 4
  v2:i32 = slot_addr ss0
  store v2, 0, v1
  v3:f32 = load v2, 0
  return v3
}
";
    let r = run_f32(ir, "f", &[Value::F32(42.5)]);
    assert!((r - 42.5).abs() < 1e-6);
}

#[test]
fn interp_slot_store_load_i32() {
    let ir = "func @f(v1:i32) -> i32 {
  slot ss0, 4
  v2:i32 = slot_addr ss0
  store v2, 0, v1
  v3:i32 = load v2, 0
  return v3
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(-77)]), -77);
}

#[test]
fn interp_slot_offset() {
    let ir = "func @f() -> f32 {
  slot ss0, 8
  v1:i32 = slot_addr ss0
  v2:f32 = fconst.f32 1.0
  v3:f32 = fconst.f32 2.0
  store v1, 0, v2
  store v1, 4, v3
  v4:f32 = load v1, 4
  return v4
}
";
    assert!((run_f32(ir, "f", &[]) - 2.0).abs() < 1e-6);
}

#[test]
fn interp_memcpy_slots() {
    let ir = "func @f() -> i32 {
  slot ss0, 4
  slot ss1, 4
  v1:i32 = slot_addr ss0
  v2:i32 = slot_addr ss1
  v3:i32 = iconst.i32 123
  store v1, 0, v3
  memcpy v2, v1, 4
  v4:i32 = load v2, 0
  return v4
}
";
    assert_eq!(run_i32(ir, "f", &[]), 123);
}

#[test]
fn interp_dynamic_index() {
    let ir = "func @arr_dyn(v1:i32) -> f32 {
  slot ss0, 16
  v2:i32 = slot_addr ss0
  v3:f32 = fconst.f32 1.0
  store v2, 0, v3
  store v2, 4, v3
  store v2, 8, v3
  store v2, 12, v3
  v4:i32 = imul_imm v1, 4
  v5:i32 = iadd v2, v4
  v6:f32 = load v5, 0
  return v6
}
";
    assert!((run_f32(ir, "arr_dyn", &[Value::I32(2)]) - 1.0).abs() < 1e-6);
}

// --- Calls ---

#[test]
fn interp_local_call() {
    let ir = "func @helper(v1:i32) -> i32 {
  v2:i32 = iadd_imm v1, 1
  return v2
}
func @main(v1:i32) -> i32 {
  v2:i32 = call @helper(v0, v1)
  return v2
}
";
    assert_eq!(run_i32(ir, "main", &[Value::I32(41)]), 42);
}

#[test]
fn interp_local_call_multi_return() {
    let ir = "func @pair() -> (f32, f32) {
  v1:f32 = fconst.f32 1.0
  v2:f32 = fconst.f32 2.0
  return v1, v2
}
func @main() -> f32 {
  v1:f32, v2:f32 = call @pair(v0)
  v3:f32 = fadd v1, v2
  return v3
}
";
    assert!((run_f32(ir, "main", &[]) - 3.0).abs() < 1e-6);
}

#[test]
fn interp_import_call_unary() {
    let ir = "import @glsl::fabs(f32) -> f32

func @f(v1:f32) -> f32 {
  v2:f32 = call @glsl::fabs(v1)
  return v2
}
";
    let r = run_f32_with_imports(ir, "f", &[Value::F32(-3.0)], &mut MockMathImports);
    assert!((r - 3.0).abs() < 1e-6);
}

#[test]
fn interp_import_call_binary() {
    let ir = "import @glsl::fmax(f32, f32) -> f32

func @f(v1:f32, v2:f32) -> f32 {
  v3:f32 = call @glsl::fmax(v1, v2)
  return v3
}
";
    let r = run_f32_with_imports(
        ir,
        "f",
        &[Value::F32(1.0), Value::F32(5.0)],
        &mut MockMathImports,
    );
    assert!((r - 5.0).abs() < 1e-6);
}

#[test]
fn interp_import_error() {
    let ir = "import @glsl::fabs(f32) -> f32
import @glsl::unknown(f32) -> f32

func @f(v1:f32) -> f32 {
  v2:f32 = call @glsl::unknown(v1)
  return v2
}
";
    let m = parse_module(ir).unwrap();
    let err = interpret(&m, "f", &[Value::F32(1.0)], &mut MockMathImports).unwrap_err();
    assert!(matches!(err, InterpError::Import(_)));
}

#[test]
fn interp_factorial() {
    let ir = "func @fact(v1:i32) -> i32 {
  v2:i32 = ieq_imm v1, 0
  if v2 {
    v3:i32 = iconst.i32 1
    return v3
  }
  v4:i32 = iconst.i32 1
  v5:i32 = isub v1, v4
  v6:i32 = call @fact(v0, v5)
  v7:i32 = imul v1, v6
  return v7
}
";
    assert_eq!(run_i32(ir, "fact", &[Value::I32(5)]), 120);
}

#[test]
fn interp_stack_overflow() {
    let ir = "func @inf(v1:i32) -> i32 {
  v2:i32 = call @inf(v0, v1)
  return v2
}
";
    let m = parse_module(ir).unwrap();
    let err = interpret_with_depth(&m, "inf", &[Value::I32(0)], &mut NoImports, 4).unwrap_err();
    assert!(matches!(err, InterpError::StackOverflow));
}

// --- Error paths ---

#[test]
fn interp_err_function_not_found() {
    let ir = "func @f() -> i32 {
  v1:i32 = iconst.i32 0
  return v1
}
";
    let m = parse_module(ir).unwrap();
    let err = interpret(&m, "nope", &[], &mut NoImports).unwrap_err();
    assert!(matches!(err, InterpError::FunctionNotFound(_)));
}

#[test]
fn interp_err_arg_arity() {
    let ir = "func @f() -> i32 {
  v1:i32 = iconst.i32 0
  return v1
}
";
    let m = parse_module(ir).unwrap();
    let err = interpret(&m, "f", &[Value::I32(1)], &mut NoImports).unwrap_err();
    assert!(matches!(err, InterpError::Internal(_)));
}

#[test]
fn interp_add() {
    let ir = "func @add(v1:f32, v2:f32) -> f32 {
  v3:f32 = fadd v1, v2
  return v3
}
";
    let m = parse_module(ir).unwrap();
    let mut imp = NoImports;
    let out = interpret(&m, "add", &[Value::F32(3.0), Value::F32(0.5)], &mut imp).unwrap();
    assert_eq!(out.len(), 1);
    assert!((out[0].as_f32().unwrap() - 3.5).abs() < 1e-5);
}

#[test]
fn interp_error_display() {
    let e = InterpError::FunctionNotFound(String::from("nope"));
    assert!(e.to_string().contains("nope"));
}

#[test]
fn interp_loop_continuing_for_sum() {
    let ir = "
func @for_sum(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  v3:i32 = iconst.i32 0
  loop {
    v4:i32 = ige_s v3, v1
    if v4 {
      break
    }
    v2 = iadd v2, v3
    continuing:
    v3 = iadd_imm v3, 1
  }
  return v2
}
";
    assert_eq!(run_i32(ir, "for_sum", &[Value::I32(5)]), 10);
    assert_eq!(run_i32(ir, "for_sum", &[Value::I32(0)]), 0);
    assert_eq!(run_i32(ir, "for_sum", &[Value::I32(1)]), 0);
}

#[test]
fn interp_loop_continuing_break_if() {
    let ir = "func @f(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  v3:i32 = iconst.i32 0
  loop {
    v2 = iadd v2, v3
    continuing:
    v3 = iadd_imm v3, 1
    v4:i32 = ilt_s v3, v1
    br_if_not v4
  }
  return v2
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(5)]), 10);
}

#[test]
fn interp_loop_continuing_continue_in_body() {
    let ir = "func @f(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  v3:i32 = iconst.i32 0
  loop {
    v4:i32 = ige_s v3, v1
    if v4 {
      break
    }
    v5:i32 = ieq_imm v3, 2
    if v5 {
      continue
    }
    v2 = iadd v2, v3
    continuing:
    v3 = iadd_imm v3, 1
  }
  return v2
}
";
    // sum 0..5 skipping i=2: 0+1+3+4 = 8
    assert_eq!(run_i32(ir, "f", &[Value::I32(5)]), 8);
}

#[test]
fn interp_loop_continuing_nested() {
    let ir = "func @f(v1:i32, v2:i32) -> i32 {
  v3:i32 = iconst.i32 0
  v4:i32 = iconst.i32 0
  loop {
    v5:i32 = ige_s v4, v1
    if v5 {
      break
    }
    v6:i32 = iconst.i32 0
    loop {
      v7:i32 = ige_s v6, v2
      if v7 {
        break
      }
      v3 = iadd v3, v6
      continuing:
      v6 = iadd_imm v6, 1
    }
    continuing:
    v4 = iadd_imm v4, 1
  }
  return v3
}
";
    // 3 outer * (0+1+2+3) inner = 3*6 = 18
    assert_eq!(run_i32(ir, "f", &[Value::I32(3), Value::I32(4)]), 18);
}

// --- Helpers (bottom of module) ---

struct NoImports;

impl ImportHandler for NoImports {
    fn call(&mut self, _: &str, _: &str, _: &[Value]) -> Result<Vec<Value>, InterpError> {
        Err(InterpError::Import(String::from("no imports")))
    }
}

struct MockMathImports;

impl ImportHandler for MockMathImports {
    fn call(
        &mut self,
        module_name: &str,
        func_name: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, InterpError> {
        match (module_name, func_name) {
            ("glsl", "fabs") => {
                let v = args[0].as_f32().unwrap();
                Ok(vec![Value::F32(v.abs())])
            }
            ("glsl", "fmax") => {
                let a = args[0].as_f32().unwrap();
                let b = args[1].as_f32().unwrap();
                Ok(vec![Value::F32(a.max(b))])
            }
            ("glsl", "unknown") => Err(InterpError::Import(String::from("unknown"))),
            _ => Err(InterpError::Import(format!(
                "unknown {module_name}::{func_name}"
            ))),
        }
    }
}

fn run(ir: &str, func: &str, args: &[Value]) -> Vec<Value> {
    let module = parse_module(ir).unwrap_or_else(|e| panic!("parse: {e:?}"));
    interpret(&module, func, args, &mut NoImports).unwrap()
}

fn run_f32(ir: &str, func: &str, args: &[Value]) -> f32 {
    let out = run(ir, func, args);
    assert_eq!(out.len(), 1, "expected 1 return value, got {}", out.len());
    out[0].as_f32().expect("expected f32")
}

fn run_i32(ir: &str, func: &str, args: &[Value]) -> i32 {
    let out = run(ir, func, args);
    assert_eq!(out.len(), 1, "expected 1 return value, got {}", out.len());
    out[0].as_i32().expect("expected i32")
}

fn run_f32_with_imports(
    ir: &str,
    func: &str,
    args: &[Value],
    imports: &mut MockMathImports,
) -> f32 {
    let module = parse_module(ir).unwrap_or_else(|e| panic!("parse: {e:?}"));
    let out = interpret(&module, func, args, imports).unwrap();
    assert_eq!(out.len(), 1);
    out[0].as_f32().expect("expected f32")
}
