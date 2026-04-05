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
        "func @add(v1:f32, v2:f32) -> f32 {
  v3:f32 = fadd v1, v2
  return v3
}
",
    );
}

#[test]
fn round_trip_abs() {
    assert_round_trip(
        "func @abs(v1:f32) -> f32 {
  v2:f32 = fconst.f32 0.0
  v3:i32 = flt v1, v2
  if v3 {
    v1 = fneg v1
  }
  return v1
}
",
    );
}

#[test]
fn round_trip_max() {
    assert_round_trip(
        "func @max(v1:f32, v2:f32) -> f32 {
  v3:i32 = fgt v1, v2
  if v3 {
    return v1
  } else {
    return v2
  }
}
",
    );
}

#[test]
fn round_trip_sum_to_n() {
    assert_round_trip(
        "func @sum_to_n(v1:i32) -> i32 {
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
",
    );
}

#[test]
fn round_trip_nested_loops() {
    assert_round_trip(
        "func @nested(v1:i32, v2:i32) -> i32 {
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
",
    );
}

#[test]
fn round_trip_dispatch() {
    assert_round_trip(
        "func @dispatch(v1:i32) -> f32 {
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
",
    );
}

#[test]
fn round_trip_early_return() {
    assert_round_trip(
        "func @early_return(v1:f32) -> f32 {
  v2:f32 = fconst.f32 0.0
  v3:i32 = flt v1, v2
  if v3 {
    v4:f32 = fneg v1
    return v4
  }
  return v1
}
",
    );
}

#[test]
fn round_trip_entry_and_multi_return() {
    assert_round_trip(
        "entry func @main(v1:f32, v2:f32) -> (f32, f32) {
  v3:f32 = fadd v1, v2
  v4:f32 = fsub v1, v2
  return v3, v4
}
",
    );
}

#[test]
fn round_trip_import_and_call() {
    assert_round_trip(
        "import @glsl::fsin(f32) -> f32

func @use(v1:f32) -> f32 {
  v2:f32 = call @glsl::fsin(v1)
  return v2
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

func @noise_sample(v1:f32, v2:f32, v3:f32) -> f32 {
  slot ss0, 12
  v4:i32 = slot_addr ss0
  call @lpfx::noise3(v4, v1, v2, v3)
  v5:f32 = load v4, 0
  v6:f32 = load v4, 4
  v7:f32 = load v4, 8
  return v5
}
",
    );
}

#[test]
fn round_trip_fill_vec3() {
    assert_round_trip(
        "func @fill_vec3(v1:f32, v2:i32) {
  v3:f32 = fmul v1, v1
  store v2, 0, v3
  store v2, 4, v3
  store v2, 8, v3
}
",
    );
}

#[test]
fn round_trip_arr_dyn() {
    assert_round_trip(
        "func @arr_dyn(v1:i32) -> f32 {
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
",
    );
}

#[test]
fn round_trip_use_ctx() {
    assert_round_trip(
        "func @use_ctx(v1:f32, v2:i32) -> f32 {
  v3:f32 = load v2, 0
  v4:f32 = load v2, 4
  v5:f32 = fadd v3, v4
  store v2, 0, v5
  return v5
}
",
    );
}

#[test]
fn round_trip_copy_mat4() {
    assert_round_trip(
        "func @copy_mat4(v1:i32, v2:i32) {
  memcpy v1, v2, 64
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
  v1:f32 = fconst.f32 -0.0
  v2:f32 = fconst.f32 inf
  v3:f32 = fconst.f32 -inf
  v4:f32 = fconst.f32 nan
  v5:i32 = iconst.i32 16
  v6:i32 = iconst.i32 -7
  return v5
}
",
    );
}

#[test]
fn round_trip_loop_continuing() {
    assert_round_trip(
        "func @for_sum(v1:i32) -> i32 {
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
",
    );
}

#[test]
fn round_trip_loop_continuing_with_break_if() {
    assert_round_trip(
        "func @for_sum2(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  v3:i32 = iconst.i32 0
  loop {
    v2 = iadd v2, v3
    continuing:
    v3 = iadd_imm v3, 1
    v4:i32 = ige_s v3, v1
    br_if_not v4
  }
  return v2
}
",
    );
}

#[test]
fn parse_accepts_hex_iconst() {
    let ir = "func @h() -> i32 {
  v1:i32 = iconst.i32 0xff
  return v1
}
";
    parse_module(ir).expect("hex iconst should parse");
}
