//! Integration-style tests: round-trip, sizing, validation.

#[path = "tests/all_ops_roundtrip.rs"]
mod all_ops_roundtrip;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::mem::size_of;

use crate::builder::FunctionBuilder;
use crate::interp::{ImportHandler, InterpError, Value, interpret};
use crate::module::{ImportDecl, IrFunction, IrModule};
use crate::op::Op;
use crate::parse::{ParseError, parse_module};
use crate::print::print_module;
use crate::types::{CalleeRef, IrType, VReg, VRegRange};
use crate::validate::{validate_function, validate_module};

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
fn validate_parsed_control_flow_examples() {
    let abs = "func @abs(v0:f32) -> f32 {
  v1:f32 = fconst.f32 0.0
  v2:i32 = flt v0, v1
  if v2 {
    v0 = fneg v0
  }
  return v0
}
";
    let dispatch = "func @dispatch(v0:i32) -> f32 {
  v1:f32 = fconst.f32 0.0
  switch v0 {
    case 0 {
      v1 = fconst.f32 1.0
    }
    case 1 {
      v1 = fconst.f32 2.0
    }
    default {
      v1 = fconst.f32 -1.0
    }
  }
  return v1
}
";
    for src in [abs, dispatch] {
        let m = parse_module(src).unwrap();
        validate_module(&m).unwrap();
    }
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

struct NoImports;

impl ImportHandler for NoImports {
    fn call(&mut self, _: &str, _: &str, _: &[Value]) -> Result<Vec<Value>, InterpError> {
        Err(InterpError::Import(String::from("no imports")))
    }
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

#[test]
fn validate_simple_add_passes() {
    let ir = "func @add(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
  return v2
}
";
    let m = parse_module(ir).unwrap();
    validate_module(&m).unwrap();
}

#[test]
fn validate_err_break_outside_loop() {
    let f = IrFunction {
        name: String::from("bad"),
        is_entry: false,
        param_count: 0,
        return_types: Vec::new(),
        vreg_types: Vec::new(),
        slots: Vec::new(),
        body: vec![Op::Break],
        vreg_pool: Vec::new(),
    };
    let m = IrModule {
        imports: Vec::new(),
        functions: vec![f],
    };
    let errs = validate_module(&m).expect_err("expected validation errors");
    assert!(errs.iter().any(|e| e.message.contains("loop")));
}

#[test]
fn validate_err_duplicate_import() {
    let ir = "import @m::f(f32) -> f32
import @m::f(f32) -> f32

func @c() {
  return
}
";
    let m = parse_module(ir).unwrap();
    let errs = validate_module(&m).expect_err("duplicate import");
    assert!(errs.iter().any(|e| e.message.contains("duplicate import")));
}

#[test]
fn validate_err_two_entry() {
    let ir = "entry func @a() {
  return
}
entry func @b() {
  return
}
";
    let m = parse_module(ir).unwrap();
    let errs = validate_module(&m).expect_err("two entry");
    assert!(errs.iter().any(|e| e.message.contains("entry")));
}

#[test]
fn validate_err_undefined_vreg() {
    let f = IrFunction {
        name: String::from("bad"),
        is_entry: false,
        param_count: 0,
        return_types: vec![crate::types::IrType::F32],
        vreg_types: vec![crate::types::IrType::F32, crate::types::IrType::F32],
        slots: Vec::new(),
        body: vec![Op::Fadd {
            dst: crate::types::VReg(1),
            lhs: crate::types::VReg(0),
            rhs: crate::types::VReg(0),
        }],
        vreg_pool: Vec::new(),
    };
    let m = IrModule {
        imports: Vec::new(),
        functions: vec![f],
    };
    let errs = validate_function(&m.functions[0], &m).expect_err("undefined v0");
    assert!(errs.iter().any(|e| e.message.contains("before definition")));
}

#[test]
fn validate_err_copy_type_mismatch() {
    let f = IrFunction {
        name: String::from("bad"),
        is_entry: false,
        param_count: 2,
        return_types: Vec::new(),
        vreg_types: vec![IrType::F32, IrType::I32],
        slots: Vec::new(),
        body: vec![Op::Copy {
            dst: VReg(1),
            src: VReg(0),
        }],
        vreg_pool: Vec::new(),
    };
    let m = IrModule {
        imports: Vec::new(),
        functions: vec![f],
    };
    let errs = validate_module(&m).expect_err("copy types");
    assert!(errs.iter().any(|e| e.message.contains("copy")));
}

