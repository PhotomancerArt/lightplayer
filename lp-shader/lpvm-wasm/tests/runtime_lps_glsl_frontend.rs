//! Runtime tests: `lps-glsl` frontend → LPIR → WASM.
//!
//! The naga-based `lps-frontend` pre-registers the `@lpir`/`@glsl` helper
//! imports that Q32 emission calls for `Fsqrt`/`Fnearest`, but `lps-glsl`
//! emits those LPIR ops directly with no import decls. These tests cover the
//! emitter synthesizing the missing helper imports (and the F32 path, which
//! lowers to native WASM instructions and needs no imports).

use lpvm::{LpsValueF32, LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_wasm::rt_wasmtime::{WasmLpvmEngine, WasmLpvmInstance};
use lpvm_wasm::{FloatMode, WasmOptions};

fn instantiate_lp_glsl(src: &str, float_mode: FloatMode) -> WasmLpvmInstance {
    let output =
        lps_glsl::compile(src, &lps_glsl::CompileOptions::default()).expect("compile glsl");
    let opts = WasmOptions {
        float_mode,
        ..Default::default()
    };
    let engine = WasmLpvmEngine::new(opts).expect("engine");
    let module = engine.compile(&output.ir, &output.meta).expect("compile");
    module.instantiate().expect("instantiate")
}

fn call_f32(inst: &mut WasmLpvmInstance, name: &str, args: &[f32]) -> f32 {
    let args: Vec<LpsValueF32> = args.iter().map(|&v| LpsValueF32::F32(v)).collect();
    let LpsValueF32::F32(v) = inst.call(name, &args).expect("call") else {
        panic!("expected F32 result");
    };
    v
}

#[test]
fn length_vec2_q32() {
    let src = "float f(float x, float y) { return length(vec2(x, y)); }";
    let mut inst = instantiate_lp_glsl(src, FloatMode::Q32);
    let v = call_f32(&mut inst, "f", &[3.0, 4.0]);
    assert!((v - 5.0).abs() < 0.01, "length(vec2(3,4)) ≈ 5, got {v}");
}

#[test]
fn length_vec2_f32() {
    let src = "float f(float x, float y) { return length(vec2(x, y)); }";
    let mut inst = instantiate_lp_glsl(src, FloatMode::F32);
    let v = call_f32(&mut inst, "f", &[3.0, 4.0]);
    assert!((v - 5.0).abs() < 1e-5, "length(vec2(3,4)) = 5, got {v}");
}

#[test]
fn sqrt_q32() {
    let src = "float f(float x) { return sqrt(x); }";
    let mut inst = instantiate_lp_glsl(src, FloatMode::Q32);
    let v = call_f32(&mut inst, "f", &[9.0]);
    assert!((v - 3.0).abs() < 0.01, "sqrt(9) ≈ 3, got {v}");
}

#[test]
fn round_even_q32() {
    let src = "float f(float x) { return roundEven(x); }";
    let mut inst = instantiate_lp_glsl(src, FloatMode::Q32);
    let v = call_f32(&mut inst, "f", &[2.6]);
    assert!((v - 3.0).abs() < 0.01, "roundEven(2.6) ≈ 3, got {v}");
}
