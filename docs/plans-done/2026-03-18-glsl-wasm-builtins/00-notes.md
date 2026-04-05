# WASM Builtin Calling — Planning Notes

**Consolidated design:** `00-design.md`  
**Phases:** `01-builtins-wasm-artifact.md` … `07-rainbow-validation-cleanup.md`

## Scope of work

Add builtin function calling to the `lps-wasm` backend. This involves:

1. **WASM import section infrastructure** — emit `(import "builtins" ...)` for builtins used by compiled shaders
2. **Builtin call emission** — resolve GLSL builtin names to import indices and emit `call` instructions
3. **Import satisfaction** — `builtins.wasm` (compiled from `lps-builtins` for `wasm32-unknown-unknown`) instantiated with **shared linear memory**; same linking path in wasmtime tests and in the browser. No separate “test-only” host path unless needed for debugging.
4. **Vector-argument builtins** — handle builtins that take/return vectors (flattened ABI)
5. **LPFX function imports** — same pattern for `lpfx_worley`, `lpfx_fbm`, `lpfx_psrdnoise`
6. **Out parameters** — WASM linear memory for `lpfx_psrdnoise`'s `out vec2 gradient`
7. **Rainbow shader end-to-end** — validate the full pipeline

Predecessors: phases 1–5 of part-iii are complete (scalar ops, type constructors, control flow, user functions, vectors). The Q32 mul/div i64 validation bug is tracked but works in practice for filetests.

## Current state

### lps-wasm module builder
- Emits 4 sections: Type, Function, Export, Code
- **No import section**, no memory section
- FunCall handling: type constructors + user functions only; builtins → error
- WasmExecutable instantiation: `Instance::new(&mut store, &module, &[])` — no imports

### Builtin infrastructure (existing)
- `lps-builtin-ids`: `BuiltinId` enum, `name()` → `"__lp_q32_sin"`, `builtin_id_from_name()`
- `lps-builtins`: `extern "C"` implementations, all i32-based for Q32
- Vector ABI: inputs scalarized (one i32 per component), vector returns via `*mut i32` (sret pointer as first param)
- `lps-frontend`: `is_builtin_function(name)`, `check_builtin_call(name, &arg_types)`, `is_lpfx_fn(name)`

### Cranelift builtin calling pattern
- GLSL name → string match in `emit_builtin_call` → per-builtin method
- Per-builtin: `get_math_libcall("sinf")` → `map_testcase_to_builtin("sinf", 1)` → `BuiltinId::LpQ32Sin`
- Vector builtins handled component-wise in Cranelift (e.g. sin(vec3) = [sin(x), sin(y), sin(z)])
- Some builtins are fully inline (clamp = max then min), others call extern functions

### Rainbow.shader builtins needed
Scalar: `clamp`, `abs`, `mod`, `fract`, `floor`, `exp`, `cos`, `sin`, `smoothstep`, `mix`, `atan`, `min`
LPFX: `lpfx_worley`, `lpfx_fbm`, `lpfx_psrdnoise` (with out param)

## Questions

### Q1: Two-pass vs pre-scan for import collection
The WASM import section must come before the code section. We need to know which builtins are used before emitting code. Two approaches:
- **Pre-scan AST:** Walk all function bodies to collect builtin names, build import table, then emit code with known indices.
- **Two-pass codegen:** Emit code once (recording builtin usage), then re-emit with correct import indices.

**Suggested answer:** Pre-scan. Simpler, no re-emission. Walk the AST looking for FunCall nodes where `is_builtin_function(name)` or `is_lpfx_fn(name)` is true.

**Decision:** Pre-scan. Matches existing `walk_for_declarations` pattern.

### Q2: Scalar-only imports or also vector-specific imports?
Cranelift handles vector builtins by calling the scalar builtin per-component (e.g. `sin(vec3)` → 3 calls to `__lp_q32_sin`). Some builtins like `clamp` are inline (max then min) and never call an import at all. Should WASM do the same (component-wise calls to scalar imports + inline logic for clamp/mix/etc.), or should we import vector-specific builtins?

**Suggested answer:** Match Cranelift — component-wise scalar imports + inline logic for compound builtins. Avoids needing vector-specific import ABI. Keeps the import surface minimal.

**Decision:** Match Cranelift. Scalar imports only, component-wise for vectors, inline for compound builtins.

### Q3: Import function naming and module
Format: `(import "builtins" "__lp_q32_sin" (func ...))`. Module name `"builtins"`, function name from `BuiltinId::name()`. LPFX uses the same module name?

**Suggested answer:** Single module `"builtins"` for both Q32 math and LPFX. Function names from `BuiltinId::name()`.