#[test]
fn validate_err_call_arity() {
    let mut fb = FunctionBuilder::new("c", &[]);
    let v0 = fb.alloc_vreg(IrType::F32);
    fb.push(Op::FconstF32 {
        dst: v0,
        value: 1.0,
    });
    fb.push_call(CalleeRef(0), &[], &[]);
    let func = fb.finish();
    let m = IrModule {
        imports: vec![ImportDecl {
            module_name: String::from("m"),
            func_name: String::from("g"),
            param_types: vec![IrType::F32],
            return_types: Vec::new(),
        }],
        functions: vec![func],
    };
    let errs = validate_module(&m).expect_err("call arity");
    assert!(errs.iter().any(|e| e.message.contains("arg count")));
}

#[test]
fn validate_err_callee_oob() {
    let f = IrFunction {
        name: String::from("bad"),
        is_entry: false,
        param_count: 0,
        return_types: Vec::new(),
        vreg_types: Vec::new(),
        slots: Vec::new(),
        body: vec![Op::Call {
            callee: CalleeRef(3),
            args: VRegRange { start: 0, count: 0 },
            results: VRegRange { start: 0, count: 0 },
        }],
        vreg_pool: Vec::new(),
    };
    let m = IrModule {
        imports: Vec::new(),
        functions: vec![f],
    };
    let errs = validate_module(&m).expect_err("callee");
    assert!(errs.iter().any(|e| e.message.contains("callee")));
}

#[test]
fn validate_err_continue_outside_loop() {
    let f = IrFunction {
        name: String::from("bad"),
        is_entry: false,
        param_count: 0,
        return_types: Vec::new(),
        vreg_types: Vec::new(),
        slots: Vec::new(),
        body: vec![Op::Continue],
        vreg_pool: Vec::new(),
    };
    let m = IrModule {
        imports: Vec::new(),
        functions: vec![f],
    };
    let errs = validate_module(&m).expect_err("continue");
    assert!(errs.iter().any(|e| e.message.contains("loop")));
}

#[test]
fn validate_err_duplicate_func_name_parsed() {
    let ir = "func @x() {
  return
}
func @x() {
  return
}
";
    let m = parse_module(ir).unwrap();
    let errs = validate_module(&m).expect_err("dup func");
    assert!(
        errs.iter()
            .any(|e| e.message.contains("duplicate function"))
    );
}

#[test]
fn validate_err_duplicate_switch_case() {
    let mut b = FunctionBuilder::new("sw", &[]);
    let sel = b.alloc_vreg(IrType::I32);
    b.push(Op::IconstI32 { dst: sel, value: 0 });
    b.push_switch(sel);
    b.push_case(0);
    b.end_switch_arm();
    b.push_case(0);
    b.end_switch_arm();
    b.push_default();
    b.end_switch_arm();
    b.end_switch();
    let f = b.finish();
    let m = IrModule {
        imports: Vec::new(),
        functions: vec![f],
    };
    let errs = validate_module(&m).expect_err("dup case");
    assert!(
        errs.iter()
            .any(|e| e.message.contains("duplicate switch case"))
    );
}

#[test]
fn validate_err_return_value_type() {
    let f = IrFunction {
        name: String::from("bad"),
        is_entry: false,
        param_count: 0,
        return_types: vec![IrType::F32],
        vreg_types: vec![IrType::I32],
        slots: Vec::new(),
        body: vec![
            Op::IconstI32 {
                dst: VReg(0),
                value: 1,
            },
            Op::Return {
                values: VRegRange { start: 0, count: 1 },
            },
        ],
        vreg_pool: vec![VReg(0)],
    };
    let m = IrModule {
        imports: Vec::new(),
        functions: vec![f],
    };
    let errs = validate_module(&m).expect_err("return type");
    assert!(errs.iter().any(|e| e.message.contains("return value")));
}

#[test]
fn validate_err_vreg_pool_oob() {
    let f = IrFunction {
        name: String::from("bad"),
        is_entry: false,
        param_count: 0,
        return_types: Vec::new(),
        vreg_types: Vec::new(),
        slots: Vec::new(),
        body: vec![Op::Return {
            values: VRegRange { start: 0, count: 1 },
        }],
        vreg_pool: Vec::new(),
    };
    let m = IrModule {
        imports: Vec::new(),
        functions: vec![f],
    };
    let errs = validate_module(&m).expect_err("pool");
    assert!(errs.iter().any(|e| e.message.contains("pool")));
}

#[test]
fn validate_err_slot_addr_oob() {
    let f = IrFunction {
        name: String::from("bad"),
        is_entry: false,
        param_count: 0,
        return_types: Vec::new(),
        vreg_types: vec![crate::types::IrType::I32],
        slots: Vec::new(),
        body: vec![Op::SlotAddr {
            dst: crate::types::VReg(0),
            slot: crate::types::SlotId(0),
        }],
        vreg_pool: Vec::new(),
    };
    let m = IrModule {
        imports: Vec::new(),
        functions: vec![f],
    };
    let errs = validate_function(&m.functions[0], &m).expect_err("bad slot");
    assert!(errs.iter().any(|e| e.message.contains("slot")));
}
