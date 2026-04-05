//! GLSL → Naga → LPIR → interpreter (integration).

use lpir::{ImportHandler, InterpError, Value, interpret, validate_module};
use lps_frontend::std_math_handler::StdMathHandler;
use lps_frontend::{compile, lower};

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
fn interp_loop_sum() {
    let glsl =
        "int f(int n) { int s = 0; int i = 0; while (i < n) { s = s + i; i = i + 1; } return s; }";
    assert_eq!(run_i32(glsl, "f", &[Value::I32(4)]), 6);
}

#[test]
fn interp_for_loop_sum() {
    let glsl = "int f(int n) { int s = 0; for (int i = 0; i < n; i++) { s = s + i; } return s; }";
    assert_eq!(run_i32(glsl, "f", &[Value::I32(4)]), 6);
}

#[test]
fn interp_float_to_int() {
    let glsl = "int f(float x) { return int(x); }";
    assert_eq!(run_i32(glsl, "f", &[Value::F32(3.7)]), 3);
}

#[test]
fn interp_int_to_float() {
    let glsl = "float f(int x) { return float(x); }";
    assert_f32_close(run_f32(glsl, "f", &[Value::I32(-2)]), -2.0, 1e-5);
}

#[test]
fn interp_float_comparisons() {
    let glsl = "int f(float a, float b) { return int(a < b) + 2 * int(a <= b) + 4 * int(a > b) \
        + 8 * int(a >= b) + 16 * int(a == b) + 32 * int(a != b); }";
    assert_eq!(
        run_i32(glsl, "f", &[Value::F32(1.0), Value::F32(2.0)]),
        1 + 2 + 32
    );
    assert_eq!(
        run_i32(glsl, "f", &[Value::F32(2.0), Value::F32(2.0)]),
        2 + 8 + 16
    );
    assert_eq!(
        run_i32(glsl, "f", &[Value::F32(3.0), Value::F32(2.0)]),
        4 + 8 + 32
    );
}

#[test]
fn interp_int_comparisons() {
    let glsl = "int f(int a, int b) { return int(a < b) + 2 * int(a <= b) + 4 * int(a > b) \
        + 8 * int(a >= b) + 16 * int(a == b) + 32 * int(a != b); }";
    assert_eq!(
        run_i32(glsl, "f", &[Value::I32(1), Value::I32(2)]),
        1 + 2 + 32
    );
    assert_eq!(
        run_i32(glsl, "f", &[Value::I32(2), Value::I32(2)]),
        2 + 8 + 16
    );
    assert_eq!(
        run_i32(glsl, "f", &[Value::I32(3), Value::I32(2)]),
        4 + 8 + 32
    );
}

#[test]
fn interp_bool_literal() {
    let glsl = "bool f() { return true; } int g() { return int(f()); }";
    assert_eq!(run_i32(glsl, "g", &[]), 1);
}

#[test]
fn interp_nested_if() {
    let glsl =
        "float f(float x) { if (x > 0.0) { if (x > 5.0) return 2.0; return 1.0; } return 0.0; }";
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(-1.0)]), 0.0, 1e-5);
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(3.0)]), 1.0, 1e-5);
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(7.0)]), 2.0, 1e-5);
}

#[test]
fn interp_floor_ceil_trunc() {
    assert_f32_close(
        run_f32(
            "float f(float x) { return floor(x); }",
            "f",
            &[Value::F32(1.7)],
        ),
        1.0,
        1e-5,
    );
    assert_f32_close(
        run_f32(
            "float f(float x) { return ceil(x); }",
            "f",
            &[Value::F32(1.2)],
        ),
        2.0,
        1e-5,
    );
    assert_f32_close(
        run_f32(
            "float f(float x) { return trunc(x); }",
            "f",
            &[Value::F32(-1.7)],
        ),
        -1.0,
        1e-5,
    );
}

#[test]
fn interp_min_max_float() {
    let glsl = "float f(float a, float b) { return min(a, b) + max(a, b); }";
    assert_f32_close(
        run_f32(glsl, "f", &[Value::F32(2.0), Value::F32(5.0)]),
        7.0,
        1e-5,
    );
}

