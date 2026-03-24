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
        "func @f(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fsub v0, v1\n  return v2\n}\n",
        "f",
        &[Value::F32(5.0), Value::F32(2.0)],
    );
    assert!((r - 3.0).abs() < 1e-6);
}

#[test]
fn interp_fmul() {
    let r = run_f32(
        "func @f(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fmul v0, v1\n  return v2\n}\n",
        "f",
        &[Value::F32(3.0), Value::F32(4.0)],
    );
    assert!((r - 12.0).abs() < 1e-6);
}

#[test]
fn interp_fdiv() {
    let r = run_f32(
        "func @f(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fdiv v0, v1\n  return v2\n}\n",
        "f",
        &[Value::F32(10.0), Value::F32(4.0)],
    );
    assert!((r - 2.5).abs() < 1e-6);
}

#[test]
fn interp_fneg() {
    let r = run_f32(
        "func @f(v0:f32) -> f32 {\n  v1:f32 = fneg v0\n  return v1\n}\n",
        "f",
        &[Value::F32(3.0)],
    );
    assert!((r + 3.0).abs() < 1e-6);
}

// --- Integer arithmetic ---

#[test]
fn interp_iadd() {
    assert_eq!(
        run_i32(
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = iadd v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = isub v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = imul v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = idiv_s v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = idiv_u v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = irem_s v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = irem_u v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32) -> i32 {\n  v1:i32 = ineg v0\n  return v1\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = iadd v0, v1\n  return v2\n}\n",
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
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = feq v0, v1\n  return v2\n}\n",
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
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = feq v0, v1\n  return v2\n}\n",
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
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = fne v0, v1\n  return v2\n}\n",
            "f",
            &[Value::F32(1.0), Value::F32(2.0)],
        ),
        1
    );
}

#[test]
fn interp_flt() {
    let ir = "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = flt v0, v1\n  return v2\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::F32(1.0), Value::F32(2.0)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::F32(2.0), Value::F32(1.0)]), 0);
}

#[test]
fn interp_fle() {
    let ir = "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = fle v0, v1\n  return v2\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::F32(1.0), Value::F32(1.0)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::F32(2.0), Value::F32(1.0)]), 0);
}

#[test]
fn interp_fgt() {
    assert_eq!(
        run_i32(
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = fgt v0, v1\n  return v2\n}\n",
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
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = fge v0, v1\n  return v2\n}\n",
            "f",
            &[Value::F32(2.0), Value::F32(2.0)],
        ),
        1
    );
}

// --- Integer comparisons (signed) ---

#[test]
fn interp_ieq() {
    let ir = "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ieq v0, v1\n  return v2\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::I32(5), Value::I32(5)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::I32(5), Value::I32(6)]), 0);
}

#[test]
fn interp_ine() {
    assert_eq!(
        run_i32(
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ine v0, v1\n  return v2\n}\n",
            "f",
            &[Value::I32(5), Value::I32(6)],
        ),
        1
    );
}

#[test]
fn interp_ilt_s() {
    let ir = "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ilt_s v0, v1\n  return v2\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::I32(-1), Value::I32(1)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::I32(1), Value::I32(-1)]), 0);
}

#[test]
fn interp_ile_s() {
    assert_eq!(
        run_i32(
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ile_s v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = igt_s v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ige_s v0, v1\n  return v2\n}\n",
            "f",
            &[Value::I32(0), Value::I32(0)],
        ),
        1
    );
}

// --- Integer comparisons (unsigned) ---

#[test]
fn interp_ilt_u() {
    let ir = "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ilt_u v0, v1\n  return v2\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::I32(1), Value::I32(2)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::I32(-1), Value::I32(1)]), 0);
}

#[test]
fn interp_ile_u() {
    assert_eq!(
        run_i32(
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ile_u v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = igt_u v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ige_u v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = iand v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ior v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ixor v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32) -> i32 {\n  v1:i32 = ibnot v0\n  return v1\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ishl v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ishr_s v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ishr_u v0, v1\n  return v2\n}\n",
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
        "func @f() -> f32 {\n  v0:f32 = fconst.f32 3.14\n  return v0\n}\n",
        "f",
        &[],
    );
    assert!((r - 3.14).abs() < 1e-5);
}

