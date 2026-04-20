# Phase 6: LPFX, out parameters, memory slots

## Scope of phase

- **Codegen:** `lpfn_psrdnoise` and similar — allocate out slots in **shared** linear memory (bump pointer or static layout computed per function); emit `i32` pointer args; load results after `call`.
- **Builtin side:** `builtins.wasm` must perform `i32.store` into the imported memory at the given offsets — verify Rust→wasm compilation for those code paths (may already be correct if Cranelift uses same symbols).
- **Worley / FBM:** Scalar returns; confirm flattened argument lists match `lps-builtins` exports.

## Code organization reminders

- `memory.rs` owns offset allocation policy; avoid scattering magic numbers.

## Implementation details

- **psrdnoise** gradient `out vec2`: 8 bytes or 2× i32 slots; alignment as required by store instructions.
- If any LPFX symbol uses **sret** with pointer in shared memory, document exact parameter order vs Cranelift.

## Validate

- LPFX-focused filetests under `filetests/` for wasm.q32.
- Manual: compile a minimal shader calling `lpfn_worley` / `lpfn_psrdnoise` and run under wasmtime.

## Validate

- Fix warnings introduced in this phase only.
