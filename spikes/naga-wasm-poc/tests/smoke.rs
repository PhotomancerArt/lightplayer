//! Host-side execution with wasmtime (std). The library under test is `#![no_std]`.

use naga_wasm_poc::{NumericMode, compile};

const GLSL_ADD: &str = r#"
#version 450

float add_floats(float a, float b) {
    return a + b;
}

void main() {
}
"#;

#[test]
fn float_add_runs_in_wasmtime() {
    let r = compile(GLSL_ADD, "add_floats", NumericMode::Float).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &r.wasm_bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "add_floats")
        .expect("export")
        .typed::<(f32, f32), f32>(&store)
        .expect("typed f32");
    let out = func.call(&mut store, (1.5, 2.5)).expect("call");
    assert_eq!(out, 4.0);
}

#[test]
fn q32_add_runs_in_wasmtime() {
    let r = compile(GLSL_ADD, "add_floats", NumericMode::Q32).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &r.wasm_bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "add_floats")
        .expect("export")
        .typed::<(i32, i32), i32>(&store)
        .expect("typed i32");

    let a = (1.5f32 * 65536.0) as i32;
    let b = (2.5f32 * 65536.0) as i32;
    let out = func.call(&mut store, (a, b)).expect("call");
    assert_eq!(out, (4.0f32 * 65536.0) as i32);
}
