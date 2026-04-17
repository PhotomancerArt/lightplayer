//! `LpvmEngine` / `LpvmInstance` smoke test (no builtins required).

use lps_frontend::{compile, lower};
use lpvm::{LpsValueF32, LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;
use lpvm_wasm::{FloatMode, WasmOptions};

#[test]
fn call_float_add_q32_without_builtins() {
    let src = "float add(float a, float b) { return a + b; }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");
    let opts = WasmOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    let engine = WasmLpvmEngine::new(opts).expect("engine");
    let module = engine.compile(&ir, &meta).expect("compile");
    let mut inst = module.instantiate().expect("instantiate");
    let out = inst
        .call("add", &[LpsValueF32::F32(1.0), LpsValueF32::F32(2.0)])
        .expect("call");
    let LpsValueF32::F32(f) = out else {
        panic!("expected F32");
    };
    assert!((f - 3.0).abs() < 0.02);
}