#[test]
fn interp_iconst() {
    assert_eq!(
        run_i32(
            "func @f() -> i32 {\n  v0:i32 = iconst.i32 42\n  return v0\n}\n",
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
            "func @f() -> i32 {\n  v0:i32 = iconst.i32 -7\n  return v0\n}\n",
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
            "func @f(v0:i32) -> i32 {\n  v1:i32 = iadd_imm v0, 10\n  return v1\n}\n",
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
            "func @f(v0:i32) -> i32 {\n  v1:i32 = isub_imm v0, 3\n  return v1\n}\n",
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
            "func @f(v0:i32) -> i32 {\n  v1:i32 = imul_imm v0, 4\n  return v1\n}\n",
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
            "func @f(v0:i32) -> i32 {\n  v1:i32 = ishl_imm v0, 2\n  return v1\n}\n",
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
            "func @f(v0:i32) -> i32 {\n  v1:i32 = ishr_s_imm v0, 1\n  return v1\n}\n",
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
            "func @f(v0:i32) -> i32 {\n  v1:i32 = ishr_u_imm v0, 1\n  return v1\n}\n",
            "f",
            &[Value::I32(-2)],
        ),
        0x7FFF_FFFF
    );
}

#[test]
fn interp_ieq_imm() {
    let ir = "func @f(v0:i32) -> i32 {\n  v1:i32 = ieq_imm v0, 42\n  return v1\n}\n";
    assert_eq!(run_i32(ir, "f", &[Value::I32(42)]), 1);
    assert_eq!(run_i32(ir, "f", &[Value::I32(0)]), 0);
}

// --- Casts ---

#[test]
fn interp_ftoi_sat_s_normal() {
    assert_eq!(
        run_i32(
            "func @f(v0:f32) -> i32 {\n  v1:i32 = ftoi_sat_s v0\n  return v1\n}\n",
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
            "func @f(v0:f32) -> i32 {\n  v1:i32 = ftoi_sat_u v0\n  return v1\n}\n",
            "f",
            &[Value::F32(3.7)],
        ),
        3
    );
}

#[test]
fn interp_itof_s() {
    let r = run_f32(
        "func @f(v0:i32) -> f32 {\n  v1:f32 = itof_s v0\n  return v1\n}\n",
        "f",
        &[Value::I32(-1)],
    );
    assert!((r + 1.0).abs() < 1e-5);
}

#[test]
fn interp_itof_u() {
    let r = run_f32(
        "func @f(v0:i32) -> f32 {\n  v1:f32 = itof_u v0\n  return v1\n}\n",
        "f",
        &[Value::I32(-1)],
    );
    assert!((r - 4294967296.0).abs() < 65536.0);
}

// --- Select / copy ---

#[test]
fn interp_select_true() {
    let r = run_f32(
        "func @f(v0:i32, v1:f32, v2:f32) -> f32 {\n  v3:f32 = select v0, v1, v2\n  return v3\n}\n",
        "f",
        &[Value::I32(1), Value::F32(10.0), Value::F32(20.0)],
    );
    assert!((r - 10.0).abs() < 1e-6);
}

#[test]
fn interp_select_false() {
    let r = run_f32(
        "func @f(v0:i32, v1:f32, v2:f32) -> f32 {\n  v3:f32 = select v0, v1, v2\n  return v3\n}\n",
        "f",
        &[Value::I32(0), Value::F32(10.0), Value::F32(20.0)],
    );
    assert!((r - 20.0).abs() < 1e-6);
}

#[test]
fn interp_copy() {
    let r = run_f32(
        "func @f(v0:f32) -> f32 {\n  v1:f32 = copy v0\n  return v1\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = idiv_s v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = idiv_u v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = irem_s v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = irem_u v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = idiv_s v0, v1\n  return v2\n}\n",
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
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = feq v0, v1\n  return v2\n}\n",
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
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = feq v0, v1\n  return v2\n}\n",
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
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = fne v0, v1\n  return v2\n}\n",
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
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = fne v0, v1\n  return v2\n}\n",
            "f",
            &[Value::F32(f32::NAN), Value::F32(1.0)],
        ),
        1
    );
}