**Decision:** Single `"builtins"` module. Names from `BuiltinId::name()`.

### Q4: GLSL name → BuiltinId mapping for WASM
Cranelift has a complex chain: GLSL name → per-builtin method → libcall name → `map_testcase_to_builtin` → BuiltinId. For WASM, we need a simpler path. Options:
- Reuse `map_testcase_to_builtin` from cranelift (it's auto-generated)
- Create a direct GLSL name → BuiltinId mapping
- Use the frontend's `check_builtin_call` which already resolves overloads

**Suggested answer:** Create a mapping in lps-wasm that goes GLSL name + arg types → BuiltinId directly. Simpler than the Cranelift chain. Could be auto-generated or hand-written for the subset we need.

**Decision:** Auto-generate `glsl_to_builtin_id(name, arg_count) -> Option<BuiltinId>` in `lps-builtin-ids` via `build-builtins.sh`. WASM backend checks inline implementations first, falls back to import call via this mapping. New builtins auto-resolve to imports with no compiler changes.

### Q5: Out parameter strategy for LPFX
`lpfx_psrdnoise` has an `out vec2 gradient` parameter. Options:
- WASM linear memory: declare memory, allocate slots, pass pointers
- Multi-value return: return the gradient components alongside the scalar return

The existing `extern "C"` ABI uses sret (pointer as first param). For WASM imports:
- If we use memory, host functions need `Caller` access to write to the module's memory
- If we use multi-value, we'd need different signatures from the native builtins

**Suggested answer:** WASM linear memory, matching the native sret convention. The host function receives a pointer into the module's memory and writes gradient components there. This aligns with the existing builtin ABI.

**Decision:** WASM linear memory with sret. Static offsets or simple bump allocator. Only needed for psrdnoise's gradient out param.

### Q6: WasmExecutable import wiring
Currently `Instance::new(&mut store, &module, &[])`. For builtins, we need to provide imports. Options:
- Use `wasmtime::Linker` to register all potential builtins upfront
- Dynamically provide only the imports the module declares

**Suggested answer:** Use `wasmtime::Linker`. Register all known builtins (or at least the ones we support). The linker will match them to the module's import declarations.

**Decision (revised):** Satisfy shader imports from a **pre-built `builtins.wasm`** instance (same artifact as production), not per-builtin `func_wrap`. Use wasmtime `Linker` / `define` (or equivalent) to expose builtins exports under the `"builtins"` module name. Build `builtins.wasm` via a small crate (e.g. `lps-builtins-wasm`) in `build-builtins.sh`, mirroring `lps-builtins-emu-app`.

### Q6b: Shared memory across shader and builtins

Separate modules default to separate memories; sret/out params require one linear memory. **Pattern:** host creates `Memory`, both **shader** and **builtins** import it (e.g. `(import "env" "memory" (memory …))`) via `--import-memory` / matching codegen — neither module owns the memory declaration. Instantiate order: create memory → instantiate `builtins.wasm` with `env.memory` → link builtins exports into shader’s `"builtins"` imports → instantiate shader with `env.memory` + builtins. Matches browser `WebAssembly.Memory` + two `instantiate` calls.

**Rationale:** Builtin module will grow; **textures and future host data** will also use the same shared memory — establishing the host-owned memory model now avoids a second migration.

**Decision:** Host-owned shared memory; shader and builtins both import memory; `builtins.wasm` built with imported memory. Tests use this path, not a divergent “native only” path.

### Q7: Where does the Q32 mul i64 validation bug stand?
impl-notes says Q32 mul triggers "expected i32, found i64" WASM validation. The `emit_q32_mul` code uses i64 instructions. Is this actually broken in practice for filetests, or only for the isolated test?

**Suggested answer:** Need to investigate — the test is `#[ignore]` so it hasn't been validated. This is relevant because builtins may depend on Q32 mul for inline operations. If the i64 instructions work in wasmtime but fail validation, we may need to use imported Q32 mul instead.

**Decision:** Fixed. `emit_q32_mul` now saves one operand to a temp before extending to i64. Test passes. The `#[ignore]` annotation on `test_q32_float_mul` should be removed. Q32 div uses the same pattern and also needs the fix (verify).

## Notes

- **Builtins crate for wasm:** New thin crate + `wasm32-unknown-unknown` build in `build-builtins.sh`, analogous to RISC-V `lps-builtins-emu-app`.
- **Shader codegen:** Must emit memory **import** (not a private `(memory …)`) when using shared model; codegen for bump/out slots uses offsets in that imported memory.
- **Scalar-only builtins** do not touch memory; shared memory is still imported so one instantiation story covers all shaders.
- **Future:** Textures and other large buffers attach to the same memory region (or grow it); design stays consistent.
