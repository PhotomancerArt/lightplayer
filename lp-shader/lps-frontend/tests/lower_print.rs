//! GLSL → Naga → LPIR text (integration).

use lpir::{LpirOp, VReg, print_module, validate_module};
use lps_frontend::{compile, lower};

#[test]
fn print_contains_func_and_fadd() {
    let glsl = "float add(float a, float b) { return a + b; }";
    let naga = compile(glsl).expect("compile");
    let (ir, _) = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate");
    let s = print_module(&ir);
    assert!(s.contains("func @add"), "{s}");
    assert!(s.contains("fadd"), "{s}");
}

#[test]
fn print_contains_if_structure() {
    let glsl = "float f(float x) { if (x > 0.0) return 1.0; else return 0.0; }";
    let naga = compile(glsl).expect("compile");
    let (ir, _) = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate");
    let s = print_module(&ir);
    assert!(s.contains("if "), "{s}");
}

#[test]
fn print_contains_std_math_import() {
    let glsl = "float f(float x) { return sin(x); }";
    let naga = compile(glsl).expect("compile");
    let (ir, _) = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate");
    let s = print_module(&ir);
    assert!(
        s.contains("import @glsl::") || s.contains("import @lpir::"),
        "{s}"
    );
    assert!(
        s.contains("call @glsl::") || s.contains("call @lpir::"),
        "{s}"
    );
}

#[test]
fn print_contains_loop_structure() {
    let glsl =
        "int f(int n) { int s = 0; int i = 0; while (i < n) { s = s + i; i = i + 1; } return s; }";
    let naga = compile(glsl).expect("compile");
    let (ir, _) = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate");
    let s = print_module(&ir);
    assert!(s.contains("loop {"), "{s}");
}

#[test]
fn print_contains_user_call() {
    let glsl = "float g(float x) { return x; } float f(float x) { return g(x); }";
    let naga = compile(glsl).expect("compile");
    let (ir, _) = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate");
    let s = print_module(&ir);
    assert!(s.contains("call @g("), "{s}");
}

#[test]
fn sum_arrays_sret_slot_addr_never_overwrites_param_vregs() {
    let glsl = r#"
        float[2] sum_arrays(float[2] a, float[2] b) {
            return float[2](a[0] + b[0], a[1] + b[1]);
        }
    "#;
    let naga = compile(glsl).expect("compile");
    let (ir, _) = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate");
    let f = ir
        .functions
        .values()
        .find(|x| x.name == "sum_arrays")
        .expect("sum_arrays");
    let u0 = f.user_param_vreg(0);
    let u1 = f.user_param_vreg(1);
    let sr = f.sret_arg.expect("sret");
    for op in &f.body {
        if let LpirOp::SlotAddr { dst, .. } = op {
            assert_ne!(*dst, VReg(0), "slot_addr dst must not be vmctx");
            assert_ne!(*dst, sr, "slot_addr dst must not be sret");
            assert_ne!(*dst, u0, "slot_addr dst must not alias param a");
            assert_ne!(*dst, u1, "slot_addr dst must not alias param b");
        }
    }
}
