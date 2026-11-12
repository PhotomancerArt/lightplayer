//! GLSL → LPIR → `compile_lpir` → wasmtime validation (no `runtime` feature).

use lpvm_wasm::{FloatMode, WasmOptions, compile_lpir};
use wasmtime::Engine;

#[test]
fn wasmtime_accepts_emitted_int_add() {
    let src = "int add(int a, int b) { return a + b; }";
    let output =
        lps_glsl::compile(src, &lps_glsl::CompileOptions::default()).expect("compile glsl");
    let (ir, meta) = (output.ir, output.meta);
    let art = compile_lpir(&ir, &meta, &WasmOptions::default()).expect("emit");
    let engine = Engine::default();
    wasmtime::Module::new(&engine, art.bytes()).expect("wasm validate");
}

#[test]
fn wasmtime_accepts_emitted_float_add_f32() {
    let src = "float add(float a, float b) { return a + b; }";
    let output =
        lps_glsl::compile(src, &lps_glsl::CompileOptions::default()).expect("compile glsl");
    let (ir, meta) = (output.ir, output.meta);
    let opts = WasmOptions {
        float_mode: FloatMode::F32,
        ..Default::default()
    };
    let art = compile_lpir(&ir, &meta, &opts).expect("emit");
    let engine = Engine::default();
    wasmtime::Module::new(&engine, art.bytes()).expect("wasm validate");
}
