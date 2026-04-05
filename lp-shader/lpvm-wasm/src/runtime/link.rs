//! Link shader WASM with `lps_builtins_wasm.wasm` and shared `env.memory`.

use std::path::{Path, PathBuf};

use wasmtime::{Engine, ExternType, Func, Instance, Linker, Memory, MemoryType, Module, Store};

use crate::error::WasmError;

/// Path to `lps_builtins_wasm.wasm`. Override with `lps_BUILTINS_WASM`.
pub fn builtins_wasm_path() -> PathBuf {
    if let Ok(p) = std::env::var("lps_BUILTINS_WASM") {
        return PathBuf::from(p);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/release/lps_builtins_wasm.wasm")
}

fn shared_env_memory_type(builtins: &Module, shader: &Module) -> Result<MemoryType, WasmError> {
    let mut min_pages: u64 = 0;
    let mut max_cap: Option<u64> = None;
    for module in [builtins, shader] {
        for imp in module.imports() {
            if imp.module() == "env" && imp.name() == "memory" {
                let ExternType::Memory(mt) = imp.ty() else {
                    return Err(WasmError::runtime("env.memory import is not a memory type"));
                };
                if mt.is_64() || mt.is_shared() {
                    return Err(WasmError::runtime(
                        "env.memory must be 32-bit non-shared for this linker path",
                    ));
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
        .map_err(|_| WasmError::runtime("env.memory minimum pages overflow u32"))?;
    let max_u32 = max_cap
        .map(|m| u32::try_from(m))
        .transpose()
        .map_err(|_| WasmError::runtime("env.memory maximum overflow u32"))?;
    Ok(MemoryType::new(min_u32, max_u32))
}

pub(crate) fn module_needs_builtins_link(shader: &Module) -> bool {
    shader
        .imports()
        .any(|imp| imp.module() == "builtins" || (imp.module() == "env" && imp.name() == "memory"))
}

/// Instantiate a compiled shader module, linking builtins + `env.memory` when imports require it.
pub(crate) fn instantiate_wasm_module(
    engine: &Engine,
    store: &mut Store<()>,
    wasm_bytes: &[u8],
    builtins_wasm: &[u8],
) -> Result<(Instance, Option<Memory>), WasmError> {
    let shader_mod = Module::new(engine, wasm_bytes)
        .map_err(|e| WasmError::runtime(format!("WASM parse: {e:#}")))?;

    if !module_needs_builtins_link(&shader_mod) {
        let instance = Instance::new(&mut *store, &shader_mod, &[])
            .map_err(|e| WasmError::runtime(format!("WASM instantiate: {e}")))?;
        return Ok((instance, None));
    }

    let builtins_mod = Module::new(engine, builtins_wasm)
        .map_err(|e| WasmError::runtime(format!("builtins WASM parse: {e}")))?;

    let memory_ty = shared_env_memory_type(&builtins_mod, &shader_mod)?;
    let memory = Memory::new(&mut *store, memory_ty)
        .map_err(|e| WasmError::runtime(format!("Memory::new: {e}")))?;

    let builtins_inst = Instance::new(&mut *store, &builtins_mod, &[memory.into()])
        .map_err(|e| WasmError::runtime(format!("builtins instantiate: {e}")))?;

    let mut linker = Linker::new(engine);
    linker
        .define(&mut *store, "env", "memory", memory)
        .map_err(|e| WasmError::runtime(format!("linker env.memory: {e}")))?;

    let builtin_funcs: Vec<(String, Func)> = builtins_inst
        .exports(&mut *store)
        .filter_map(|export| {
            let name = export.name().to_string();
            export.into_func().map(|f| (name, f))
        })
        .collect();

    for (name, func) in builtin_funcs {
        linker
            .define(&mut *store, "builtins", &name, func)
            .map_err(|e| WasmError::runtime(format!("linker builtins.{name}: {e}")))?;
    }

    let instance = linker
        .instantiate(&mut *store, &shader_mod)
        .map_err(|e| WasmError::runtime(format!("shader instantiate: {e}")))?;
    Ok((instance, Some(memory)))
}