#[test]
fn interp_flt_nan() {
    let ir = "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = flt v0, v1\n  return v2\n}\n";
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
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = fle v0, v1\n  return v2\n}\n",
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
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = fgt v0, v1\n  return v2\n}\n",
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
            "func @f(v0:f32, v1:f32) -> i32 {\n  v2:i32 = fge v0, v1\n  return v2\n}\n",
            "f",
            &[Value::F32(f32::NAN), Value::F32(1.0)],
        ),
        0
    );
}

#[test]
fn interp_fadd_nan() {
    let r = run_f32(
        "func @f(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fadd v0, v1\n  return v2\n}\n",
        "f",
        &[Value::F32(f32::NAN), Value::F32(1.0)],
    );
    assert!(r.is_nan());
}

#[test]
fn interp_fmul_nan() {
    let r = run_f32(
        "func @f(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fmul v0, v1\n  return v2\n}\n",
        "f",
        &[Value::F32(f32::NAN), Value::F32(1.0)],
    );
    assert!(r.is_nan());
}

// --- Float division ---

#[test]
fn interp_fdiv_by_zero_pos() {
    let r = run_f32(
        "func @f(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fdiv v0, v1\n  return v2\n}\n",
        "f",
        &[Value::F32(1.0), Value::F32(0.0)],
    );
    assert!(r.is_infinite() && r.is_sign_positive());
}

#[test]
fn interp_fdiv_by_zero_neg() {
    let r = run_f32(
        "func @f(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fdiv v0, v1\n  return v2\n}\n",
        "f",
        &[Value::F32(-1.0), Value::F32(0.0)],
    );
    assert!(r.is_infinite() && r.is_sign_negative());
}

#[test]
fn interp_fdiv_zero_by_zero() {
    let r = run_f32(
        "func @f(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fdiv v0, v1\n  return v2\n}\n",
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
            "func @f(v0:f32) -> i32 {\n  v1:i32 = ftoi_sat_s v0\n  return v1\n}\n",
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
            "func @f(v0:f32) -> i32 {\n  v1:i32 = ftoi_sat_s v0\n  return v1\n}\n",
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
            "func @f(v0:f32) -> i32 {\n  v1:i32 = ftoi_sat_s v0\n  return v1\n}\n",
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
            "func @f(v0:f32) -> i32 {\n  v1:i32 = ftoi_sat_s v0\n  return v1\n}\n",
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
            "func @f(v0:f32) -> i32 {\n  v1:i32 = ftoi_sat_s v0\n  return v1\n}\n",
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
            "func @f(v0:f32) -> i32 {\n  v1:i32 = ftoi_sat_s v0\n  return v1\n}\n",
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
            "func @f(v0:f32) -> i32 {\n  v1:i32 = ftoi_sat_u v0\n  return v1\n}\n",
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
            "func @f(v0:f32) -> i32 {\n  v1:i32 = ftoi_sat_u v0\n  return v1\n}\n",
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
            "func @f(v0:f32) -> i32 {\n  v1:i32 = ftoi_sat_u v0\n  return v1\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ishl v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ishl v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ishr_s v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = ishr_u v0, v1\n  return v2\n}\n",
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
            "func @f(v0:i32, v1:i32) -> i32 {\n  v2:i32 = imul v0, v1\n  return v2\n}\n",
            "f",
            &[Value::I32(100_000), Value::I32(100_000)],
        ),
        100_000i32.wrapping_mul(100_000)
    );
}

// --- Control flow: if / else ---

#[test]
fn interp_if_true_branch() {
    let ir = "func @f(v0:i32) -> i32 {
  v1:i32 = iconst.i32 10
  v2:i32 = iconst.i32 20
  if v0 {
    v1 = iconst.i32 99
  }
  return v1
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(1)]), 99);
}

#[test]
fn interp_if_false_branch() {
    let ir = "func @f(v0:i32) -> i32 {
  v1:i32 = iconst.i32 10
  v2:i32 = iconst.i32 20
  if v0 {
    v1 = iconst.i32 99
  }
  return v1
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(0)]), 10);
}

