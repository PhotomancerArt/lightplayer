//! Regression: `WasmLpvmEngine::compile_with_config` actually flows
//! `lpir::CompilerConfig` into emission (not silently dropped).

use lpir::CompilerConfig;
use lps_frontend::{compile, lower};
use lps_q32::q32_options::MulMode;
use lpvm::LpvmEngine;
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

#[test]
fn compile_with_config_q32_mul_mode_flows_to_emission() {
    // One user function with a float multiply (not a void main with constants,
    // which can fold away before Fmul reaches emission).
    let glsl = "float mul(float a, float b) { return a * b; }";

    let naga = compile(glsl).expect("front-end compile");
    let (ir, meta) = lower(&naga).expect("lower");

    let engine = WasmLpvmEngine::new(WasmOptions::default()).expect("engine new");

    let mut cfg_a = CompilerConfig::default();
    let mut cfg_b = CompilerConfig::default();
    cfg_a.q32.mul = MulMode::Saturating;
    cfg_b.q32.mul = MulMode::Wrapping;

    let mod_a = engine
        .compile_with_config(&ir, &meta, &cfg_a)
        .expect("compile a");
    let mod_b = engine
        .compile_with_config(&ir, &meta, &cfg_b)
        .expect("compile b");

    assert_ne!(
        mod_a.wasm_bytes(),
        mod_b.wasm_bytes(),
        "compile_with_config must thread CompilerConfig into emission; \
         identical bytes mean per-call config was dropped"
    );
}
