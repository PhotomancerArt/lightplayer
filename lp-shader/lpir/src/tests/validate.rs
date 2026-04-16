//! Validator positive and negative tests.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::builder::FunctionBuilder;
use crate::lpir_module::{ImportDecl, IrFunction, LpirModule, VMCTX_VREG};
use crate::lpir_op::LpirOp;
use crate::parse::parse_module;
use crate::types::{CalleeRef, IrType, VReg, VRegRange};
use crate::validate::{validate_function, validate_module};

#[test]
fn validate_parsed_control_flow_examples() {
    let abs = "func @abs(v1:f32) -> f32 {
  v2:f32 = fconst.f32 0.0
  v3:i32 = flt v1, v2
  if v3 {
    v1 = fneg v1
  }
  return v1
}
";
    let dispatch = "func @dispatch(v1:i32) -> f32 {
  v2:f32 = fconst.f32 0.0
  switch v1 {
    case 0 {
      v2 = fconst.f32 1.0
    }
    case 1 {
      v2 = fconst.f32 2.0
    }
    default {
      v2 = fconst.f32 -1.0
    }
  }
  return v2
}
";
    for src in [abs, dispatch] {
        let m = parse_module(src).unwrap();
        validate_module(&m).unwrap();
    }
}

#[test]
fn validate_simple_add_passes() {
    let ir = "func @add(v1:f32, v2:f32) -> f32 {
  v3:f32 = fadd v1, v2
  return v3
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
        vmctx_vreg: VMCTX_VREG,
        param_count: 0,
        return_types: Vec::new(),
        vreg_types: Vec::new(),
        slots: Vec::new(),
        body: vec![LpirOp::Break],
        vreg_pool: Vec::new(),
    };
    let m = LpirModule {
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
        vmctx_vreg: VMCTX_VREG,
        param_count: 0,
        return_types: vec![IrType::F32],
        vreg_types: vec![IrType::Pointer, IrType::F32, IrType::F32],
        slots: Vec::new(),
        body: vec![LpirOp::Fadd {
            dst: VReg(2),
            lhs: VReg(1),
            rhs: VReg(1),
        }],
        vreg_pool: Vec::new(),
    };
    let m = LpirModule {
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
        vmctx_vreg: VMCTX_VREG,
        param_count: 2,
        return_types: Vec::new(),
        vreg_types: vec![IrType::Pointer, IrType::F32, IrType::I32],
        slots: Vec::new(),
        body: vec![LpirOp::Copy {
            dst: VReg(2),
            src: VReg(1),
        }],
        vreg_pool: Vec::new(),
    };
    let m = LpirModule {
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
    fb.push(LpirOp::FconstF32 {
        dst: v0,
        value: 1.0,
    });
    fb.push_call(CalleeRef(0), &[], &[]);
    let func = fb.finish();
    let m = LpirModule {
        imports: vec![ImportDecl {
            module_name: String::from("m"),
            func_name: String::from("g"),
            param_types: vec![IrType::F32],
            return_types: Vec::new(),
            lpfn_glsl_params: None,
            needs_vmctx: false,
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
        vmctx_vreg: VMCTX_VREG,
        param_count: 0,
        return_types: Vec::new(),
        vreg_types: Vec::new(),
        slots: Vec::new(),
        body: vec![LpirOp::Call {
            callee: CalleeRef(3),
            args: VRegRange { start: 0, count: 0 },
            results: VRegRange { start: 0, count: 0 },
        }],
        vreg_pool: Vec::new(),
    };
    let m = LpirModule {
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
        vmctx_vreg: VMCTX_VREG,
        param_count: 0,
        return_types: Vec::new(),
        vreg_types: Vec::new(),
        slots: Vec::new(),
        body: vec![LpirOp::Continue],
        vreg_pool: Vec::new(),
    };
    let m = LpirModule {
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
    b.push(LpirOp::IconstI32 { dst: sel, value: 0 });
    b.push_switch(sel);
    b.push_case(0);
    b.end_switch_arm();
    b.push_case(0);
    b.end_switch_arm();
    b.push_default();
    b.end_switch_arm();
    b.end_switch();
    let f = b.finish();
    let m = LpirModule {
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
        vmctx_vreg: VMCTX_VREG,
        param_count: 0,
        return_types: vec![IrType::F32],
        vreg_types: vec![IrType::Pointer, IrType::I32],
        slots: Vec::new(),
        body: vec![
            LpirOp::IconstI32 {
                dst: VReg(1),
                value: 1,
            },
            LpirOp::Return {
                values: VRegRange { start: 0, count: 1 },
            },
        ],
        vreg_pool: vec![VReg(1)],
    };
    let m = LpirModule {
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
        vmctx_vreg: VMCTX_VREG,
        param_count: 0,
        return_types: Vec::new(),
        vreg_types: Vec::new(),
        slots: Vec::new(),
        body: vec![LpirOp::Return {
            values: VRegRange { start: 0, count: 1 },
        }],
        vreg_pool: Vec::new(),
    };
    let m = LpirModule {
        imports: Vec::new(),
        functions: vec![f],
    };
    let errs = validate_module(&m).expect_err("pool");
    assert!(errs.iter().any(|e| e.message.contains("pool")));
}

#[test]
fn validate_err_continue_in_continuing() {
    let ir = "func @bad(v1:i32) -> i32 {
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
    continue
  }
  return v2
}
";
    let m = parse_module(ir).unwrap();
    let errs = validate_module(&m).expect_err("continue in continuing");
    assert!(
        errs.iter()
            .any(|e| e.message.contains("continue inside continuing"))
    );
}

#[test]
fn validate_ok_continue_in_nested_loop_in_continuing() {
    let ir = "func @ok() -> i32 {
  v1:i32 = iconst.i32 0
  v2:i32 = iconst.i32 0
  loop {
    v3:i32 = ige_s v2, v1
    if v3 {
      break
    }
    continuing:
    v2 = iadd_imm v2, 1
    v4:i32 = iconst.i32 0
    loop {
      v5:i32 = ieq_imm v4, 1
      if v5 {
        break
      }
      v4 = iadd_imm v4, 1
      continue
    }
  }
  return v1
}
";
    let m = parse_module(ir).unwrap();
    validate_module(&m).unwrap();
}

#[test]
fn validate_ok_loop_continuing_passes() {
    let ir = "func @f(v1:i32) -> i32 {
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
    let m = parse_module(ir).unwrap();
    validate_module(&m).unwrap();
}

#[test]
fn validate_err_slot_addr_oob() {
    let f = IrFunction {
        name: String::from("bad"),
        is_entry: false,
        vmctx_vreg: VMCTX_VREG,
        param_count: 0,
        return_types: Vec::new(),
        vreg_types: vec![IrType::Pointer, IrType::I32],
        slots: Vec::new(),
        body: vec![LpirOp::SlotAddr {
            dst: VReg(1),
            slot: crate::types::SlotId(0),
        }],
        vreg_pool: Vec::new(),
    };
    let m = LpirModule {
        imports: Vec::new(),
        functions: vec![f],
    };
    let errs = validate_function(&m.functions[0], &m).expect_err("bad slot");
    assert!(errs.iter().any(|e| e.message.contains("slot")));
}
