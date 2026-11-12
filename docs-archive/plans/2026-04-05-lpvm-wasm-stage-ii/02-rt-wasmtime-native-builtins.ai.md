# Phase 2: Refactor `rt_wasmtime` with Native Builtin Linking

## Scope

- Rename `runtime/` → `rt_wasmtime/`.
- Replace filesystem `.wasm` loading with native `Func::new` dispatch.
- Drop the `runtime` feature flag — runtime is always included.
- Add `lps-builtins` as a dependency.
- Update `Cargo.toml` with target-specific dependencies.
- Fix existing tests.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation

### 1. Update `Cargo.toml`

Drop the `runtime` feature. Add `lps-builtins` and target-gate wasmtime:

```toml
[package]
name = "lpvm-wasm"
# ...

[dependencies]
lpir = { path = "../lpir" }
lps-shared = { path = "../lps-shared" }
lps-builtin-ids = { path = "../lps-builtin-ids" }
lps-builtins = { path = "../lps-builtins" }
lpvm = { path = "../lpvm" }
wasm-encoder = "0.245"
log = { workspace = true, default-features = false }

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
wasmtime = "42"

[target.'cfg(target_arch = "wasm32")'.dependencies]
js-sys = "0.3"
wasm-bindgen = "0.2"

[dev-dependencies]
lps-frontend = { path = "../lps-frontend" }
```

Remove the `[[test]]` sections with `required-features = ["runtime"]`.

### 2. Rename `runtime/` → `rt_wasmtime/`

Rename the directory. Update all internal `use crate::runtime::` paths to
`use crate::rt_wasmtime::`.

### 3. Update `lib.rs`

Remove `#![cfg_attr(not(feature = "runtime"), no_std)]` — the crate always
requires std (wasmtime on host, wasm-bindgen on browser). Replace
`#[cfg(feature = "runtime")] pub mod runtime;` with:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod rt_wasmtime;
#[cfg(target_arch = "wasm32")]
pub mod rt_browser;
```

For this phase, `rt_browser` doesn't exist yet, so guard with a TODO comment
or a stub module.

### 4. Update `error.rs`

Remove `#[cfg(feature = "runtime")]` from the `Runtime` variant and the
`runtime()` constructor. The runtime is always present.

### 5. Rewrite `rt_wasmtime/link.rs` — native builtin linking

Replace the filesystem-based linking with native dispatch. The key change:
instead of loading `lps_builtins_wasm.wasm` and instantiating it, create
host functions that call directly into `lps-builtins`.

**Strategy:** Inspect the shader module's imports. For each import in the
`"builtins"` namespace, look up the import's `FuncType` and create a
`Func::new` with a closure that dispatches to the corresponding
`#[no_mangle] extern "C"` function by name.

```rust
use wasmtime::{Caller, Engine, Func, FuncType, Instance, Linker, Memory, MemoryType, Module, Store, Val, ValType};
use lps_builtin_ids::BuiltinId;

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

        let builtin_id = BuiltinId::builtin_id_from_name(&name)
            .ok_or_else(|| WasmError::runtime(format!("unknown builtin: {name}")))?;

        let func = create_native_builtin(store, &func_ty, &name, builtin_id)?;
        linker.define(&mut *store, "builtins", &name, func)
            .map_err(|e| WasmError::runtime(format!("linker builtins.{name}: {e}")))?;
    }
    Ok(())
}
```

`create_native_builtin` creates a `Func::new` that takes `&[Val]` args and
calls the corresponding `extern "C"` function. Since all builtins are scalar
i32-in/i32-out (Q32) or i32-in/i32-out with varying arity, dispatch by name:

```rust
fn create_native_builtin(
    store: &mut Store<()>,
    func_ty: &FuncType,
    name: &str,
    _builtin_id: BuiltinId,
) -> Result<Func, WasmError> {
    let name_owned = name.to_string();
    let func = Func::new(&mut *store, func_ty.clone(), move |_caller, args, results| {
        dispatch_builtin(&name_owned, args, results)
    });
    Ok(func)
}

fn dispatch_builtin(
    name: &str,
    args: &[Val],
    results: &mut [Val],
) -> Result<(), wasmtime::Error> {
    // Extract i32 args, call the extern "C" function, write i32 results.
    // Use a match on name or BuiltinId to dispatch.
    // Most builtins are fn(i32) -> i32 or fn(i32, i32) -> i32, etc.
    // ...
}
```

The `dispatch_builtin` function needs to handle the varying arities. The
cleanest approach: match on name and call the function directly. There are
~100 builtins, but most share a handful of signatures. Group by arity:

