//! One engine, shared linear memory: multiple instances and host bump allocations.

use lps_frontend::{compile, lower};
use lpvm::{AllocError, LpsValueF32, LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;
use lpvm_wasm::{FloatMode, WasmOptions};

#[test]
fn two_instances_same_engine_share_memory() {
    let src = "float add(float a, float b) { return a + b; }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");
    let opts = WasmOptions {
        float_mode: FloatMode::Q32,
    };
    let engine = WasmLpvmEngine::new(opts).expect("engine");
    let module = engine.compile(&ir, &meta).expect("compile");

    let mut a = module.instantiate().expect("instantiate a");
    let mut b = module.instantiate().expect("instantiate b");

    let ra = a
        .call("add", &[LpsValueF32::F32(1.0), LpsValueF32::F32(2.0)])
        .expect("call a");
    let rb = b
        .call("add", &[LpsValueF32::F32(10.0), LpsValueF32::F32(20.0)])
        .expect("call b");

    let LpsValueF32::F32(x) = ra else {
        panic!("expected F32");
    };
    let LpsValueF32::F32(y) = rb else {
        panic!("expected F32");
    };
    assert!((x - 3.0).abs() < 0.02);
    assert!((y - 30.0).abs() < 0.2);
}

#[test]
fn engine_memory_alloc_smoke() {
    let src = "float add(float a, float b) { return a + b; }";
    let naga = compile(src).expect("parse");
    let (ir, meta) = lower(&naga).expect("lower");
    let opts = WasmOptions {
        float_mode: FloatMode::Q32,
    };
    let engine = WasmLpvmEngine::new(opts).expect("engine");
    let _module = engine.compile(&ir, &meta).expect("compile");

    // wasmtime path uses bump allocator with grow
    let p1 = engine.memory().alloc(64, 8).expect("alloc 1");
    let p2 = engine.memory().alloc(64, 8).expect("alloc 2");
    assert_ne!(p1.guest_base(), p2.guest_base());
    assert!(!p1.native_ptr().is_null());
    assert!(!p2.native_ptr().is_null());
}

#[test]
fn zero_alloc_errors() {
    let engine = WasmLpvmEngine::new(WasmOptions {
        float_mode: FloatMode::Q32,
    })
    .expect("engine");
    let err = engine.memory().alloc(0, 8).unwrap_err();
    assert_eq!(err, AllocError::InvalidSize);
}
