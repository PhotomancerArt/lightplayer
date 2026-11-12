# Phase 2: Delete old emit.rs + dead code

## Scope

Delete the old 1470-line `rv32/emit.rs` monolith and all references to deleted
modules (`regalloc`, `IsaBackend`, `CodeBlob`). After this phase, the crate
will not compile (no emission pipeline exists yet), but all surviving files will
be clean of dead references.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Delete `rv32/emit.rs`

This file contains:
- `EmitContext` + `emit_vinst()` — the old VInst→bytes emitter using greedy/linear_scan
- `emit_function_bytes()` — old orchestration (lower → alloc → emit)
- `emit_module_elf()` — ELF generation
- `NativeReloc`, `EmittedFunction`, `CallSaveLayout` — types
- Tests

All of this is being replaced by:
- `compile.rs` (orchestration) — phase 3
- `emit.rs` at root (new emission) — phase 4
- `link.rs` (ELF generation) — phase 5

Delete the entire file. Update `rv32/mod.rs` to remove `pub mod emit;`.

### 2. Save types needed by other phases

Before deleting, note these types that will be recreated:

- `NativeReloc { offset: usize, symbol: String }` → goes in `compile.rs` (phase 3)
- `EmittedFunction` → becomes `CompiledFunction` in `compile.rs` (phase 3)

### 3. Clean up `rv32/mod.rs`

The `emit_function_fastalloc_bytes` function references the deleted `emit.rs`.
Replace the function body with a TODO stub:

```rust
/// Lower, fast-allocate, and emit one function to raw RISC-V bytes (no ELF).
/// TODO: phase 3 replaces this with compile::compile_function
pub fn emit_function_fastalloc_bytes(
    _func: &IrFunction,
    _ir: &LpirModule,
    _module_abi: &ModuleAbi,
    _fn_sig: &LpsFnSig,
    _float_mode: FloatMode,
) -> Result<Vec<u8>, NativeError> {
    Err(NativeError::FastallocInternal(alloc::string::String::from(
        "M3.2: pipeline being restructured",
    )))
}
```

### 4. Stub out callers

Files that called `emit_function_bytes` or `emit_module_elf`:

**`debug_asm.rs`**: Stub `compile_module_asm_text` to return
`Err(NativeError::FastallocInternal(...))` with a TODO.

**`rt_jit/compiler.rs`**: Stub `compile_module_jit` to return
`Err(NativeError::FastallocInternal(...))` with a TODO.

**`rt_emu/engine.rs`**: Stub the `compile` method to return
`Err(NativeError::FastallocInternal(...))` with a TODO.

### 5. Clean up `lib.rs`

Remove any re-exports that reference deleted items. Ensure `pub mod rv32;`
is declared. Remove `pub mod regalloc;` if still present.

### 6. Clean up `error.rs`

Ensure `NativeError::FastallocInternal(String)` variant exists (it should
from M3.1). If not, add it. Also remove any variants that only the old
pipeline used (verify first — likely none are exclusive).

## Validate

```bash
cargo check -p lpvm-native
```

Should compile with stubs. All surviving code is clean, no dead references.
Tests that depend on old pipeline will fail at runtime (return errors), but
test compilation should succeed.