#[test]
fn interp_if_else_true() {
    let ir = "func @f(v0:i32) -> i32 {
  v1:i32 = iconst.i32 1
  v2:i32 = iconst.i32 2
  if v0 {
    return v1
  } else {
    return v2
  }
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(1)]), 1);
}

#[test]
fn interp_if_else_false() {
    let ir = "func @f(v0:i32) -> i32 {
  v1:i32 = iconst.i32 1
  v2:i32 = iconst.i32 2
  if v0 {
    return v1
  } else {
    return v2
  }
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(0)]), 2);
}

#[test]
fn interp_if_else_return_max() {
    let ir = "func @max(v0:f32, v1:f32) -> f32 {
  v2:i32 = fgt v0, v1
  if v2 {
    return v0
  } else {
    return v1
  }
}
";
    assert!((run_f32(ir, "max", &[Value::F32(3.0), Value::F32(2.0)]) - 3.0).abs() < 1e-6);
    assert!((run_f32(ir, "max", &[Value::F32(1.0), Value::F32(4.0)]) - 4.0).abs() < 1e-6);
}

// --- Loops ---

#[test]
fn interp_loop_sum_to_n() {
    let ir = "func @sum_to_n(v0:i32) -> i32 {
  v1:i32 = iconst.i32 0
  v2:i32 = iconst.i32 0
  loop {
    v3:i32 = ilt_s v2, v0
    br_if_not v3
    v1 = iadd v1, v2
    v4:i32 = iconst.i32 1
    v2 = iadd v2, v4
    continue
  }
  return v1
}
";
    assert_eq!(run_i32(ir, "sum_to_n", &[Value::I32(10)]), 45);
}

#[test]
fn interp_loop_break() {
    let ir = "func @f() -> i32 {
  v0:i32 = iconst.i32 7
  loop {
    break
    v0 = iconst.i32 99
  }
  return v0
}
";
    assert_eq!(run_i32(ir, "f", &[]), 7);
}

#[test]
fn interp_loop_continue() {
    let ir = "func @f() -> i32 {
  v0:i32 = iconst.i32 0
  v1:i32 = iconst.i32 0
  v2:i32 = iconst.i32 3
  loop {
    v3:i32 = ilt_s v1, v2
    br_if_not v3
    v1 = iadd_imm v1, 1
    v4:i32 = ieq_imm v1, 2
    if v4 {
      continue
    }
    v0 = iadd_imm v0, 1
    continue
  }
  return v0
}
";
    assert_eq!(run_i32(ir, "f", &[]), 2);
}

#[test]
fn interp_nested_loops() {
    let ir = "func @nested(v0:i32, v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  v3:i32 = iconst.i32 0
  loop {
    v4:i32 = ilt_s v3, v0
    br_if_not v4
    v5:i32 = iconst.i32 0
    loop {
      v6:i32 = ilt_s v5, v1
      br_if_not v6
      v2 = iadd v2, v5
      v7:i32 = iconst.i32 1
      v5 = iadd v5, v7
      continue
    }
    v8:i32 = iconst.i32 1
    v3 = iadd v3, v8
    continue
  }
  return v2
}
";
    assert_eq!(run_i32(ir, "nested", &[Value::I32(3), Value::I32(4)]), 18);
}

#[test]
fn interp_br_if_not_exits() {
    let ir = "func @f() -> i32 {
  v0:i32 = iconst.i32 0
  loop {
    v1:i32 = iconst.i32 0
    br_if_not v1
    v0 = iconst.i32 1
    break
  }
  return v0
}
";
    assert_eq!(run_i32(ir, "f", &[]), 0);
}

#[test]
fn interp_br_if_not_continues() {
    let ir = "func @f() -> i32 {
  v0:i32 = iconst.i32 0
  loop {
    v1:i32 = iconst.i32 1
    br_if_not v1
    return v0
    break
  }
  return v0
}
";
    assert_eq!(run_i32(ir, "f", &[]), 0);
}

// --- Switch ---

