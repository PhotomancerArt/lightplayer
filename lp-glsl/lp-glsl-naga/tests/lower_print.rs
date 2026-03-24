//! GLSL → Naga → LPIR text (integration).

use lp_glsl_naga::{compile, lower};
use lpir::{print_module, validate_module};

#[test]
fn print_contains_func_and_fadd() {
    let glsl = "float add(float a, float b) { return a + b; }";
    let naga = compile(glsl).expect("compile");
    let ir = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate");
    let s = print_module(&ir);
    assert!(s.contains("func @add"), "{s}");
    assert!(s.contains("fadd"), "{s}");
}

#[test]
fn print_contains_if_structure() {
    let glsl = "float f(float x) { if (x > 0.0) return 1.0; else return 0.0; }";
    let naga = compile(glsl).expect("compile");
    let ir = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate");
    let s = print_module(&ir);
    assert!(s.contains("if "), "{s}");
}

#[test]
fn print_contains_std_math_import() {
    let glsl = "float f(float x) { return sin(x); }";
    let naga = compile(glsl).expect("compile");
    let ir = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate");
    let s = print_module(&ir);
    assert!(s.contains("import @std.math::"), "{s}");
    assert!(s.contains("call @std.math::"), "{s}");
}

#[test]
fn print_contains_user_call() {
    let glsl = "float g(float x) { return x; } float f(float x) { return g(x); }";
    let naga = compile(glsl).expect("compile");
    let ir = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate");
    let s = print_module(&ir);
    assert!(s.contains("call @g("), "{s}");
}
