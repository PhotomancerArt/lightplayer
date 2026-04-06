//! Link shader WASM with native `builtins` imports and shared `env.memory`.

use wasmtime::{Engine, ExternType, Func, Instance, Linker, Memory, MemoryType, Module, Store};

use lps_builtin_ids::BuiltinId;

use crate::error::WasmError;

use super::native_builtin_dispatch::dispatch_native_builtin;

fn shader_env_memory_type(shader: &Module) -> Result<MemoryType, WasmError> {
    for imp in shader.imports() {
        if imp.module() == "env" && imp.name() == "memory" {
            let ExternType::Memory(mt) = imp.ty() else {
                return Err(WasmError::runtime("env.memory import is not a memory type"));
            };
            if mt.is_64() || mt.is_shared() {
                return Err(WasmError::runtime(
                    "env.memory must be 32-bit non-shared for this linker path",
                ));
            }
            return Ok(mt);
        }
    }
    Err(WasmError::runtime("shader missing env.memory import"))
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
        let func = Func::new(&mut *store, func_ty, move |caller, params, results| {
            dispatch_native_builtin(caller, builtin_id, params, results)
        });

        linker
            .define(&mut *store, "builtins", &name_for_closure, func)
            .map_err(|e| WasmError::runtime(format!("linker builtins.{name_for_closure}: {e}")))?;
    }
    Ok(())
}

/// Instantiate a compiled shader module, linking native builtins + `env.memory` when imports require it.
pub(crate) fn instantiate_wasm_module(
    engine: &Engine,
    store: &mut Store<()>,
    wasm_bytes: &[u8],
) -> Result<(Instance, Option<Memory>), WasmError> {
    let shader_mod = Module::new(engine, wasm_bytes)
        .map_err(|e| WasmError::runtime(format!("WASM parse: {e:#}")))?;

    if !module_needs_builtins_link(&shader_mod) {
        let instance = Instance::new(&mut *store, &shader_mod, &[])
            .map_err(|e| WasmError::runtime(format!("WASM instantiate: {e}")))?;
        return Ok((instance, None));
    }

    let mut linker = Linker::new(engine);

    let memory_ty = shader_env_memory_type(&shader_mod)?;
    let memory = Memory::new(&mut *store, memory_ty)
        .map_err(|e| WasmError::runtime(format!("Memory::new: {e}")))?;
    linker
        .define(&mut *store, "env", "memory", memory)
        .map_err(|e| WasmError::runtime(format!("linker env.memory: {e}")))?;

    link_builtins(&mut linker, &mut *store, &shader_mod)?;

    let instance = linker
        .instantiate(&mut *store, &shader_mod)
        .map_err(|e| WasmError::runtime(format!("shader instantiate: {e}")))?;
    Ok((instance, Some(memory)))
}
