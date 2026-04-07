//! Link shader WASM with native `builtins` imports and shared `env.memory`.

use wasmtime::{Engine, ExternType, Func, Instance, Linker, Memory, Module, Store};

use lps_builtin_ids::BuiltinId;

use crate::error::WasmError;

use super::WasmLpvmSharedRuntime;
use super::native_builtin_dispatch::dispatch_native_builtin;

pub(crate) fn shader_env_memory_minimum_pages(shader: &Module) -> Option<u64> {
    shader.imports().find_map(|imp| {
        if imp.module() == "env" && imp.name() == "memory" {
            if let ExternType::Memory(mt) = imp.ty() {
                return Some(mt.minimum());
            }
        }
        None
    })
}

pub(crate) fn module_needs_builtins_link(shader: &Module) -> bool {
    shader
        .imports()
        .any(|imp| imp.module() == "builtins" || (imp.module() == "env" && imp.name() == "memory"))
}

fn link_builtins(
    linker: &mut Linker<()>,
    store: &mut Store<()>,
    shader_mod: &Module,
    env_memory_handle: Memory,
) -> Result<(), WasmError> {
    for imp in shader_mod.imports() {
        if imp.module() != "builtins" {
            continue;
        }
        let name = imp.name().to_string();
        let ExternType::Func(func_ty) = imp.ty() else {
            return Err(WasmError::runtime(format!(
                "builtins.{name}: expected function import"
            )));
        };

        let builtin_id = BuiltinId::builtin_id_from_name(&name).ok_or_else(|| {
            WasmError::runtime(format!("unknown builtin import: builtins.{name}"))
        })?;

        let func_ty = func_ty.to_owned();
        let name_for_closure = name.clone();
        let env_mem = env_memory_handle;
        let func = Func::new(&mut *store, func_ty, move |caller, params, results| {
            dispatch_native_builtin(caller, env_mem, builtin_id, params, results)
        });

        linker
            .define(&mut *store, "builtins", &name_for_closure, func)
            .map_err(|e| WasmError::runtime(format!("linker builtins.{name_for_closure}: {e}")))?;
    }
    Ok(())
}

/// Instantiate a compiled shader module, linking native builtins + shared `env.memory` when imports require it.
///
/// Uses the [`Memory`] already created in `runtime` (same store for every instance).
pub(crate) fn instantiate_wasm_module(
    engine: &Engine,
    runtime: &WasmLpvmSharedRuntime,
    wasm_bytes: &[u8],
) -> Result<Instance, WasmError> {
    let shader_mod = Module::new(engine, wasm_bytes)
        .map_err(|e| WasmError::runtime(format!("WASM parse: {e:#}")))?;

    let mut guard = runtime.lock();
    let memory = guard.memory;
    let store = &mut guard.store;

    if !module_needs_builtins_link(&shader_mod) {
        let instance = Instance::new(&mut *store, &shader_mod, &[])
            .map_err(|e| WasmError::runtime(format!("WASM instantiate: {e}")))?;
        return Ok(instance);
    }

    let mut linker = Linker::new(engine);
    linker
        .define(&mut *store, "env", "memory", memory)
        .map_err(|e| WasmError::runtime(format!("linker env.memory: {e}")))?;

    link_builtins(&mut linker, &mut *store, &shader_mod, memory)?;

    linker
        .instantiate(&mut *store, &shader_mod)
        .map_err(|e| WasmError::runtime(format!("shader instantiate: {e}")))
}
