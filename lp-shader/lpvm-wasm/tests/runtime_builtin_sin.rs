//! Runtime test: shader calls `sin` via linked `lps_builtins_wasm.wasm`.
//!
//! This target is only built with the `runtime` feature (enabled by default).
//!
//! Requires the builtins artifact (same as `lps-filetests`):
//! `cargo build -p lps-builtins-wasm --target wasm32-unknown-unknown --release`
//! or set `lps_BUILTINS_WASM` to the `.wasm` path.

use std::f32::consts::FRAC_PI_2;

use lps_frontend::{compile, lower};
use lpvm::{LpsValue, LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_wasm::runtime::{WasmLpvmEngine, link};
use lpvm_wasm::{FloatMode, WasmOptions};

fn builtins_engine(opts: WasmOptions) -> WasmLpvmEngine {
    let path = link::builtins_wasm_path();
    WasmLpvmEngine::try_default_builtins(opts).unwrap_or_else(|e| {
        panic!(
            "{e}\n\
             Build builtins: cargo build -p lps-builtins-wasm --target wasm32-unknown-unknown --release\n\
             Expected file: {}\n\
             Or set lps_BUILTINS_WASM to the wasm path.",
            path.display()
        )
    })
}

#[test]
fn call_sin_q32_linked_builtins() {
    let src = "float f(float x) { return sin(x); }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");
    let opts = WasmOptions {
        float_mode: FloatMode::Q32,
    };
    let engine = builtins_engine(opts);
    let module = engine.compile(&ir, &meta).expect("compile");
    let mut inst = module.instantiate().expect("instantiate");

    let LpsValue::F32(z) = inst.call("f", &[LpsValue::F32(0.0)]).expect("sin(0)") else {
        panic!("expected F32");
    };
    assert!(z.abs() < 0.02, "sin(0) ≈ 0, got {z}");

    let LpsValue::F32(one) = inst
        .call("f", &[LpsValue::F32(FRAC_PI_2)])
        .expect("sin(pi/2)")
    else {
        panic!("expected F32");
    };
    assert!((one - 1.0).abs() < 0.04, "sin(pi/2) ≈ 1, got {one}");
}
