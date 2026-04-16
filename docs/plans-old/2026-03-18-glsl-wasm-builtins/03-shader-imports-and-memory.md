# Phase 3: Shader module — pre-scan, import section, memory import, indices

## Scope of phase

- **`builtin_scan`:** Walk all statements/expressions in the typed AST; record builtin and LPFX
  calls (names + enough information to resolve `BuiltinId` after phase 2).
- **`compile_to_wasm` ordering:** Build deterministic ordered list of imports → assign indices
  `0..M-1` for **imported functions**; user function indices follow WASM rules (imports first).
- **Import section:** Emit `wasm_encoder::ImportSection` for each used builtin function with correct
  `(param …) (result …)` signatures matching `builtins.wasm` exports (scalar i32 pipeline for Q32
  unless a specific builtin needs more — align with `lps-builtins` symbols).
- **Memory import:** When the shader needs out-params or always (per design: **always import** for
  one linking story), emit `(import "env" "memory" (memory min_pages))` — coordinate min pages with
  host.
- **Codegen context:** Pass `import_base: u32`, `memory_import_active: bool`, and maps
  `BuiltinId -> import index` into function body emission.

## Code organization reminders

- Scan in a dedicated module; avoid duplicating full AST match in codegen.
- Section order in `compile_to_wasm` must match WASM spec: imports before functions that reference
  them.

## Implementation details

- Empty shader / no builtin usage: either omit memory import until needed, or always emit memory
  import with zero builtin imports — **decide in implementation** and document in module README (
  preference from design: one consistent story → likely always import memory once builtins path is
  enabled).
- Function index fixups: every `call` to user functions must use **index = import_count +
  local_func_index**.

## Validate

```bash
cd lps && cargo test -p lps-wasm
```

Add unit tests that compile a tiny shader calling **no** builtins and one calling a **single**
import — inspect bytes or use wasmtime to list imports.

## Validate

- No dead code warnings; remove temporary `println!`.
