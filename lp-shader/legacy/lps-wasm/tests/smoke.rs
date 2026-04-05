//! wasmtime smoke tests for the LPIR → WASM pipeline.

use lps_frontend::FloatMode;
use lps_wasm::{WasmOptions, glsl_wasm};

fn run_f32(source: &str, func_name: &str, args: &[f32]) -> f32 {
    let opts = WasmOptions {
        float_mode: FloatMode::F32,
    };
    let module = glsl_wasm(source, opts).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let wasm_mod = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &wasm_mod, &[]).expect("instantiate");
    let func = instance.get_func(&mut store, func_name).expect("get_func");
    let wasm_args: Vec<wasmtime::Val> = args
        .iter()
        .map(|a| wasmtime::Val::F32(a.to_bits()))
        .collect();
    let mut results = [wasmtime::Val::F32(0)];
    func.call(&mut store, &wasm_args, &mut results)
        .expect("call");
    match results[0] {
        wasmtime::Val::F32(bits) => f32::from_bits(bits),
        _ => panic!("expected f32"),
    }
}

fn run_q32_f32(source: &str, func_name: &str, args: &[f32]) -> f32 {
    const SCALE: f32 = 65536.0;
    let opts = WasmOptions {
        float_mode: FloatMode::Q32,
    };
    let module = glsl_wasm(source, opts).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let wasm_mod = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &wasm_mod, &[]).expect("instantiate");
    let func = instance.get_func(&mut store, func_name).expect("get_func");
    let wasm_args: Vec<wasmtime::Val> = args
        .iter()
        .map(|a| wasmtime::Val::I32((*a * SCALE) as i32))
        .collect();
    let mut results = [wasmtime::Val::I32(0)];
    func.call(&mut store, &wasm_args, &mut results)
        .expect("call");
    match results[0] {
        wasmtime::Val::I32(i) => i as f32 / SCALE,
        _ => panic!("expected i32"),
    }
}

fn run_q32_i32(source: &str, func_name: &str, args: &[i32]) -> i32 {
    let opts = WasmOptions {
        float_mode: FloatMode::Q32,
    };
    let module = glsl_wasm(source, opts).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let wasm_mod = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &wasm_mod, &[]).expect("instantiate");
    let func = instance.get_func(&mut store, func_name).expect("get_func");
    let wasm_args: Vec<wasmtime::Val> = args.iter().map(|a| wasmtime::Val::I32(*a)).collect();
    let mut results = [wasmtime::Val::I32(0)];
    func.call(&mut store, &wasm_args, &mut results)
        .expect("call");
    match results[0] {
        wasmtime::Val::I32(i) => i,
        _ => panic!("expected i32"),
    }
}

fn run_q32_f32_0(source: &str, func_name: &str) -> f32 {
    const SCALE: f32 = 65536.0;
    let opts = WasmOptions {
        float_mode: FloatMode::Q32,
    };
    let module = glsl_wasm(source, opts).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let wasm_mod = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &wasm_mod, &[]).expect("instantiate");
    let func = instance.get_func(&mut store, func_name).expect("get_func");
    let mut results = [wasmtime::Val::I32(0)];
    func.call(&mut store, &[], &mut results).expect("call");
    match results[0] {
        wasmtime::Val::I32(i) => i as f32 / SCALE,
        _ => panic!("expected i32"),
    }
}

#[test]
fn float_literal_return() {
    let v = run_f32("float f() { return 3.14; }", "f", &[]);
    assert!((v - 3.14).abs() < 0.01);
}

#[test]
fn float_add() {
    let v = run_f32(
        "float add(float a, float b) { return a + b; }",
        "add",
        &[1.5, 2.5],
    );
    assert!((v - 4.0).abs() < 0.001);
}

#[test]
fn int_add_typed() {
    let source = "int add(int a, int b) { return a + b; }";
    let opts = WasmOptions::default();
    let module = glsl_wasm(source, opts).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let wasm_mod = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &wasm_mod, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "add")
        .expect("get_func")
        .typed::<(i32, i32), i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, (3, 4)).expect("call"), 7);
}

#[test]
fn multiple_functions_exported() {
    let src = r#"
            float foo() { return 1.0; }
            float bar() { return 2.0; }
        "#;
    let opts = WasmOptions {
        float_mode: FloatMode::F32,
    };
    let module = glsl_wasm(src, opts).expect("compile");
    assert_eq!(module.exports.len(), 2);
}

