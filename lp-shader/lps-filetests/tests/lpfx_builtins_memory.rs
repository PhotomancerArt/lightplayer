//! Host-side checks for LPFX builtins and scratch memory.

use lps_filetests::test_run::wasm_link::{builtins_wasm_path, instantiate_wasm_module};
use lps_frontend::FloatMode;
use lps_wasm::{WasmOptions, glsl_wasm};
use wasmtime::{Engine, Instance, Memory, MemoryType, Module, Store, Val};

#[test]
fn lpfx_saturate_vec3_q32_writes_shared_memory() {
    let builtins_path = builtins_wasm_path();
    let builtins_bytes = std::fs::read(&builtins_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", builtins_path.display()));

    let engine = Engine::default();
    let mut store = Store::new(&engine, ());
    let builtins_mod = Module::new(&engine, &builtins_bytes).expect("parse builtins wasm");
    // Must match builtins wasm `env.memory` import minimum (see `wasm_link::shared_env_memory_type`).
    let memory = Memory::new(&mut store, MemoryType::new(17, None)).expect("memory");
    let builtins_inst =
        Instance::new(&mut store, &builtins_mod, &[memory.clone().into()]).expect("instantiate");

    let f = builtins_inst
        .get_func(&mut store, "__lp_lpfx_saturate_vec3_q32")
        .expect("export __lp_lpfx_saturate_vec3_q32");

    let base = 65536usize;
    for b in memory.data_mut(&mut store)[base..base + 16].iter_mut() {
        *b = 0xAB;
    }

    f.call(
        &mut store,
        &[
            Val::I32(base as i32),
            Val::I32(-32768),
            Val::I32(32768),
            Val::I32(98304),
        ],
        &mut [],
    )
    .expect("call saturate vec3");

    let data = memory.data(&store);
    let read_i32 = |off: usize| i32::from_le_bytes(data[off..off + 4].try_into().unwrap());

    assert_eq!(read_i32(base), 0, "saturated x");
    assert_eq!(read_i32(base + 4), 32768, "saturated y (0.5)");
    assert_eq!(read_i32(base + 8), 65536, "saturated z (1.0)");
}

#[test]
#[ignore = "WASM import ABI mismatch for vec3 LPFX (multi-return vs result-pointer). See docs/roadmaps/2026-03-25-lpir-features/"]
fn shader_lpfx_saturate_vec3_writes_scratch_then_reads_it() {
    let src = r#"
float test_get_rx() {
    vec3 v = vec3(-0.5, 0.5, 1.5);
    vec3 r = lpfx_saturate(v);
    return r.x;
}
float test_only_x_valid() {
    vec3 v = vec3(-0.5, 0.5, 1.5);
    vec3 result = lpfx_saturate(v);
    return abs(result.x - 0.0) < 0.01 ? 1.0 : 0.0;
}
float test_xy_valid() {
    vec3 v = vec3(-0.5, 0.5, 1.5);
    vec3 result = lpfx_saturate(v);
    bool valid = abs(result.x - 0.0) < 0.01 && abs(result.y - 0.5) < 0.01;
    return valid ? 1.0 : 0.0;
}
float test_lpfx_saturate_vec3() {
    vec3 v = vec3(-0.5, 0.5, 1.5);
    vec3 result = lpfx_saturate(v);
    bool valid = abs(result.x - 0.0) < 0.01 &&
                 abs(result.y - 0.5) < 0.01 &&
                 abs(result.z - 1.0) < 0.01;
    return valid ? 1.0 : 0.0;
}
"#;
    let compiled = glsl_wasm(
        src,
        WasmOptions {
            float_mode: FloatMode::Q32,
        },
    )
    .expect("compile");
    let engine = Engine::default();
    let mut store = Store::new(&engine, ());
    let (instance, mem) =
        instantiate_wasm_module(&engine, &mut store, &compiled.bytes).expect("instantiate");
    let memory = mem.expect("builtins-linked memory");

    let get_rx = instance
        .get_typed_func::<(), i32>(&mut store, "test_get_rx")
        .expect("test_get_rx");
    let rx = get_rx.call(&mut store, ()).expect("get_rx");
    assert_eq!(rx, 0, "r.x after saturate (q32)");

    let only_x = instance
        .get_typed_func::<(), i32>(&mut store, "test_only_x_valid")
        .expect("test_only_x_valid");
    assert_eq!(
        only_x.call(&mut store, ()).expect("only_x"),
        65536,
        "single abs cmp"
    );

    let xy = instance
        .get_typed_func::<(), i32>(&mut store, "test_xy_valid")
        .expect("test_xy_valid");
    assert_eq!(xy.call(&mut store, ()).expect("xy"), 65536, "two cmps &&");

    let f = instance
        .get_typed_func::<(), i32>(&mut store, "test_lpfx_saturate_vec3")
        .expect("export");

    let base = 65536usize;
    for b in memory.data_mut(&mut store)[base..base + 12].iter_mut() {
        *b = 0;
    }

    let ret = f.call(&mut store, ()).expect("call");
    let data = memory.data(&store);
    let read_i32 = |off: usize| i32::from_le_bytes(data[off..off + 4].try_into().unwrap());

    assert_eq!(read_i32(base), 0, "scratch x after call");
    assert_eq!(read_i32(base + 4), 32768, "scratch y after call");
    assert_eq!(read_i32(base + 8), 65536, "scratch z after call");
    assert_eq!(ret, 65536, "q32 1.0 return when valid");
}
