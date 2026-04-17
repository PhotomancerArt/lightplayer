//! [`LpirOp::Block`] / [`LpirOp::ExitBlock`] behavior and text round-trip.

use alloc::string::String;
use alloc::vec::Vec;

use crate::interp::{ImportHandler, InterpError, Value, interpret};
use crate::lpir_op::LpirOp;
use crate::parse::parse_module;
use crate::print::print_module;
use crate::validate::validate_module;

struct NoImports;

impl ImportHandler for NoImports {
    fn call(&mut self, _: &str, _: &str, _: &[Value]) -> Result<Vec<Value>, InterpError> {
        Err(InterpError::Import(String::from("no imports")))
    }
}

fn run_i32(ir: &str, func: &str, args: &[Value]) -> i32 {
    let module = parse_module(ir).unwrap_or_else(|e| panic!("parse: {e:?}"));
    validate_module(&module).unwrap_or_else(|e| panic!("validate: {e:?}"));
    let out = interpret(&module, func, args, &mut NoImports).unwrap();
    assert_eq!(out.len(), 1);
    out[0].as_i32().expect("i32")
}

#[test]
fn block_exit_skips_rest_of_body() {
    let ir = "func @f(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  block {
    exit_block
    v2 = iconst.i32 1
  }
  return v2
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(0)]), 0);
}

#[test]
fn block_nested_exit_innermost_only() {
    let ir = "func @f(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  block {
    block {
      exit_block
      v2 = iconst.i32 1
    }
    v2 = iconst.i32 2
  }
  return v2
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(0)]), 2);
}

#[test]
fn block_exit_from_inside_if() {
    let ir = "func @f(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  block {
    if v1 {
      exit_block
    }
    v2 = iconst.i32 7
  }
  return v2
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(1)]), 0);
    assert_eq!(run_i32(ir, "f", &[Value::I32(0)]), 7);
}

#[test]
fn loop_continuing_offset_points_at_marker_op() {
    let ir = "func @f(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  loop {
    v3:i32 = iconst.i32 1
    continuing:
    v2 = iadd v2, v3
    br_if_not v1
  }
  return v2
}
";
    let module = parse_module(ir).unwrap_or_else(|e| panic!("parse: {e:?}"));
    validate_module(&module).unwrap_or_else(|e| panic!("validate: {e:?}"));
    let f = module.functions.values().next().expect("one func");
    let (loop_pc, co) = f
        .body
        .iter()
        .enumerate()
        .find_map(|(i, op)| {
            if let LpirOp::LoopStart {
                continuing_offset, ..
            } = op
            {
                Some((i, *continuing_offset as usize))
            } else {
                None
            }
        })
        .expect("LoopStart");
    assert!(matches!(f.body.get(co), Some(LpirOp::Continuing)));
    assert_eq!(co, loop_pc + 2);
}

#[test]
fn block_text_round_trip() {
    let src = "func @f(v1:i32) -> i32 {
  block {
    exit_block
  }
  v2:i32 = iconst.i32 3
  return v2
}
";
    let module = parse_module(src).unwrap_or_else(|e| panic!("parse: {e:?}"));
    validate_module(&module).unwrap_or_else(|e| panic!("validate: {e:?}"));
    assert_eq!(print_module(&module), src);
}
