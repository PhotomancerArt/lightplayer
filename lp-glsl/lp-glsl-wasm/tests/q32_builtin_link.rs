//! Instantiate Q32 shader modules with `lp_glsl_builtins_wasm.wasm` + shared linear memory.
//!
//! Requires: `cargo build -p lp-glsl-builtins-wasm --target wasm32-unknown-unknown --release`
//! Output: `target/wasm32-unknown-unknown/release/lp_glsl_builtins_wasm.wasm` from repo root.
//! Override path with `LP_GLSL_BUILTINS_WASM`.

use std::path::{Path, PathBuf};
use std::string::String;
use std::vec::Vec;

use lp_glsl_wasm::{WasmOptions, glsl_wasm};
use wasmtime::{Engine, ExternType, Func, Instance, Linker, Memory, MemoryType, Module, Store};

fn builtins_wasm_path() -> PathBuf {
    if let Ok(p) = std::env::var("LP_GLSL_BUILTINS_WASM") {
        return PathBuf::from(p);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/release/lp_glsl_builtins_wasm.wasm")
}

/// Memory type satisfying every `env.memory` import (builtins + shader).
fn shared_env_memory_type(builtins: &Module, shader: &Module) -> anyhow::Result<MemoryType> {
    let mut min_pages: u64 = 0;
    let mut max_cap: Option<u64> = None;
    for module in [builtins, shader] {
        for imp in module.imports() {
            if imp.module() == "env" && imp.name() == "memory" {
                let ExternType::Memory(mt) = imp.ty() else {
                    anyhow::bail!("env.memory import is not a memory type");
                };
                if mt.is_64() || mt.is_shared() {
                    anyhow::bail!("env.memory must be 32-bit non-shared for this linker path");
                }
                min_pages = min_pages.max(mt.minimum());
                max_cap = match (max_cap, mt.maximum()) {
                    (None, None) => None,
                    (None, Some(b)) => Some(b),
                    (Some(a), None) => Some(a),
                    (Some(a), Some(b)) => Some(a.min(b)),
                };
            }
        }
    }
    let min_u32 = u32::try_from(min_pages)
        .map_err(|_| anyhow::anyhow!("env.memory minimum pages do not fit in u32"))?;
    let max_u32 = max_cap
        .map(|m| u32::try_from(m))
        .transpose()
        .map_err(|_| anyhow::anyhow!("env.memory maximum pages do not fit in u32"))?;
    Ok(MemoryType::new(min_u32, max_u32))
}

fn link_q32_shader(
    engine: &Engine,
    shader_bytes: &[u8],
    builtins_bytes: &[u8],
) -> anyhow::Result<(Store<()>, Instance)> {
    let builtins_mod = Module::new(engine, builtins_bytes).map_err(|e| {
        anyhow::anyhow!("builtins Module::new: {e}\n(hint: build with cargo build -p lp-glsl-builtins-wasm --target wasm32-unknown-unknown --release)")
    })?;
    let shader_mod = Module::new(engine, shader_bytes)?;

    let mut store = Store::new(engine, ());
    let memory_ty = shared_env_memory_type(&builtins_mod, &shader_mod)?;
    let memory = Memory::new(&mut store, memory_ty)
        .map_err(|e| anyhow::anyhow!("Memory::new failed: {e}"))?;

    let builtins_inst = Instance::new(&mut store, &builtins_mod, &[memory.clone().into()])
        .map_err(|e| {
            anyhow::anyhow!("builtins Instance::new failed (needs env.memory import): {e}")
        })?;

    let mut linker = Linker::new(engine);
    linker
        .define(&mut store, "env", "memory", memory.clone())
        .map_err(|e| anyhow::anyhow!("linker env.memory: {e}"))?;

    let builtin_funcs: Vec<(String, Func)> = builtins_inst
        .exports(&mut store)
        .filter_map(|export| {
            let name = export.name().to_string();
            export.into_func().map(|f| (name, f))
        })
        .collect();

    for (name, func) in builtin_funcs {
        linker
            .define(&mut store, "builtins", &name, func)
            .map_err(|e| anyhow::anyhow!("linker builtins.{name}: {e}"))?;
    }

    let shader_inst = linker
        .instantiate(&mut store, &shader_mod)
        .map_err(|e| anyhow::anyhow!("shader instantiate: {e}"))?;

    Ok((store, shader_inst))
}

#[test]
fn test_q32_sin_linked_execution() {
    let builtins_path = builtins_wasm_path();
    assert!(
        builtins_path.is_file(),
        "missing builtins wasm at {}\n\
         build: cargo build -p lp-glsl-builtins-wasm --target wasm32-unknown-unknown --release\n\
         or set LP_GLSL_BUILTINS_WASM",
        builtins_path.display()
    );

    let builtins_bytes = std::fs::read(&builtins_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", builtins_path.display()));

    let source = r#"
        float main() {
            return sin(1.0);
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile shader");
    assert!(!module.bytes.is_empty());

    let engine = Engine::default();
    let (mut store, instance) =
        link_q32_shader(&engine, &module.bytes, &builtins_bytes).expect("link");

    let func = instance.get_func(&mut store, "main").expect("export main");
    let typed = func
        .typed::<(), i32>(&store)
        .expect("main signature () -> i32");

    let got = typed.call(&mut store, ()).expect("call main");
    let want = (1.0_f64.sin() * 65536.0).round() as i32;
    assert!(
        (got - want).abs() <= 64,
        "sin(1.0) Q32: got {got} want ~{want} (float ref ±1 ulp slack)"
    );
}

fn q32_from_float(f: f64) -> i32 {
    (f * 65536.0).round() as i32
}

#[test]
fn test_lpfx_worley_linked() {
    let builtins_path = builtins_wasm_path();
    assert!(
        builtins_path.is_file(),
        "missing builtins wasm at {}",
        builtins_path.display()
    );
    let builtins_bytes = std::fs::read(&builtins_path).expect("read builtins");

    let source = r#"
        float main() {
            return lpfx_worley(vec2(1.0, 2.0), 0u);
        }
    "#;
    let module = glsl_wasm(source, WasmOptions::default()).expect("compile");
    let engine = Engine::default();
    let (mut store, instance) =
        link_q32_shader(&engine, &module.bytes, &builtins_bytes).expect("link");

    let func = instance.get_func(&mut store, "main").expect("main");
    let typed = func.typed::<(), i32>(&store).expect("sig");
    let _ = typed.call(&mut store, ()).expect("call should not trap");
}

#[test]
fn test_lpfx_psrdnoise_linked_writes_gradient() {
    let builtins_path = builtins_wasm_path();
    assert!(
        builtins_path.is_file(),
        "missing builtins wasm at {}",
        builtins_path.display()
    );
    let builtins_bytes = std::fs::read(&builtins_path).expect("read builtins");

    let source = r#"
        float main() {
            vec2 g;
            return lpfx_psrdnoise(vec2(0.5, 0.25), vec2(0.0), 0.0, g, 0u);
        }
    "#;
    let module = glsl_wasm(source, WasmOptions::default()).expect("compile");
    let engine = Engine::default();
    let (mut store, instance) =
        link_q32_shader(&engine, &module.bytes, &builtins_bytes).expect("link");

    let func = instance.get_func(&mut store, "main").expect("main");
    let typed = func.typed::<(), i32>(&store).expect("sig");
    let got = typed.call(&mut store, ()).expect("call");
    assert!(got.abs() < 1_000_000, "psrdnoise scalar sanity, got {got}");
}

#[test]
fn test_rainbow_shader_main_linked() {
    let builtins_path = builtins_wasm_path();
    assert!(
        builtins_path.is_file(),
        "missing builtins wasm at {}",
        builtins_path.display()
    );
    let builtins_bytes = std::fs::read(&builtins_path).expect("read builtins");

    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/basic/src/rainbow.shader/main.glsl");
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let module = glsl_wasm(&src, WasmOptions::default()).expect("compile rainbow");
    let engine = Engine::default();
    let (mut store, instance) =
        link_q32_shader(&engine, &module.bytes, &builtins_bytes).expect("link rainbow");

    let func = instance.get_func(&mut store, "main").expect("main");
    let mut results = [
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
    ];
    let args = [
        wasmtime::Val::I32(q32_from_float(100.0)),
        wasmtime::Val::I32(q32_from_float(100.0)),
        wasmtime::Val::I32(q32_from_float(200.0)),
        wasmtime::Val::I32(q32_from_float(200.0)),
        wasmtime::Val::I32(q32_from_float(1.0)),
    ];
    func.call(&mut store, &args, &mut results)
        .expect("main(vec2, vec2, float) should not trap");

    for (i, r) in results.iter().enumerate() {
        let wasmtime::Val::I32(v) = r else {
            panic!("result {i} not i32");
        };
        assert!(v.abs() < 1_000_000_000, "result[{i}]={v} out of range");
    }
}

#[test]
fn test_scaled_coord_differs_on_row_under_linker() {
    let builtins_path = builtins_wasm_path();
    assert!(
        builtins_path.is_file(),
        "missing builtins wasm at {}",
        builtins_path.display()
    );
    let builtins_bytes = std::fs::read(&builtins_path).expect("read builtins");

    let source = r#"
        vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
            vec2 center = outputSize * 0.5;
            vec2 dir = fragCoord - center;
            float scale = 0.05;
            vec2 scaledCoord = center + dir * scale;
            return vec4(scaledCoord.x, scaledCoord.y, 0.0, 1.0);
        }
    "#;
    let module = glsl_wasm(source, WasmOptions::default()).expect("compile");
    let engine = Engine::default();
    let (mut store, instance) =
        link_q32_shader(&engine, &module.bytes, &builtins_bytes).expect("link");

    let func = instance.get_func(&mut store, "main").expect("main");
    let wx = 64i32 << 16;
    let wy = 64i32 << 16;
    let mut a = [
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
    ];
    let mut b = [
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
    ];
    let args_left = [
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(32 << 16),
        wasmtime::Val::I32(wx),
        wasmtime::Val::I32(wy),
        wasmtime::Val::I32(0),
    ];
    let args_right = [
        wasmtime::Val::I32(32 << 16),
        wasmtime::Val::I32(32 << 16),
        wasmtime::Val::I32(wx),
        wasmtime::Val::I32(wy),
        wasmtime::Val::I32(0),
    ];
    func.call(&mut store, &args_left, &mut a).expect("call");
    func.call(&mut store, &args_right, &mut b).expect("call");
    let ax = match a[0] {
        wasmtime::Val::I32(v) => v,
        _ => panic!("r"),
    };
    let bx = match b[0] {
        wasmtime::Val::I32(v) => v,
        _ => panic!("r"),
    };
    assert_ne!(
        ax, bx,
        "scaledCoord.x at (0,32) vs (32,32): {ax} vs {bx} (vec2 fragCoord math broken?)"
    );
}

#[test]
fn test_rainbow_same_row_diff_x_differs() {
    let builtins_path = builtins_wasm_path();
    assert!(
        builtins_path.is_file(),
        "missing builtins wasm at {}",
        builtins_path.display()
    );
    let builtins_bytes = std::fs::read(&builtins_path).expect("read builtins");

    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/basic/src/rainbow.shader/main.glsl");
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let module = glsl_wasm(&src, WasmOptions::default()).expect("compile rainbow");
    let engine = Engine::default();
    let (mut store, instance) =
        link_q32_shader(&engine, &module.bytes, &builtins_bytes).expect("link rainbow");

    let func = instance.get_func(&mut store, "main").expect("main");
    let wx = 64i32 << 16;
    let wy = 64i32 << 16;
    let t = q32_from_float(1.0);

    let mut a = [
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
    ];
    let mut b = [
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(0),
    ];

    let args_left = [
        wasmtime::Val::I32(0),
        wasmtime::Val::I32(32 << 16),
        wasmtime::Val::I32(wx),
        wasmtime::Val::I32(wy),
        wasmtime::Val::I32(t),
    ];
    let args_right = [
        wasmtime::Val::I32(32 << 16),
        wasmtime::Val::I32(32 << 16),
        wasmtime::Val::I32(wx),
        wasmtime::Val::I32(wy),
        wasmtime::Val::I32(t),
    ];

    func.call(&mut store, &args_left, &mut a)
        .expect("main left pixel should not trap");
    func.call(&mut store, &args_right, &mut b)
        .expect("main right pixel should not trap");

    let ai: Vec<i32> = a
        .iter()
        .map(|v| match v {
            wasmtime::Val::I32(i) => *i,
            _ => panic!("expected i32 results"),
        })
        .collect();
    let bi: Vec<i32> = b
        .iter()
        .map(|v| match v {
            wasmtime::Val::I32(i) => *i,
            _ => panic!("expected i32 results"),
        })
        .collect();
    assert_ne!(
        ai, bi,
        "rainbow output at (0,32) vs (32,32) must differ; if equal, fragCoord.x is not affecting the shader (web demo horizontal bands)"
    );
}
