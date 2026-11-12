# Phase 3: Migrate wasm_link.rs to lpvm-wasm

## Scope

The file `lps-filetests/src/test_run/wasm_link.rs` provides wasmtime-based
linking for builtins + shader WASM. It's currently used by
`tests/lpfn_builtins_memory.rs`. We need to migrate that test to use `lpvm-wasm`'s
instantiate/link path instead, then delete `wasm_link.rs`.

## Current State

### `lps-filetests/src/test_run/wasm_link.rs`

Provides:
- `builtins_wasm_path()` - finds the builtins wasm file
- `instantiate_wasm_module()` - creates wasmtime instance with builtins linked
- `shared_env_memory_type()` - memory type for env.memory

Used by:
- `lps-filetests/tests/lpfn_builtins_memory.rs`

### `lpvm-wasm` path

`lpvm-wasm` already has:
- `WasmEngine::compile()` - compiles IR to WASM bytes
- `WasmModule::instantiate()` - creates wasmtime instance
- `WasmInstance::call_q32()` - executes with proper VMContext/fuel setup

## Migration Strategy

### Option A: Use `lpvm-wasm` directly in the test

Replace `wasm_link.rs` usage with `lpvm-wasm` API:

```rust
// Old (wasm_link.rs):
let (instance, memory) = wasm_link::instantiate_wasm_module(&engine, &mut store, &wasm_bytes)?;

// New (lpvm-wasm):
let engine = WasmEngine::new(Default::default());
let module = engine.compile(glsl_source)?;  // or from existing WASM bytes
let instance = module.instantiate(&mut store)?;
instance.call_q32(&mut store, "render", args)?;
```

### Option B: Simplified test using lpvm-wasm

If the test is specifically testing builtins memory, we might:
1. Load/compile the shader via `lpvm-wasm`
2. Call `WasmInstance::call_q32` which internally handles VMContext/fuel
3. Verify the builtins properly read fuel from VMContext

## Code Changes

### `lps-filetests/tests/lpfn_builtins_memory.rs`

**Current imports:**
```rust
use lps_filetests::test_run::wasm_link::{builtins_wasm_path, instantiate_wasm_module};
```

**New imports:**
```rust
use lpvm_wasm::{WasmEngine, CompileOptions};
// Or use lps_filetests runner infrastructure if already ported
```

**Current test body:**
```rust
let engine = Engine::new(&config)?;
let mut store = Store::new(&engine, ());
let (instance, memory) = instantiate_wasm_module(&engine, &mut store, &shader_wasm)?;

// Manually write VMContext to memory
let fuel = 1_000_000u64;
let fuel_le = fuel.to_le_bytes();
memory.write(&mut store, 0, &fuel_le)?;

// Call function
let func = instance.get_typed_func::<(i32,), i32>(&mut store, "__lp_get_fuel")?;
let result = func.call(&mut store, (0,))?;
```

**New test body (rough):**
```rust
// Use lpvm-wasm which handles VMContext internally
let engine = WasmEngine::new(CompileOptions::default());
let module = engine.compile_from_wasm_bytes(&shader_wasm)?;
let mut instance = module.instantiate()?;

// call_q32 handles VMContext setup internally
let result = instance.call_q32(0 /* vmctx_word */, &[], &mut result_buf)?;
```

## Research Required

Before implementing, check:
1. Does `lpvm-wasm` have `compile_from_wasm_bytes` or only from GLSL/IR?
2. Does `WasmInstance` expose fuel-reading builtins test interface?
3. Can we call individual builtins like `__lp_get_fuel` or only full shaders?

The test may need to be rewritten to test builtins via the full shader path,
or `lpvm-wasm` may need a test-only interface.

## Alternative: Delete the Test

If the test is redundant (covered by filetests using wasm.q32 target), consider
deleting it entirely instead of migrating.

## Files to Modify

- `lps-filetests/tests/lpfn_builtins_memory.rs` - migrate to lpvm-wasm or delete
- `lps-filetests/src/test_run/mod.rs` - remove `pub mod wasm_link;`

## Files to Delete

- `lps-filetests/src/test_run/wasm_link.rs`

## Code Organization Reminders

- If test is kept, rewrite using `lpvm-wasm` APIs only
- If test is deleted, ensure equivalent coverage exists in filetests
- `wasm_runner.rs` may also need updates if it uses `wasm_link`

## Validate

```bash
cargo test -p lps-filetests --test lpfn_builtins_memory  # should pass or be removed
cargo check -p lps-filetests --lib
cargo test -p lps-filetests
```

## Phase Notes

- Check if `wasm_runner.rs` also uses `wasm_link.rs`
- If test is complex to migrate, consider deletion with note about filetest coverage
- This removes the last non-lpvm-wasm WASM linking code
