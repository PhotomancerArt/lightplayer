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

#[test]
fn call_array_mul2_q32_sret_roundtrip() {
    let src = r#"
float[4] mul2(float[4] a) {
    float[4] r;
    r[0] = a[0] * 2.0;
    r[1] = a[1] * 2.0;
    r[2] = a[2] * 2.0;
    r[3] = a[3] * 2.0;
    return r;
}
"#;
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");
    let opts = WasmOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    let engine = WasmLpvmEngine::new(opts).expect("engine");
    let module = engine.compile(&ir, &meta).expect("compile");
    let mut inst = module.instantiate().expect("instantiate");

    let arg = LpsValueF32::Array(
        vec![
            LpsValueF32::F32(1.0),
            LpsValueF32::F32(2.0),
            LpsValueF32::F32(3.0),
            LpsValueF32::F32(4.0),
        ]
        .into_boxed_slice(),
    );
    let out = inst.call("mul2", &[arg]).expect("call");
    let LpsValueF32::Array(items) = out else {
        panic!("expected Array return");
    };
    assert_eq!(items.len(), 4);
    for (i, want) in [(0, 2.0), (1, 4.0), (2, 6.0), (3, 8.0)] {
        let LpsValueF32::F32(got) = items[i] else {
            panic!("expected f32 at {i}");
        };
        assert!((got - want).abs() < 0.02, "elem {i}: got {got} want {want}");
    }
}
