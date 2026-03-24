//! Link shader WASM with `lp_glsl_builtins_wasm.wasm` and shared `env.memory`.

use std::path::{Path, PathBuf};

use lp_glsl_cranelift::{ErrorCode, GlslError};
use wasmtime::{Engine, ExternType, Func, Instance, Linker, Memory, MemoryType, Module, Store};

/// Path to `lp_glsl_builtins_wasm.wasm`. Override with `LP_GLSL_BUILTINS_WASM`.
pub fn builtins_wasm_path() -> PathBuf {
    if let Ok(p) = std::env::var("LP_GLSL_BUILTINS_WASM") {
        return PathBuf::from(p);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/release/lp_glsl_builtins_wasm.wasm")
}

fn shared_env_memory_type(builtins: &Module, shader: &Module) -> Result<MemoryType, GlslError> {
    let mut min_pages: u64 = 0;
    let mut max_cap: Option<u64> = None;
    for module in [builtins, shader] {
        for imp in module.imports() {
            if imp.module() == "env" && imp.name() == "memory" {
                let ExternType::Memory(mt) = imp.ty() else {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        "env.memory import is not a memory type",
                    ));
                };
                if mt.is_64() || mt.is_shared() {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
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
        .map_err(|_| GlslError::new(ErrorCode::E0400, "env.memory minimum pages overflow u32"))?;
    let max_u32 = max_cap
        .map(|m| u32::try_from(m))
        .transpose()
        .map_err(|_| GlslError::new(ErrorCode::E0400, "env.memory maximum overflow u32"))?;
    Ok(MemoryType::new(min_u32, max_u32))
}

fn module_needs_builtins_link(shader: &Module) -> bool {
    shader
        .imports()
        .any(|imp| imp.module() == "builtins" || (imp.module() == "env" && imp.name() == "memory"))
}

/// Instantiate a compiled shader module, linking builtins + `env.memory` when imports require it.
///
/// When LPFX/builtins are linked, returns the shared [`Memory`] handle (same backing store the
/// shader imports as `env.memory`) so tests can inspect linear memory after calls.
pub fn instantiate_wasm_module(
    engine: &Engine,
    store: &mut Store<()>,
    wasm_bytes: &[u8],
) -> Result<(Instance, Option<Memory>), GlslError> {
    let shader_mod = Module::new(engine, wasm_bytes)
        .map_err(|e| GlslError::new(ErrorCode::E0400, format!("WASM parse: {e:#}")))?;

    if !module_needs_builtins_link(&shader_mod) {
        let instance = Instance::new(&mut *store, &shader_mod, &[])
            .map_err(|e| GlslError::new(ErrorCode::E0400, format!("WASM instantiate: {e}")))?;
        return Ok((instance, None));
    }

    let builtins_path = builtins_wasm_path();
    let builtins_bytes = std::fs::read(&builtins_path).map_err(|e| {
        GlslError::new(
            ErrorCode::E0400,
            format!(
                "read `{}`: {e}\n\
                 build: cargo build -p lp-glsl-builtins-wasm --target wasm32-unknown-unknown --release\n\
                 or set LP_GLSL_BUILTINS_WASM",
                builtins_path.display()
            ),
        )
    })?;

    let builtins_mod = Module::new(engine, &builtins_bytes)
        .map_err(|e| GlslError::new(ErrorCode::E0400, format!("builtins WASM parse: {e}")))?;

    let memory_ty = shared_env_memory_type(&builtins_mod, &shader_mod)?;
    let memory = Memory::new(&mut *store, memory_ty)
        .map_err(|e| GlslError::new(ErrorCode::E0400, format!("Memory::new: {e}")))?;

    let builtins_inst = Instance::new(&mut *store, &builtins_mod, &[memory.into()])
        .map_err(|e| GlslError::new(ErrorCode::E0400, format!("builtins instantiate: {e}")))?;

    let mut linker = Linker::new(engine);
    linker
        .define(&mut *store, "env", "memory", memory)
        .map_err(|e| GlslError::new(ErrorCode::E0400, format!("linker env.memory: {e}")))?;

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
            .map_err(|e| {
                GlslError::new(ErrorCode::E0400, format!("linker builtins.{name}: {e}"))
            })?;
    }

    let instance = linker
        .instantiate(&mut *store, &shader_mod)
        .map_err(|e| GlslError::new(ErrorCode::E0400, format!("shader instantiate: {e}")))?;
    Ok((instance, Some(memory)))
}
