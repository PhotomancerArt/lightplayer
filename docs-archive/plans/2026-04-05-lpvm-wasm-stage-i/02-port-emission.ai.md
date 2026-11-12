## Phase 2: Port Emission Code

### Scope

Copy-adapt emission code from `lps-wasm/src/emit/` to `lpvm-wasm/src/emit/`.
This is a parallel implementation — `lps-wasm` remains untouched.

Key differences from source:
- Direct `IrModule` input (no GLSL frontend coupling)
- Emission entry is `emit_module(ir, options)` not `glsl_wasm(source, options)`
- No Q32 runtime conversion (runtime handles that)

### Implementation Details

**emit/mod.rs:**

Re-export and module organization:
```rust
mod control;
mod func;
mod imports;
mod memory;
mod ops;
mod q32;

pub use super::emit_module;
pub(crate) use imports::ImportFilter;
```

**emit.rs (main entry):**

Adapt from `lps-wasm/src/emit/mod.rs`:
- Function signature: `pub fn emit_module(ir: &IrModule, options: &WasmOptions)`
- Return: `Result<(Vec<u8>, Option<i32>), WasmError>`
- Convert errors to `WasmError::Emission(String)`
- Keep shadow stack base detection logic
- Keep `render_frame` emission for `main` entry

**emit/control.rs:**

Copy from `lps-wasm/src/emit/control.rs`:
- `emit_if`, `emit_loop`, `emit_switch`
- Adapt error types to `WasmError`

**emit/func.rs:**

Copy from `lps-wasm/src/emit/func.rs`:
- `wasm_function_signature` — IrFunction → WASM params/results
- `encode_ir_function` — main function body emission
- `FuncEmitCtx` struct — per-function emission state

**emit/imports.rs:**

Copy from `lps-wasm/src/emit/imports.rs`:
- `ImportFilter` — filter to available builtins
- Import remapping for filtered indices

**emit/memory.rs:**

Copy from `lps-wasm/src/emit/memory.rs`:
- Shadow stack constants
- Slot allocation logic

**emit/ops.rs:**

Copy from `lps-wasm/src/emit/ops.rs`:
- LPIR Op → WASM instruction mapping
- Q32 builtins emission (import calls)

**emit/q32.rs:**

Copy from `lps-wasm/src/emit/q32.rs`:
- Q32 arithmetic helpers

### Notes

- Keep `wasm-encoder` usage identical — it works well
- Error messages: use `alloc::format!` for consistency
- Comments: copy useful ones, add "TODO(lpvm-wasm):" for new concerns
- Tests: none yet — phase 3 adds emission tests

### Validate

```bash
cargo check -p lpvm-wasm --no-default-features 2>&1 | head -30
```

Fix any compilation errors. Some TODOs in body are acceptable.
