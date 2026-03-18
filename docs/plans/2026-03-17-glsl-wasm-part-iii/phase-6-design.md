# Phase 6 Design: Builtin functions via WASM imports

Plan reference: `2026-03-17-glsl-wasm-part-iii.md` Phase 6

## Goals

1. Add import section handling; track used builtins during codegen
2. Implement builtin call emission (call import)
3. Map GLSL builtin names to BuiltinId
4. Provide host functions in WasmExecutable for each import
5. Handle vector-argument builtins (ABI)
6. Implement builtins for rainbow.shader: clamp, abs, mod, fract, floor, exp, cos, sin, smoothstep, mix, atan, min

---

## 1. Import section

**WASM module layout:** Imports first (in order), then function definitions. Function indices: 0..M-1 = imports, M..M+N-1 = user functions.

**Import format:** `(import "builtins" "__lp_q32_sin" (func (param i32) (result i32)))`

**Build during codegen:** When we encounter a builtin call, record `BuiltinId` in a set. After all functions compiled, emit Import section with one import per used builtin. Assign import indices 0, 1, 2, ...

**Order:** Deterministic. Sort by BuiltinId or by name. Same order when building import-to-index map for call emission.

---

## 2. Builtin call emission

**During emit_rvalue for FunCall:** If builtin, look up import index. Emit args (each arg may be scalar or vector—flatten to scalars for vector args). Emit `call import_idx`.

**Context needs:** `used_builtins: HashSet<BuiltinId>`, `builtin_to_import_index: HashMap<BuiltinId, u32>`. The map is built after we know the full import list. During codegen we only record usage. After codegen, we build the map and would need to re-emit... or we build the map before codegen: first pass to collect used builtins, build import section and map, second pass to emit code with map available.

**Single pass:** Pre-scan all function bodies for builtin calls. Build used_builtins. Then build imports and map. Then emit code. So: two traversal passes over the AST, or one that collects and one that emits.

---

## 3. GLSL name → BuiltinId

**Frontend:** `lp_glsl_frontend::semantic::builtins::is_builtin_function(name)`, `check_builtin_call(name, arg_types)`. There may be a resolver that returns BuiltinId for a given overload.

**lp-glsl-builtin-ids:** `BuiltinId::from_name(name)` or similar. Check the API. The name() returns things like "__lp_q32_sin". GLSL names are "sin", "cos", etc. We need GLSL→builtin mapping. Cranelift's mapping.rs maps testcase names. Frontend likely has builtin registry that maps GLSL name + arg types → BuiltinId.

---

## 4. Host functions in WasmExecutable

**WasmExecutable:** When instantiating the module, we provide imports. Each import is a host function. The host function signature must match: (param i32) (result i32) for __lp_q32_sin.

**Implementation:** Call the native Rust function from lp-glsl-builtins. Example: `|args| __lp_q32_sin(args[0].i32())`.

**wasmtime:** `Linker::func_wrap("builtins", "__lp_q32_sin", |a: i32| { lp_glsl_builtins::__lp_q32_sin(a) })`.

---

## 5. Vector-argument builtins ABI

**Examples:** `clamp(vec3, float, float)`, `mix(vec3, vec3, float)`.

**Options:**
- **Flattened:** Each component as separate i32 param. `clamp(vec3, float, float)` → (i32, i32, i32, i32, i32) = 5 params. Return: 3 results (multi-value) or use sret.
- **Memory:** Pass pointer to vector in linear memory. Builtin reads/writes memory. Adds memory dependency.
- **Match Cranelift:** Cranelift passes vectors as multiple params (sret for return?). Match that ABI so we can reuse builtin implementations.

**Cranelift builtin ABI:** Check how vectors are passed. Likely multiple params. For return, may use sret (pointer) or multi-value. For WASM, multi-value return is native. Use flattened params + multi-value return for consistency.

---

## 6. Rainbow.shader builtins

**Required:** clamp, abs, mod, fract, floor, exp, cos, sin, smoothstep, mix, atan, min.

**Scalar Q32:** All have `__lp_q32_*` forms. Signature: (param i32) (result i32) or (param i32) (param i32) (result i32).

**Vector variants:** clamp(vec3, ...) etc. Use flattened ABI: 3 params for vec3, 1 for float, etc.

---

## 7. Module compilation flow

```
1. Collect functions (main + user)
2. First pass: scan all bodies for builtin calls → used_builtins
3. Build import section: for each used_builtin, emit (import "builtins" name (func ...))
4. Build type section: import types + function types
5. Build func section: import count + function count
6. Build export section
7. For each function: emit body (with builtin_to_import_idx map for call emission)
8. Build code section
```

---

## File change summary

| File | Changes |
|------|---------|
| `codegen/mod.rs` | Two-pass: collect builtins, build imports, emit with map |
| `codegen/expr/function.rs` | emit_builtin_call: emit args, call import |
| `module.rs` or new | Import section builder |
| `exec/*` or wasm runner | Linker: provide host funcs for each import |
| `codegen/builtin_map.rs` | New: GLSL name + arg types → BuiltinId, BuiltinId → import name |

---

## Validation

- builtins/* filetests
- rainbow.shader compiles (builtins used there)