#[test]
fn q32_add() {
    let v = run_q32_f32(
        "float add(float a, float b) { return a + b; }",
        "add",
        &[1.5, 2.5],
    );
    assert!((v - 4.0).abs() < 0.02);
}

#[test]
fn q32_mul() {
    let v = run_q32_f32(
        "float mul(float a, float b) { return a * b; }",
        "mul",
        &[2.0, 3.0],
    );
    assert!((v - 6.0).abs() < 0.03);
}

#[test]
fn q32_div() {
    let v = run_q32_f32(
        "float div(float a, float b) { return a / b; }",
        "div",
        &[6.0, 2.0],
    );
    assert!((v - 3.0).abs() < 0.03);
}

#[test]
fn q32_abs() {
    let v = run_q32_f32("float a(float x) { return abs(x); }", "a", &[-1.5]);
    assert!((v - 1.5).abs() < 0.03);
}

#[test]
fn q32_while_accumulates() {
    let v = run_q32_f32_0(
        "float f() { float s = 0.0; float i = 0.0; while (i < 4.0) { s = s + 1.0; i = i + 1.0; } return s; }",
        "f",
    );
    assert!((v - 4.0).abs() < 0.05);
}

#[test]
fn int_switch_dispatch() {
    let source = "int pick(int x) { switch (x) { case 0: return 10; case 1: return 20; default: return 99; } }";
    let opts = WasmOptions::default();
    let module = glsl_wasm(source, opts).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let wasm_mod = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &wasm_mod, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "pick")
        .expect("get_func")
        .typed::<i32, i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, 0).expect("call"), 10);
    assert_eq!(func.call(&mut store, 1).expect("call"), 20);
    assert_eq!(func.call(&mut store, 2).expect("call"), 99);
}

#[test]
fn q32_floor_and_ceil() {
    let flo = run_q32_f32("float f(float x) { return floor(x); }", "f", &[1.75]);
    assert!((flo - 1.0).abs() < 0.03);
    let cei = run_q32_f32("float g(float x) { return ceil(x); }", "g", &[1.25]);
    assert!((cei - 2.0).abs() < 0.03);
}

#[test]
fn q32_chained_float_compare_and() {
    let v = run_q32_f32_0(
        "float f() { float a = 1.0; float b = 0.5; return (a < 2.0 && b < 1.0) ? 1.0 : 0.0; }",
        "f",
    );
    assert!((v - 1.0).abs() < 0.02);
}

#[test]
fn q32_chained_float_compare_or() {
    let v = run_q32_f32_0(
        "float f() { float a = 3.0; float b = 0.5; return (a < 2.0 || b < 1.0) ? 1.0 : 0.0; }",
        "f",
    );
    assert!((v - 1.0).abs() < 0.02);
}

#[test]
fn q32_triple_float_compare_and() {
    let v = run_q32_f32_0(
        "float f() { float a = 1.0; float b = 0.5; float c = 0.25; return (a < 2.0 && b < 1.0 && c < 0.5) ? 1.0 : 0.0; }",
        "f",
    );
    assert!((v - 1.0).abs() < 0.02);
}

// --- Arithmetic ---

#[test]
fn q32_subtract() {
    let v = run_q32_f32(
        "float sub(float a, float b) { return a - b; }",
        "sub",
        &[5.0, 2.0],
    );
    assert!((v - 3.0).abs() < 0.02);
}

#[test]
fn q32_negate() {
    let v = run_q32_f32("float neg(float x) { return -x; }", "neg", &[3.5]);
    assert!((v - (-3.5)).abs() < 0.02);
}

#[test]
fn q32_int_modulo() {
    let v = run_q32_i32("int m(int a, int b) { return a % b; }", "m", &[7, 3]);
    assert_eq!(v, 1);
}

// --- Control flow ---

#[test]
fn q32_if_else() {
    let v = run_q32_f32(
        "float pick(float x) { float r = -1.0; if (x > 0.0) { r = 1.0; } return r; }",
        "pick",
        &[5.0],
    );
    assert!((v - 1.0).abs() < 0.02);
    let v2 = run_q32_f32(
        "float pick(float x) { float r = -1.0; if (x > 0.0) { r = 1.0; } return r; }",
        "pick",
        &[-5.0],
    );
    assert!((v2 - (-1.0)).abs() < 0.02);
}