#[test]
fn interp_min_max_int() {
    let glsl = "int f(int a, int b) { return min(a, b) + max(a, b); }";
    assert_eq!(run_i32(glsl, "f", &[Value::I32(2), Value::I32(5)]), 7);
}

#[test]
fn interp_clamp() {
    let glsl = "float f(float x) { return clamp(x, 0.0, 1.0); }";
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(0.5)]), 0.5, 1e-5);
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(-1.0)]), 0.0, 1e-5);
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(2.0)]), 1.0, 1e-5);
}

#[test]
fn interp_sign() {
    assert_f32_close(
        run_f32(
            "float f(float x) { return sign(x); }",
            "f",
            &[Value::F32(3.0)],
        ),
        1.0,
        1e-5,
    );
    assert_f32_close(
        run_f32(
            "float f(float x) { return sign(x); }",
            "f",
            &[Value::F32(-2.0)],
        ),
        -1.0,
        1e-5,
    );
    assert_f32_close(
        run_f32(
            "float f(float x) { return sign(x); }",
            "f",
            &[Value::F32(0.0)],
        ),
        0.0,
        1e-5,
    );
}

#[test]
fn interp_step() {
    let glsl = "float f(float e, float x) { return step(e, x); }";
    assert_f32_close(
        run_f32(glsl, "f", &[Value::F32(1.0), Value::F32(0.5)]),
        0.0,
        1e-5,
    );
    assert_f32_close(
        run_f32(glsl, "f", &[Value::F32(1.0), Value::F32(1.0)]),
        1.0,
        1e-5,
    );
}

#[test]
fn interp_smoothstep() {
    let glsl = "float f(float x) { return smoothstep(0.0, 1.0, x); }";
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(0.0)]), 0.0, 1e-4);
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(1.0)]), 1.0, 1e-4);
}

#[test]
fn interp_fract() {
    assert_f32_close(
        run_f32(
            "float f(float x) { return fract(x); }",
            "f",
            &[Value::F32(2.25)],
        ),
        0.25,
        1e-5,
    );
}

#[test]
fn interp_fma() {
    let glsl = "float f(float a, float b, float c) { return fma(a, b, c); }";
    assert_f32_close(
        run_f32(
            glsl,
            "f",
            &[Value::F32(2.0), Value::F32(3.0), Value::F32(4.0)],
        ),
        10.0,
        1e-4,
    );
}

#[test]
fn interp_exp_log() {
    assert_f32_close(
        run_f32(
            "float f(float x) { return exp(x); }",
            "f",
            &[Value::F32(0.0)],
        ),
        1.0,
        1e-4,
    );
    assert_f32_close(
        run_f32(
            "float f(float x) { return log(x); }",
            "f",
            &[Value::F32(1.0)],
        ),
        0.0,
        1e-4,
    );
}

#[test]
fn interp_sin_cos() {
    assert_f32_close(
        run_f32(
            "float f(float x) { return sin(x); }",
            "f",
            &[Value::F32(0.0)],
        ),
        0.0,
        1e-4,
    );
    assert_f32_close(
        run_f32(
            "float f(float x) { return cos(x); }",
            "f",
            &[Value::F32(0.0)],
        ),
        1.0,
        1e-4,
    );
    let pi_2 = std::f32::consts::FRAC_PI_2;
    assert_f32_close(
        run_f32(
            "float f(float x) { return sin(x); }",
            "f",
            &[Value::F32(pi_2)],
        ),
        1.0,
        1e-3,
    );
}