#[test]
fn interp_switch_case_match() {
    let ir = "func @dispatch(v0:i32) -> f32 {
  v1:f32 = fconst.f32 0.0
  switch v0 {
    case 0 {
      v1 = fconst.f32 1.0
    }
    case 1 {
      v1 = fconst.f32 2.0
    }
    case 2 {
      v1 = fconst.f32 4.0
    }
    default {
      v1 = fconst.f32 -1.0
    }
  }
  return v1
}
";
    assert!((run_f32(ir, "dispatch", &[Value::I32(1)]) - 2.0).abs() < 1e-6);
}

#[test]
fn interp_switch_default() {
    let ir = "func @dispatch(v0:i32) -> f32 {
  v1:f32 = fconst.f32 0.0
  switch v0 {
    case 0 {
      v1 = fconst.f32 1.0
    }
    case 1 {
      v1 = fconst.f32 2.0
    }
    case 2 {
      v1 = fconst.f32 4.0
    }
    default {
      v1 = fconst.f32 -1.0
    }
  }
  return v1
}
";
    assert!((run_f32(ir, "dispatch", &[Value::I32(99)]) + 1.0).abs() < 1e-6);
}

#[test]
fn interp_switch_no_default() {
    let ir = "func @sw(v0:i32) -> i32 {
  v1:i32 = iconst.i32 42
  switch v0 {
    case 0 {
      v1 = iconst.i32 1
    }
  }
  return v1
}
";
    assert_eq!(run_i32(ir, "sw", &[Value::I32(5)]), 42);
}

// --- Early return ---

#[test]
fn interp_early_return() {
    let ir = "func @early_return(v0:f32) -> f32 {
  v1:f32 = fconst.f32 0.0
  v2:i32 = flt v0, v1
  if v2 {
    v3:f32 = fneg v0
    return v3
  }
  return v0
}
";
    assert!((run_f32(ir, "early_return", &[Value::F32(-2.0)]) - 2.0).abs() < 1e-6);
    assert!((run_f32(ir, "early_return", &[Value::F32(3.0)]) - 3.0).abs() < 1e-6);
}

// --- Memory ---

#[test]
fn interp_slot_store_load() {
    let ir = "func @f(v0:f32) -> f32 {
  slot ss0, 4
  v1:i32 = slot_addr ss0
  store v1, 0, v0
  v2:f32 = load v1, 0
  return v2
}
";
    let r = run_f32(ir, "f", &[Value::F32(42.5)]);
    assert!((r - 42.5).abs() < 1e-6);
}

#[test]
fn interp_slot_store_load_i32() {
    let ir = "func @f(v0:i32) -> i32 {
  slot ss0, 4
  v1:i32 = slot_addr ss0
  store v1, 0, v0
  v2:i32 = load v1, 0
  return v2
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(-77)]), -77);
}

#[test]
fn interp_slot_offset() {
    let ir = "func @f() -> f32 {
  slot ss0, 8
  v0:i32 = slot_addr ss0
  v1:f32 = fconst.f32 1.0
  v2:f32 = fconst.f32 2.0
  store v0, 0, v1
  store v0, 4, v2
  v3:f32 = load v0, 4
  return v3
}
";
    assert!((run_f32(ir, "f", &[]) - 2.0).abs() < 1e-6);
}

#[test]
fn interp_memcpy_slots() {
    let ir = "func @f() -> i32 {
  slot ss0, 4
  slot ss1, 4
  v0:i32 = slot_addr ss0
  v1:i32 = slot_addr ss1
  v2:i32 = iconst.i32 123
  store v0, 0, v2
  memcpy v1, v0, 4
  v3:i32 = load v1, 0
  return v3
}
";
    assert_eq!(run_i32(ir, "f", &[]), 123);
}

#[test]
fn interp_dynamic_index() {
    let ir = "func @arr_dyn(v0:i32) -> f32 {
  slot ss0, 16
  v1:i32 = slot_addr ss0
  v2:f32 = fconst.f32 1.0
  store v1, 0, v2
  store v1, 4, v2
  store v1, 8, v2
  store v1, 12, v2
  v3:i32 = imul_imm v0, 4
  v4:i32 = iadd v1, v3
  v5:f32 = load v4, 0
  return v5
}
";
    assert!((run_f32(ir, "arr_dyn", &[Value::I32(2)]) - 1.0).abs() < 1e-6);
}

