//! Integration-style tests: round-trip, sizing.

#[path = "tests/all_ops_roundtrip.rs"]
mod all_ops_roundtrip;

#[path = "tests/interp.rs"]
mod interp;

#[path = "tests/validate.rs"]
mod validate;

use alloc::string::{String, ToString};
use core::mem::size_of;

use crate::op::Op;
use crate::parse::{ParseError, parse_module};
use crate::print::print_module;
use crate::validate::validate_module;

fn assert_round_trip(src: &str) {
    let module = parse_module(src).unwrap_or_else(|e| panic!("parse: {e:?}"));
    let out = print_module(&module);
    assert_eq!(out, src);
}

#[test]
fn round_trip_simple_add() {
    assert_round_trip(
        "func @add(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
  return v2
}
",
    );
}

#[test]
fn round_trip_abs() {
    assert_round_trip(
        "func @abs(v0:f32) -> f32 {
  v1:f32 = fconst.f32 0.0
  v2:i32 = flt v0, v1
  if v2 {
    v0 = fneg v0
  }
  return v0
}
",
    );
}

#[test]
fn round_trip_max() {
    assert_round_trip(
        "func @max(v0:f32, v1:f32) -> f32 {
  v2:i32 = fgt v0, v1
  if v2 {
    return v0
  } else {
    return v1
  }
}
",
    );
}

#[test]
fn round_trip_sum_to_n() {
    assert_round_trip(
        "func @sum_to_n(v0:i32) -> i32 {
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
",
    );
}

#[test]
fn round_trip_nested_loops() {
    assert_round_trip(
        "func @nested(v0:i32, v1:i32) -> i32 {
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
",
    );
}

#[test]
fn round_trip_dispatch() {
    assert_round_trip(
        "func @dispatch(v0:i32) -> f32 {
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
",
    );
}

#[test]
fn round_trip_early_return() {
    assert_round_trip(
        "func @early_return(v0:f32) -> f32 {
  v1:f32 = fconst.f32 0.0
  v2:i32 = flt v0, v1
  if v2 {
    v3:f32 = fneg v0
    return v3
  }
  return v0
}
",
    );
}

#[test]
fn round_trip_entry_and_multi_return() {
    assert_round_trip(
        "entry func @main(v0:f32, v1:f32) -> (f32, f32) {
  v2:f32 = fadd v0, v1
  v3:f32 = fsub v0, v1
  return v2, v3
}
",
    );
}

#[test]
fn round_trip_import_and_call() {
    assert_round_trip(
        "import @std.math::fsin(f32) -> f32

func @use(v0:f32) -> f32 {
  v1:f32 = call @std.math::fsin(v0)
  return v1
}
",
    );
}

#[test]
fn op_enum_payload_reasonable_size() {
    // Design note (stage II): ~20 bytes per op was an initial target; on 64-bit the
    // enum is larger. Keep a loose bound so size regressions are visible.
    assert!(
        size_of::<Op>() <= 32,
        "Op size {} exceeds 32-byte sanity bound",
        size_of::<Op>()
    );
}

#[test]
fn parse_error_unexpected_line() {
    let err = parse_module("func @test() {\n  xyz\n}\n").unwrap_err();
    assert!(
        err.message.contains("unrecognized") || err.message.contains("expected"),
        "{:?}",
        err.message
    );
}

#[test]
fn parse_error_unclosed_brace() {
    let err = parse_module("func @test() {").unwrap_err();
    assert!(err.line >= 1);
}

#[test]
fn parse_error_display() {
    let e = ParseError {
        line: 2,
        column: 5,
        message: String::from("test"),
    };
    assert!(e.to_string().contains("test"));
}

#[test]
fn round_trip_noise_sample() {
    assert_round_trip(
        "import @lpfx::noise3(i32, f32, f32, f32)

func @noise_sample(v0:f32, v1:f32, v2:f32) -> f32 {
  slot ss0, 12
  v3:i32 = slot_addr ss0
  call @lpfx::noise3(v3, v0, v1, v2)
  v4:f32 = load v3, 0
  v5:f32 = load v3, 4
  v6:f32 = load v3, 8
  return v4
}
",
    );
}

#[test]
fn round_trip_fill_vec3() {
    assert_round_trip(
        "func @fill_vec3(v0:f32, v1:i32) {
  v2:f32 = fmul v0, v0
  store v1, 0, v2
  store v1, 4, v2
  store v1, 8, v2
}
",
    );
}

#[test]
fn round_trip_arr_dyn() {
    assert_round_trip(
        "func @arr_dyn(v0:i32) -> f32 {
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
",
    );
}

#[test]
fn round_trip_use_ctx() {
    assert_round_trip(
        "func @use_ctx(v0:f32, v1:i32) -> f32 {
  v2:f32 = load v1, 0
  v3:f32 = load v1, 4
  v4:f32 = fadd v2, v3
  store v1, 0, v4
  return v4
}
",
    );
}

#[test]
fn round_trip_copy_mat4() {
    assert_round_trip(
        "func @copy_mat4(v0:i32, v1:i32) {
  memcpy v0, v1, 64
}
",
    );
}

#[test]
fn round_trip_all_ops() {
    let m = all_ops_roundtrip::module_all_ops();
    let s = print_module(&m);
    let m2 = parse_module(&s).unwrap_or_else(|e| panic!("parse: {e:?}\n{s}"));
    assert_eq!(print_module(&m2), s);
    validate_module(&m2).unwrap();
}

#[test]
fn round_trip_constants() {
    assert_round_trip(
        "func @constants() -> i32 {
  v0:f32 = fconst.f32 -0.0
  v1:f32 = fconst.f32 inf
  v2:f32 = fconst.f32 -inf
  v3:f32 = fconst.f32 nan
  v4:i32 = iconst.i32 16
  v5:i32 = iconst.i32 -7
  return v4
}
",
    );
}

#[test]
fn parse_accepts_hex_iconst() {
    let ir = "func @h() -> i32 {
  v0:i32 = iconst.i32 0xff
  return v0
}
";
    parse_module(ir).expect("hex iconst should parse");
}