#[test]
fn q32_if_else_both_return() {
    let v = run_q32_f32(
        "float f(float x) { if (x > 0.0) { return 1.0; } else { return 0.0; } return 0.0; }",
        "f",
        &[5.0],
    );
    assert!((v - 1.0).abs() < 0.02);
    let v2 = run_q32_f32(
        "float f(float x) { if (x > 0.0) { return 1.0; } else { return 0.0; } return 0.0; }",
        "f",
        &[-5.0],
    );
    assert!((v2 - 0.0).abs() < 0.02);
}

#[test]
fn q32_vec4_if_else_return_validates() {
    let src = r#"
        vec2 prsd_demo(vec2 a, float t) { return vec2(0.0, 1.0); }
        vec4 rainbow_main(vec2 fragCoord, vec2 outputSize, float time) {
            vec2 tv = prsd_demo(fragCoord, time);
            if (true) {
                return vec4(tv.x, tv.y, 0.0, 1.0);
            } else {
                return vec4(0.0, 0.0, 0.0, 1.0);
            }
        }
        float ok() { return 1.0; }
    "#;
    let module = glsl_wasm(
        src,
        WasmOptions {
            float_mode: FloatMode::Q32,
        },
    )
    .expect("compile");
    let engine = wasmtime::Engine::default();
    wasmtime::Module::new(&engine, &module.bytes).expect("wasm validate");
}

#[test]
fn q32_for_loop() {
    let v = run_q32_f32_0(
        "float f() { float s = 0.0; for (int i = 0; i < 5; i++) { s = s + 1.0; } return s; }",
        "f",
    );
    assert!((v - 5.0).abs() < 0.05);
}

// --- Math builtins ---

#[test]
fn q32_min_max() {
    let mn = run_q32_f32(
        "float mn(float a, float b) { return min(a, b); }",
        "mn",
        &[3.0, 1.5],
    );
    assert!((mn - 1.5).abs() < 0.02);
    let mx = run_q32_f32(
        "float mx(float a, float b) { return max(a, b); }",
        "mx",
        &[3.0, 1.5],
    );
    assert!((mx - 3.0).abs() < 0.02);
}

#[test]
fn q32_mix() {
    let v = run_q32_f32_0("float f() { return mix(0.0, 10.0, 0.5); }", "f");
    assert!((v - 5.0).abs() < 0.1);
}

#[test]
fn q32_clamp() {
    let v = run_q32_f32_0("float f() { return clamp(5.0, 0.0, 3.0); }", "f");
    assert!((v - 3.0).abs() < 0.05);
}

#[test]
fn q32_step() {
    let below = run_q32_f32(
        "float s(float edge, float x) { return step(edge, x); }",
        "s",
        &[1.0, 0.5],
    );
    assert!((below - 0.0).abs() < 0.02);
    let above = run_q32_f32(
        "float s(float edge, float x) { return step(edge, x); }",
        "s",
        &[1.0, 2.0],
    );
    assert!((above - 1.0).abs() < 0.02);
}

// --- Calls ---

#[test]
fn q32_call_user_func() {
    let v = run_q32_f32_0(
        "float dbl(float x) { return x + x; } float f() { return dbl(3.0); }",
        "f",
    );
    assert!((v - 6.0).abs() < 0.05);
}

#[test]
fn q32_call_chain() {
    let v = run_q32_f32_0(
        "float inc(float x) { return x + 1.0; } float dbl(float x) { return inc(inc(x)); } float f() { return dbl(1.0); }",
        "f",
    );
    assert!((v - 3.0).abs() < 0.05);
}

// --- Casts ---

#[test]
fn q32_float_to_int() {
    let v = run_q32_i32(
        "int f(float x) { return int(x); }",
        "f",
        &[3 << 16 | 0x8000],
    );
    assert_eq!(v, 3);
}

#[test]
fn q32_int_to_float() {
    const SCALE: f32 = 65536.0;
    let raw = run_q32_i32("float f(int x) { return float(x); }", "f", &[5]);
    let v = raw as f32 / SCALE;
    assert!((v - 5.0).abs() < 0.02);
}
