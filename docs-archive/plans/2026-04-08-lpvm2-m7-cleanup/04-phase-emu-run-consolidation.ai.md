# Phase 4: Consolidate emu_run.rs into EmuInstance

## Scope

The `lpvm-emu/src/emu_run.rs` module provides `glsl_q32_call_emulated` and other
helpers. `EmuInstance` now implements `LpvmInstance::call`/`call_q32`. We should
consolidate these to reduce maintenance burden and avoid ABI drift between the
two paths.

## Current State

### `lp-shader/lpvm-emu/src/emu_run.rs`

Provides:

- `GUEST_VMCTX_BYTES` constant
- `write_guest_vmctx_header()` - writes VMContext to guest memory
- `glsl_q32_call_emulated()` - high-level Q32 call helper
- `run_loaded_function_i32()` - runs at PC with args
- `run_lpir_function_i32()` - runs IR function by name
- `riscv32_reference_isa()` - reference ISA config

### `lp-shader/lpvm-emu/src/instance.rs`

`EmuInstance` implements:

- `prepare_call()` - writes VMContext to guest memory
- `call()` - legacy call (maybe unimplemented for Q32)
- `call_q32()` - Q32 ABI call through emulator

## Consolidation Strategy

### Option A: Migrate all callers to `EmuInstance`

Find all uses of `emu_run` functions and migrate:

```bash
rg "glsl_q32_call_emulated|run_lpir_function_i32|run_loaded_function_i32" lp-shader/ --glob "*.rs"
```

**If filetests only:**

- Filetests should use `EmuEngine` → `EmuModule` → `EmuInstance::call_q32`
- This is the proper LPVM trait path

**If other consumers exist:**

- Migrate them to use `EmuInstance`

### Option B: Make emu_run.rs use EmuInstance internally

Keep the helper functions as thin wrappers:

```rust
pub fn glsl_q32_call_emulated(...) -> Result<...> {
    // Create temporary EmuInstance and use it
    let module = EmuModule::from_loaded_function(...);
    let instance = module.instantiate()?;
    instance.call_q32(vmctx_word, args, results)?;
}
```

This is less ideal because it keeps two paths.

### Option C: Delete emu_run.rs entirely

If no external consumers exist outside tests (which should use `EmuInstance`),
delete the file and inline any truly shared logic into `EmuModule`/`EmuInstance`.

## Recommended: Option A or C

After checking consumers, either:

- **A:** Migrate callers to `EmuInstance` if there are legitimate non-test uses
- **C:** Delete if only tests used it and they can use `EmuInstance`

## Code Changes

### If migrating callers:

Update call sites from:

```rust
use lpvm_emu::{glsl_q32_call_emulated, write_guest_vmctx_header};
// ...
write_guest_vmctx_header(&mut vmctx_slot);
let result = glsl_q32_call_emulated(&mut emu, loaded_fn.pc, &args, vmctx_addr)?;
```

To:

```rust
use lpvm_emu::{EmuEngine, EmuModule};
// ...
let engine = EmuEngine::new(Default::default());
let module = EmuModule::from_loaded_code(code, symbols);
let mut instance = module.instantiate()?;
instance.call_q32(vmctx_word, &args, &mut results)?;
```

### `lp-shader/lpvm-emu/src/lib.rs`

**Delete exports:**

```rust
// Remove:
pub use emu_run::{glsl_q32_call_emulated, run_loaded_function_i32, ...};
```

### `lp-shader/lpvm-emu/src/instance.rs`

**Verify `call_q32` is complete:**

- Does it write VMContext header? (should use `prepare_call`)
- Does it handle fuel properly?
- Does it match what `glsl_q32_call_emulated` did?

Compare implementations to ensure no functionality is lost.

### `lp-shader/lpvm-emu/src/emu_run.rs`

**Delete file** after all consumers migrated.

## Research Required

Before implementing:

1. Find all consumers of `emu_run` exports
2. Check if `EmuInstance::call_q32` is fully functional
3. Verify VMContext handling matches between old and new paths

## Code Organization Reminders

- Don't lose functionality - compare implementations carefully
- VMContext fuel handling is critical - verify both paths set it correctly
- If `EmuInstance::call_q32` is missing features, add them first

## Validate

```bash
# Find consumers
rg "glsl_q32_call_emulated|run_loaded_function_i32" lp-shader/ --glob "*.rs"

# After migration:
cargo check -p lpvm-emu --lib
cargo test -p lpvm-emu --lib

# Filetests using emu:
cargo test -p lps-filetests -- --target rv32.q32c
```

## Phase Notes

- This is about reducing duplicate code paths, not just deletion
- The `EmuInstance::call_q32` path is the canonical one going forward
- Ensure VMContext/fuel handling is identical between old and new paths
