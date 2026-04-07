//! Runtime test: shader calls `sin` via natively linked `lps-builtins`.

use std::f32::consts::FRAC_PI_2;

use lps_frontend::{compile, lower};
use lpvm::{LpsValueF32, LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;
use lpvm_wasm::{FloatMode, WasmOptions};

#[test]
fn call_sin_q32_linked_builtins() {
    let src = "float f(float x) { return sin(x); }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");
    let opts = WasmOptions {
        float_mode: FloatMode::Q32,
    };
    let engine = WasmLpvmEngine::new(opts).expect("engine");
    let module = engine.compile(&ir, &meta).expect("compile");
    let mut inst = module.instantiate().expect("instantiate");

    let LpsValueF32::F32(z) = inst.call("f", &[LpsValueF32::F32(0.0)]).expect("sin(0)") else {
        panic!("expected F32");
    };
    assert!(z.abs() < 0.02, "sin(0) ≈ 0, got {z}");

    let LpsValueF32::F32(one) = inst
        .call("f", &[LpsValueF32::F32(FRAC_PI_2)])
        .expect("sin(pi/2)")
    else {
        panic!("expected F32");
    };
    assert!((one - 1.0).abs() < 0.04, "sin(pi/2) ≈ 1, got {one}");
}