#[test]
fn interp_call_chain() {
    let glsl = "float c(float x) { return x + 1.0; } float b(float x) { return c(x) * 2.0; } \
        float f(float x) { return b(x) - 1.0; }";
    assert_f32_close(run_f32(glsl, "f", &[Value::F32(1.0)]), 3.0, 1e-5);
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

#[test]
fn interp_vec3_compose_return() {
    let glsl = "vec3 f() { return vec3(1.0, 2.0, 4.0); }";
    let out = run(glsl, "f", &[]);
    assert_eq!(out.len(), 3, "vec3 return should yield 3 values");
    assert_f32_close(out[0].as_f32().expect("x"), 1.0, 1e-5);
    assert_f32_close(out[1].as_f32().expect("y"), 2.0, 1e-5);
    assert_f32_close(out[2].as_f32().expect("z"), 4.0, 1e-5);
}

#[test]
fn interp_vec3_param_and_swizzle() {
    let glsl = "float f(vec3 v) { vec3 t = v.zyx; return t.x + t.y; }";
    assert_f32_close(
        run_f32(
            glsl,
            "f",
            &[Value::F32(1.0), Value::F32(2.0), Value::F32(4.0)],
        ),
        4.0 + 2.0,
        1e-5,
    );
}

#[test]
fn interp_vec3_broadcast_mul() {
    let glsl = "vec3 f(vec3 v) { return v * 2.0; }";
    let out = run(
        glsl,
        "f",
        &[Value::F32(1.0), Value::F32(2.0), Value::F32(3.0)],
    );
    assert_eq!(out.len(), 3);
    assert_f32_close(out[0].as_f32().unwrap(), 2.0, 1e-5);
    assert_f32_close(out[1].as_f32().unwrap(), 4.0, 1e-5);
    assert_f32_close(out[2].as_f32().unwrap(), 6.0, 1e-5);
}

#[test]
fn interp_dot() {
    let glsl = "float f(vec3 a, vec3 b) { return dot(a, b); }";
    assert_f32_close(
        run_f32(
            glsl,
            "f",
            &[
                Value::F32(1.0),
                Value::F32(2.0),
                Value::F32(3.0),
                Value::F32(0.0),
                Value::F32(1.0),
                Value::F32(0.0),
            ],
        ),
        2.0,
        1e-5,
    );
}

#[test]
fn interp_mix_vec_scalar_t() {
    let glsl = "vec3 f(vec3 a, vec3 b) { return mix(a, b, 0.5); }";
    let out = run(
        glsl,
        "f",
        &[
            Value::F32(0.0),
            Value::F32(0.0),
            Value::F32(0.0),
            Value::F32(2.0),
            Value::F32(4.0),
            Value::F32(6.0),
        ],
    );
    assert_f32_close(out[0].as_f32().unwrap(), 1.0, 1e-5);
    assert_f32_close(out[1].as_f32().unwrap(), 2.0, 1e-5);
    assert_f32_close(out[2].as_f32().unwrap(), 3.0, 1e-5);
}

#[test]
fn interp_mat3_vec3_mul() {
    let glsl = "vec3 f() { mat3 m = mat3(2.0); return m * vec3(1.0, 1.0, 1.0); }";
    let out = run(glsl, "f", &[]);
    assert_eq!(out.len(), 3);
    assert_f32_close(out[0].as_f32().unwrap(), 2.0, 1e-4);
    assert_f32_close(out[1].as_f32().unwrap(), 2.0, 1e-4);
    assert_f32_close(out[2].as_f32().unwrap(), 2.0, 1e-4);
}

#[test]
fn interp_normalize_length() {
    let glsl = "float f() { return length(normalize(vec3(3.0, 0.0, 4.0))); }";
    assert_f32_close(run_f32(glsl, "f", &[]), 1.0, 1e-4);
}

#[test]
fn interp_transpose_mat2() {
    let glsl = "float f() { mat2 m = mat2(1.0, 2.0, 3.0, 4.0); mat2 t = transpose(m); return t[0][1] + t[1][0]; }";
    assert_f32_close(run_f32(glsl, "f", &[]), 3.0 + 2.0, 1e-4);
}

fn compile_and_lower(glsl: &str) -> lpir::IrModule {
    let naga = compile(glsl).expect("compile");
    let (ir, _) = lower(&naga).expect("lower");
    validate_module(&ir).expect("validate lowered IR");
    ir
}

fn run(glsl: &str, func: &str, args: &[Value]) -> Vec<Value> {
    let module = compile_and_lower(glsl);
    let mut h: StdMathHandler = Default::default();
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