- 1-in-1-out: sin, cos, tan, etc.
- 2-in-1-out: pow, atan2, mod, fma variant, etc.
- 3-in-1-out: fma, hsv2rgb, hue2rgb, etc.
- Multi-in-multi-out: noise, worley, etc. (return multiple i32 results)

For this phase, a generated or hand-written dispatch function is needed.
A practical approach: use `BuiltinId` to dispatch, reading args as raw i32
values and writing results as raw i32 values:

```rust
fn dispatch_builtin(name: &str, args: &[Val], results: &mut [Val]) -> Result<(), wasmtime::Error> {
    match name {
        "__lps_sin_q32" => {
            let x = args[0].unwrap_i32();
            results[0] = Val::I32(lps_builtins::builtins::glsl::sin_q32::__lps_sin_q32(x));
        }
        // ... (all other builtins)
        _ => return Err(wasmtime::Error::msg(format!("unknown builtin: {name}"))),
    }
    Ok(())
}
```

This is verbose but straightforward. A proc macro or codegen could generate
it from `lps-builtin-ids`, but for now a hand/copy approach is fine — the
list is auto-generated in `builtin_refs.rs` and can be adapted.

**Alternative**: The dispatch can use raw function pointer casting since all
builtins are `extern "C"` with `i32` args/results. Extract arg count from
`func_ty`, cast the function pointer, and call generically. This is more
elegant but requires careful unsafe code. Choose whichever is more practical
during implementation.

### 6. Remove `builtins_wasm` from engine/module

`WasmLpvmEngine` no longer stores `builtins_wasm: Vec<u8>`. Remove:
- `builtins_wasm` field from `WasmLpvmEngine` and `WasmLpvmModule`
- `WasmLpvmEngine::new()` no longer takes builtins bytes
- `WasmLpvmEngine::try_default_builtins()` — remove entirely
- `link::builtins_wasm_path()` — remove entirely
- `WasmLpvmModule::compile()` no longer clones builtins bytes

New engine constructor:

```rust
impl WasmLpvmEngine {
    pub fn new(compile_options: WasmOptions) -> Result<Self, WasmError> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)
            .map_err(|e| WasmError::runtime(format!("failed to create WASM engine: {e}")))?;
        Ok(Self { engine, compile_options })
    }
}
```

### 7. Update `rt_wasmtime/link.rs` — `instantiate_wasm_module`

New signature (no more `builtins_wasm` bytes):

```rust
pub(crate) fn instantiate_wasm_module(
    engine: &Engine,
    store: &mut Store<()>,
    wasm_bytes: &[u8],
) -> Result<(Instance, Option<Memory>), WasmError> {
    let shader_mod = Module::new(engine, wasm_bytes)?;

    if !module_needs_builtins_link(&shader_mod) {
        let instance = Instance::new(&mut *store, &shader_mod, &[])?;
        return Ok((instance, None));
    }

    let mut linker = Linker::new(engine);

    // Create shared memory for shader (env.memory import)
    let memory_ty = shader_env_memory_type(&shader_mod)?;
    let memory = Memory::new(&mut *store, memory_ty)?;
    linker.define(&mut *store, "env", "memory", memory)?;

    // Link native builtins
    link_builtins(&mut linker, &mut *store, &shader_mod)?;

    let instance = linker.instantiate(&mut *store, &shader_mod)?;
    Ok((instance, Some(memory)))
}
```

The `shared_env_memory_type` function simplifies — it only needs to inspect
the shader module's `env.memory` import (no builtins module to merge with).

### 8. Update `rt_wasmtime/instance.rs`

Remove `builtins_wasm` from the `WasmLpvmInstance::new()` call:

```rust
let (instance, _) = link::instantiate_wasm_module(
    &module.engine,
    &mut store,
    &module.wasm_bytes,
)?;
```

### 9. Update tests

`runtime_lpvm_call.rs`: Update to use simplified engine constructor:

```rust
let engine = WasmLpvmEngine::new(opts).expect("engine");
```

`runtime_builtin_sin.rs`: Remove builtins path loading. Use the same
simplified constructor — builtins are now linked natively:

```rust
let engine = WasmLpvmEngine::new(opts).expect("engine");
```

Remove `use lpvm_wasm::runtime::link;` and the `builtins_engine()` helper.
Update module path references from `runtime::` to `rt_wasmtime::`.

`compile_roundtrip.rs`: Should be unaffected (emit-only).

## Validate

```bash
cargo check -p lpvm-wasm
cargo test -p lpvm-wasm
cargo test -p lpvm-wasm --test runtime_lpvm_call
cargo test -p lpvm-wasm --test runtime_builtin_sin
cargo test -p lpvm-wasm --test compile_roundtrip
```
