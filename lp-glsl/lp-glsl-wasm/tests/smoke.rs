//! wasmtime smoke tests for the Naga-based pipeline.

use lp_glsl_naga::FloatMode;
use lp_glsl_wasm::{WasmOptions, glsl_wasm};

fn run_f32(source: &str, func_name: &str, args: &[f32]) -> f32 {
    let opts = WasmOptions {
        float_mode: FloatMode::Float,
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
        float_mode: FloatMode::Float,
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