// --- Calls ---

#[test]
fn interp_local_call() {
    let ir = "func @helper(v0:i32) -> i32 {
  v1:i32 = iadd_imm v0, 1
  return v1
}
func @main(v0:i32) -> i32 {
  v1:i32 = call @helper(v0)
  return v1
}
";
    assert_eq!(run_i32(ir, "main", &[Value::I32(41)]), 42);
}

#[test]
fn interp_local_call_multi_return() {
    let ir = "func @pair() -> (f32, f32) {
  v0:f32 = fconst.f32 1.0
  v1:f32 = fconst.f32 2.0
  return v0, v1
}
func @main() -> f32 {
  v0:f32, v1:f32 = call @pair()
  v2:f32 = fadd v0, v1
  return v2
}
";
    assert!((run_f32(ir, "main", &[]) - 3.0).abs() < 1e-6);
}

#[test]
fn interp_import_call_unary() {
    let ir = "import @std.math::fabs(f32) -> f32

func @f(v0:f32) -> f32 {
  v1:f32 = call @std.math::fabs(v0)
  return v1
}
";
    let r = run_f32_with_imports(ir, "f", &[Value::F32(-3.0)], &mut MockMathImports);
    assert!((r - 3.0).abs() < 1e-6);
}

#[test]
fn interp_import_call_binary() {
    let ir = "import @std.math::fmax(f32, f32) -> f32

func @f(v0:f32, v1:f32) -> f32 {
  v2:f32 = call @std.math::fmax(v0, v1)
  return v2
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
    let ir = "import @std.math::fabs(f32) -> f32
import @std.math::unknown(f32) -> f32

func @f(v0:f32) -> f32 {
  v1:f32 = call @std.math::unknown(v0)
  return v1
}
";
    let m = parse_module(ir).unwrap();
    let err = interpret(&m, "f", &[Value::F32(1.0)], &mut MockMathImports).unwrap_err();
    assert!(matches!(err, InterpError::Import(_)));
}

#[test]
fn interp_factorial() {
    let ir = "func @fact(v0:i32) -> i32 {
  v1:i32 = ieq_imm v0, 0
  if v1 {
    v2:i32 = iconst.i32 1
    return v2
  }
  v3:i32 = iconst.i32 1
  v4:i32 = isub v0, v3
  v5:i32 = call @fact(v4)
  v6:i32 = imul v0, v5
  return v6
}
";
    assert_eq!(run_i32(ir, "fact", &[Value::I32(5)]), 120);
}

#[test]
fn interp_stack_overflow() {
    let ir = "func @inf(v0:i32) -> i32 {
  v1:i32 = call @inf(v0)
  return v1
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
  v0:i32 = iconst.i32 0
  return v0
}
";
    let m = parse_module(ir).unwrap();
    let err = interpret(&m, "nope", &[], &mut NoImports).unwrap_err();
    assert!(matches!(err, InterpError::FunctionNotFound(_)));
}

#[test]
fn interp_err_arg_arity() {
    let ir = "func @f() -> i32 {
  v0:i32 = iconst.i32 0
  return v0
}
";
    let m = parse_module(ir).unwrap();
    let err = interpret(&m, "f", &[Value::I32(1)], &mut NoImports).unwrap_err();
    assert!(matches!(err, InterpError::Internal(_)));
}

#[test]
fn interp_add() {
    let ir = "func @add(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
  return v2
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
            ("std.math", "fabs") => {
                let v = args[0].as_f32().unwrap();
                Ok(vec![Value::F32(v.abs())])
            }
            ("std.math", "fmax") => {
                let a = args[0].as_f32().unwrap();
                let b = args[1].as_f32().unwrap();
                Ok(vec![Value::F32(a.max(b))])
            }
            ("std.math", "unknown") => Err(InterpError::Import(String::from("unknown"))),
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
