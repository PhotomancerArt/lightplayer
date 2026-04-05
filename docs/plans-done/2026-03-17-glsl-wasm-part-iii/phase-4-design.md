# Phase 4 Design: User function calls

Plan reference: `2026-03-17-glsl-wasm-part-iii.md` Phase 4

## Goals

1. Build function index map during module compilation
2. Implement `Expr::FunCall` for user-defined functions
3. Handle scalar return values
4. Implement `const` variable declarations
5. Implement global const references in expressions

---

## 1. Function index map

**Current:** Module has Type section, Function section, Export section, Code section. Function indices 0..N-1 for N functions. No imports yet.

**Phase 6 adds imports.** So final layout: imports first (indices 0..M-1), then user functions (indices M..M+N-1).

**For Phase 4:** No imports yet. Function index = position in `functions` array. main first, then user functions.

**Map:** `name -> index`. Build when iterating functions for Type/Function sections.

```rust
let mut func_index_map: HashMap<String, u32> = HashMap::new();
for (i, func) in functions.iter().enumerate() {
    func_index_map.insert(func.name.clone(), i as u32);
}
```

Pass this map to codegen (or store in a compilation context that codegen can access).

---

## 2. User function call emission

**Expr::FunCall** with user-defined name: look up index, emit args, `call idx`.

**Argument emission:** Push args left-to-right (WASM convention: first param = top of stack after args). So emit first arg (bottom), ..., last arg (top). Correct: WASM expects params in order. Emit arg[0], arg[1], ... arg[n]. Stack: [a0, a1, ..., an]. call pops them and pushes results.

**Return value:** Scalar return = one value on stack. `emit_rvalue` for FunCall returns `WasmRValue::scalar(return_ty)`.

```rust
// call user_func(a, b)
for arg in args {
    emit_rvalue(ctx, sink, arg, options)?;
}
let idx = func_index_map.get(func_name)?;
sink.call(*idx);
```

**Context needs func_index_map:** Codegen runs per-function. We need access to the map. Options: (a) pass as param to emit_function, store in WasmCodegenContext; (b) pass to emit_rvalue when we need it. Prefer (a): extend WasmCodegenContext or pass a `CompilationContext` that has func_index_map.

**Compilation flow:** Currently `compile_to_wasm` builds types, funcs, exports, codes. It doesn't have a shared context for codegen. We need to build func_index_map before the code section, then pass it to `stmt::emit_function` or create a `WasmCompilationContext` that holds it.

---

## 3. Return value handling

**Scalar:** One result. Caller's stack gets one value. Done.

**Vector (Phase 5):** Multi-value return. Function type has multiple results; `call` pushes all. Deferred.

---

## 4. Const variable declarations

**Syntax:** `const bool CYCLE_PALETTE = true;`

**Semantics:** Value is constant. Could be constant-folded at compile time, or emitted as a local with a constant initializer.

**Approach:** Treat as regular local with initializer. The initializer must be a constant expression. We allocate a local, emit the constant, local.set. No difference from non-const for codegen; const is enforced by frontend (no assignment to const).

**Allocation:** In `allocate_local_from_decl`, check if declaration has `const` qualifier. Still allocate a local; store const flag if needed for validation. For emission, same as regular init.

**Const in expressions:** `CYCLE_PALETTE` is a variable reference. We look up in locals, emit local.get. Same as non-const.

---

## 5. Global const

**Scope:** Consts can be at shader scope (global) or function scope (local). For function-scope, they're in `ctx.locals` after allocation. For global scope: the TypedShader may have a global scope. Need to check how lps-frontend represents globals.

**TypedShader:** Has main_function and user_functions. Does it have global variables? Check frontend. If globals exist, they may need to be passed as function params or defined as WASM globals. WASM has a Global section. For constant globals, we could define them as immutable globals. For simplicity, if rainbow.shader's CYCLE_PALETTE is in main(), it's a local const. Phase 4 focuses on that.

---

## 6. FunCall dispatch order

In `emit_rvalue` for FunCall:
1. Type constructor (vec2, int, etc.) → Phase 2/5
2. Builtin → Phase 6
3. LPFX → Phase 7
4. User function → this phase
5. Else → error "unknown function"

---

## 7. Module structure pass

**Current:** Single pass builds types, funcs, exports, codes.

**Change:** Build func_index_map before codes. Pass to emit_function. Each function body is emitted with access to the map.

```rust
let func_index_map: HashMap<_, _> = functions.iter().enumerate()
    .map(|(i, f)| (f.name.clone(), i as u32)).collect();

for func in &functions {
    let body = stmt::emit_function(func, &func_index_map, options)?;
    codes.function(&body);
}
```

---

## File change summary

| File | Changes |
|------|---------|
| `codegen/mod.rs` | Build func_index_map, pass to emit_function |
| `codegen/expr/mod.rs` | FunCall: dispatch to user function when not constructor/builtin/lpfx |
| `codegen/expr/function.rs` | New: emit_user_function_call |
| `codegen/context.rs` | Add func_index_map to context (or pass separately) |
| `codegen/stmt/declaration.rs` | Handle const (no code change if same as regular init) |

---

## Validation

- `function/*` filetests
- Const variable in expressions
- Recursive calls (if allowed by GLSL) — likely not; no recursion in GLSL for most profiles
