//! GLSL → Naga → LPIR → interpreter (integration).

use lp_glsl_naga::std_math_handler::StdMathHandler;
use lp_glsl_naga::{compile, lower};
use lpir::{ImportHandler, InterpError, Value, interpret, validate_module};

#[test]
fn interp_float_add() {
    let glsl = "float f(float a, float b) { return a + b; }";
    assert_f32_close(
        run_f32(glsl, "f", &[Value::F32(2.0), Value::F32(3.0)]),
        5.0,
        1e-5,
    );
}

#[test]
fn interp_float_sub_mul_div() {
    assert_f32_close(
        run_f32(
            "float f(float a, float b) { return a - b; }",
            "f",
            &[Value::F32(5.0), Value::F32(2.0)],
        ),
        3.0,
        1e-5,
    );
    assert_f32_close(
        run_f32(
            "float f(float a, float b) { return a * b; }",
            "f",
            &[Value::F32(3.0), Value::F32(4.0)],
        ),
        12.0,
        1e-5,
    );
    assert_f32_close(
        run_f32(
            "float f(float a, float b) { return a / b; }",
            "f",
            &[Value::F32(8.0), Value::F32(2.0)],
        ),
        4.0,
        1e-5,
    );
}

#[test]
fn interp_int_arithmetic() {
    let glsl = "int f(int a, int b) { return a + b; }";
    assert_eq!(run_i32(glsl, "f", &[Value::I32(2), Value::I32(3)]), 5);
}

#[test]
fn interp_float_negation() {
    let glsl = "float f(float x) { return -x; }";
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(3.0)]), -3.0, 1e-5);
}

#[test]
fn interp_int_negation() {
    let glsl = "int f(int x) { return -x; }";
    assert_eq!(run_i32(glsl, "f", &[Value::I32(7)]), -7);
}

#[test]
fn interp_literal_return() {
    let glsl = "float f() { return 42.0; }";
    assert_f32_close(run_f32(glsl, "f", &[]), 42.0, 1e-5);
}

#[test]
fn interp_int_literal() {
    let glsl = "int f() { return 7; }";
    assert_eq!(run_i32(glsl, "f", &[]), 7);
}

#[test]
fn interp_if_else() {
    let glsl = "float f(float x) { if (x > 0.0) return 1.0; else return -1.0; }";
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(1.0)]), 1.0, 1e-5);
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(-1.0)]), -1.0, 1e-5);
}

#[test]
#[ignore = "while/for loops: Naga loop body can reference counter locals before init stores land in the LPIR op order (validator: used before definition)"]
fn interp_loop_sum() {
    let glsl =
        "int f(int n) { int s = 0; int i = 0; while (i < n) { s = s + i; i = i + 1; } return s; }";
    assert_eq!(run_i32(glsl, "f", &[Value::I32(4)]), 6);
}

#[test]
fn interp_local_var() {
    let glsl = "float f(float x) { float y = x * 2.0; return y + 1.0; }";
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(3.0)]), 7.0, 1e-5);
}

#[test]
fn interp_ternary() {
    let glsl = "float f(float x) { return x > 0.0 ? 1.0 : -1.0; }";
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(2.0)]), 1.0, 1e-5);
}

#[test]
fn interp_call_user_func() {
    let glsl =
        "float double_(float x) { return x * 2.0; } float f(float x) { return double_(x) + 1.0; }";
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(3.0)]), 7.0, 1e-5);
}

#[test]
fn interp_abs_float() {
    let glsl = "float f(float x) { return abs(x); }";
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(-2.5)]), 2.5, 1e-5);
}

#[test]
fn interp_abs_int() {
    let glsl = "int f(int x) { return abs(x); }";
    assert_eq!(run_i32(glsl, "f", &[Value::I32(-4)]), 4);
}

#[test]
fn interp_sqrt() {
    let glsl = "float f(float x) { return sqrt(x); }";
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(4.0)]), 2.0, 1e-5);
}

#[test]
fn interp_mix() {
    let glsl = "float f(float a, float b, float t) { return mix(a, b, t); }";
    assert_f32_close(
        run_f32(
            glsl,
            "f",
            &[Value::F32(0.0), Value::F32(10.0), Value::F32(0.25)],
        ),
        2.5,
        1e-5,
    );
}

#[test]
fn interp_sin_std_math() {
    let glsl = "float f(float x) { return sin(x); }";
    assert!(run_f32(glsl, "f", &[Value::F32(0.0)]).abs() < 1e-5);
}

#[test]
fn interp_pow() {
    let glsl = "float f(float x, float y) { return pow(x, y); }";
    assert_f32_close(
        run_f32(glsl, "f", &[Value::F32(2.0), Value::F32(3.0)]),
        8.0,
        1e-4,
    );
}

#[test]
fn interp_lpfx_saturate() {
    let glsl = "float f(float x) { return lpfx_saturate(x); }";
    let mut h = CombinedImports::default();
    let ir = compile_and_lower(glsl);
    let out = interpret(&ir, "f", &[Value::F32(1.5)], &mut h).expect("interp");
    assert_f32_close(out[0].as_f32().expect("f32"), 1.0, 1e-5);
}

fn compile_and_lower(glsl: &str) -> lpir::IrModule {
    let naga = compile(glsl).expect("compile");
    let ir = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate lowered IR");
    ir
}

fn run(glsl: &str, func: &str, args: &[Value]) -> Vec<Value> {
    let module = compile_and_lower(glsl);
    let mut h = StdMathHandler;
    interpret(&module, func, args, &mut h).expect("interpret")
}

fn run_f32(glsl: &str, func: &str, args: &[Value]) -> f32 {
    run(glsl, func, args)[0].as_f32().expect("f32 result")
}

fn run_i32(glsl: &str, func: &str, args: &[Value]) -> i32 {
    run(glsl, func, args)[0].as_i32().expect("i32 result")
}

fn assert_f32_close(actual: f32, expected: f32, epsilon: f32) {
    assert!(
        (actual - expected).abs() < epsilon,
        "expected {expected}, got {actual}"
    );
}

#[derive(Default)]
struct CombinedImports {
    math: StdMathHandler,
}

impl ImportHandler for CombinedImports {
    fn call(
        &mut self,
        module_name: &str,
        func_name: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, InterpError> {
        if module_name == "lpfx" && func_name.starts_with("lpfx_saturate_") {
            let x = args[0]
                .as_f32()
                .ok_or_else(|| InterpError::Import("expected f32".into()))?;
            return Ok(vec![Value::F32(x.max(0.0).min(1.0))]);
        }
        self.math.call(module_name, func_name, args)
    }
}
