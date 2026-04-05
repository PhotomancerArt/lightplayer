# Plan: LPFX Functions Static Registry

# Design

## Scope

Eliminate heap allocations from the LPFX function registry (`lpfx_fns::init_functions`). Currently
143 allocations (~4 KB) at first use, all live for the process. Target: 0 heap bytes for the
registry by using static/const data in ROM. Also eliminate temporary Vec allocations in
`find_lpfx_fn` lookups.

## File structure

```
lp-shader/lp-glsl-compiler/src/frontend/semantic/lpfx/
├── lpfx_fn.rs           # UPDATE: LpfxFn uses FunctionSignatureRef; add ParameterRef, FunctionSignatureRef
├── lpfx_fns.rs          # REPLACE: static LPFX_FNS array (generated), lpfx_fns() returns &LPFX_FNS directly
├── lpfx_fn_registry.rs  # UPDATE: find_lpfx_fn loop-based (no Vec allocs); use new ref types
└── lpfx_sig.rs          # UPDATE: build_call_signature uses FunctionSignatureRef (same field names)

lp-shader/lp-glsl-builtins-gen-app/src/lpfx/
└── generate.rs          # UPDATE: emit static const array with &'static str, &[ParameterRef]
```

## Architecture

```
                    ┌─────────────────────────────────────┐
                    │  static LPFX_FNS: &[LpfxFn]         │
                    │  (in .rodata / flash, 0 heap)        │
                    └─────────────────┬───────────────────┘
                                      │
lpfx_fns() ──────────────────────────►│
                                      │
find_lpfx_fn(name, arg_types) ───────►│  iterates, compares name/params
                                      │  (no Vec allocs)
                                      ▼
                    ┌─────────────────────────────────────┐
                    │  LpfxFn {                           │
                    │    glsl_sig: FunctionSignatureRef {  │
                    │      name: &'static str,             │
                    │      return_type: Type,               │
                    │      parameters: &'static [ParamRef]  │
                    │    },                                │
                    │    impls: LpfxFnImpl                 │
                    │  }                                   │
                    └─────────────────────────────────────┘
```

`FunctionSignature` / `Parameter` (semantic/functions.rs) remain for user-defined functions. LPFX
uses parallel `ParameterRef` and `FunctionSignatureRef` with `&'static str` and
`&'static [ParameterRef]`.

## Phases

### Phase 1: Add ParameterRef and FunctionSignatureRef; update LpfxFn

**Scope:** Introduce the ref-based types in `lpfx_fn.rs` and switch `LpfxFn` to use
`FunctionSignatureRef`. The compiler will break until Phase 2 updates consumers.

**Code organization:** Place `ParameterRef` and `FunctionSignatureRef` before `LpfxFn`. Use
`ParamQualifier` from `crate::semantic::functions`. Keep `LpfxFnImpl` unchanged.

**Implementation details:**

1. In `lp-glsl-compiler/src/frontend/semantic/lpfx/lpfx_fn.rs`:
    - Add `ParameterRef` (mirrors `Parameter` with `&'static str`):
      ```rust
      use crate::semantic::functions::ParamQualifier;
 
      #[derive(Debug)]
      pub struct ParameterRef {
          pub name: &'static str,
          pub ty: Type,
          pub qualifier: ParamQualifier,
      }
 
      #[derive(Debug)]
      pub struct FunctionSignatureRef {
          pub name: &'static str,
          pub return_type: Type,
          pub parameters: &'static [ParameterRef],
      }
      ```
    - Change `LpfxFn` to use `FunctionSignatureRef`:
      ```rust
      pub struct LpfxFn {
          pub glsl_sig: FunctionSignatureRef,
          pub impls: LpfxFnImpl,
      }
      ```
    - Remove `use crate::semantic::functions::FunctionSignature;`; add
      `use crate::semantic::types::Type;` if not present.

2. Ensure `ParamQualifier` is re-exported or accessible (it lives in `semantic::functions`).
   `ParameterRef` and `FunctionSignatureRef` use it directly.

**Validate:** `cargo build -p lp-glsl-compiler` — will fail until Phase 2. Run only to confirm the
new types compile; do not run full test suite yet.

---

### Phase 2: Update lpfx_sig and codegen consumers to use ref types

**Scope:** Update all consumers of `LpfxFn.glsl_sig` to work with `FunctionSignatureRef`. Field
names (`.name`, `.parameters`, `.return_type`, `param.ty`, `param.qualifier`) are unchanged — only
the type changes, so most code needs no edits. Fix any `param.ty.clone()` — `Type` is `Copy` so use
`param.ty` or `*param.ty` if needed.

**Implementation details:**

1. **lpfx_sig.rs** (`build_call_signature`): No structural changes — it uses
   `func.glsl_sig.return_type`, `func.glsl_sig.parameters`, `param.ty`, `param.qualifier`. Change
   `param.ty.clone()` to `param.ty` (Type is Copy) if present.

2. **codegen/lpfx_fns.rs** (`emit_lp_lib_fn_call`, etc.): Same — uses `func.glsl_sig.parameters`,
   `func.glsl_sig.return_type`. No signature changes.

3. **codegen/expr/function.rs** (LPFX call path): The LPFX branch uses `func.glsl_sig` from
   `find_lpfx_fn` — no changes if it only reads `.parameters`, `.return_type`, `param.ty`,
   `param.qualifier`.

4. **lpfx_fn_registry.rs**: Update `find_lpfx_fn` and `matches_signature` to use `glsl_sig.name` (
   now `&str` — compare with `== name`), `glsl_sig.parameters.len()`, etc. Remove
   `use alloc::vec::Vec` once Phase 4 refactors the lookup. For now keep the filter/collect logic —
   it will still compile with `FunctionSignatureRef`.

**Validate:** `cargo build -p lp-glsl-compiler` succeeds. Tests will fail because `lpfx_fns.rs`
still has the old `init_functions` format.

---

### Phase 3: Update codegen to emit static array; replace init_functions

**Scope:** Change `lp-glsl-builtins-gen-app` to emit a `static` array with `&'static str` and
`&'static [ParameterRef]` instead of `init_functions()` with `String::from` and `vec![]`. Replace
`lpfx_fns()` body to return the static slice directly.

**Implementation details:**

1. In **lp-glsl-builtins-gen-app/src/lpfx/generate.rs**:
    - Change imports in emitted code: remove `alloc::{boxed::Box, string::String, vec, vec::Vec}`.
      Add `use crate::semantic::functions::ParamQualifier;` and ensure `ParameterRef`,
      `FunctionSignatureRef` are used (from `super::lpfx_fn`).
    - Emit `static LPFX_FNS: &[LpfxFn] = &[` instead of
      `fn init_functions() -> ... { let vec: Vec<LpfxFn> = vec![`.
    - For each function: emit
      `FunctionSignatureRef { name: "lpfx_fbm", return_type: Type::Float, parameters: &[...] },`
      where parameters is a `&[ParameterRef]` literal.
    - Use `ParameterRef { name: "p", ty: Type::Vec2, qualifier: ParamQualifier::In }` etc.
    - For shared parameter arrays (e.g. same params for multiple overloads), emit
      `static PARAMS_XYZ: &[ParameterRef] = &[...];` and reference `parameters: PARAMS_XYZ` — or
      inline `&[ParameterRef { ... }, ...]` per entry. Inlining is simpler; sharing can be a later
      optimization.
    - Remove `Box::leak(vec.into_boxed_slice())` and `init_functions()`. Replace `lpfx_fns()` body
      with `LPFX_FNS` (no OnceLock, no static mut).
    - Emit:
      ```rust
      pub fn lpfx_fns() -> &'static [LpfxFn] {
          LPFX_FNS
      }
      ```

2. **Format helpers** in generate.rs:
    - `format_function_signature` → emit
      `FunctionSignatureRef { name: "..", return_type: ..., parameters: &[ ... ] }`.
    - `format_parameter` → emit `ParameterRef { name: "..", ty: ..., qualifier: ... }`.

3. Run `scripts/build-builtins.sh` to regenerate `lpfx_fns.rs`.

4. Manually fix any first-time generation issues (e.g. escaping in strings, Type::Array which uses
   Box — LPFX params don't use Array, so we're fine).

**Validate:**

- `scripts/build-builtins.sh`
- `cargo test -p lp-glsl-compiler --features std`
- `cargo test -p lp-glsl-filetests` (if LPFX is exercised in filetests)

---

### Phase 4: Refactor find_lpfx_fn to loop-based (no Vec allocs)

**Scope:** Replace the three `filter().collect()` chains in `find_lpfx_fn` with a single loop that
finds the matching function without allocating.

**Implementation details:**

1. In **lpfx_fn_registry.rs**, rewrite `find_lpfx_fn`:
   ```rust
   pub fn find_lpfx_fn(name: &str, arg_types: &[Type]) -> Option<&'static LpfxFn> {
       let functions = get_cached_functions();
       let mut exact_match: Option<&'static LpfxFn> = None;

       for func in functions.iter() {
           if func.glsl_sig.name != name {
               continue;
           }
           if func.glsl_sig.parameters.len() != arg_types.len() {
               continue;
           }
           if !matches_signature(func, arg_types) {
               continue;
           }
           // Found a match
           if exact_match.is_some() {
               return None; // Ambiguous: multiple matches
           }
           exact_match = Some(func);
       }
       exact_match
   }
   ```

2. Remove `use alloc::{format, string::String, vec::Vec};` — keep `format` and `String` if still
   used by `check_lpfx_fn_call` (they are). Remove only `Vec` if it's unused.

**Validate:** `cargo test -p lp-glsl-compiler --features std`

---

### Phase 5: Cleanup and validation

**Scope:** Remove temporary code, fix warnings, run full validation.

**Implementation details:**

1. Grep the diff for TODOs, debug prints, and temporary code. Remove any found.
2. Run `cargo +nightly fmt` on changed files.
3. Run `cargo clippy -p lp-glsl-compiler -p lp-glsl-builtins-gen-app -- -D warnings` and fix any
   issues.
4. Run `cargo test -p lp-glsl-compiler --features std` and `cargo test -p lp-glsl-builtins-gen-app`.
5. Run `scripts/build-builtins.sh` one final time to ensure generation is idempotent.

**Validate:**

- `scripts/build-builtins.sh`
- `cargo test -p lp-glsl-compiler --features std`
- `cargo test -p lp-glsl-builtins-gen-app`

---

# Notes

(Answers from design iteration)

- **Type strategy**: Parallel types. Add `ParameterRef` and `FunctionSignatureRef` in the lpfx
  module.
- **find_lpfx_fn allocations**: Yes, include loop-based refactor. Lower priority.
- **no_std**: With static data, `lpfx_fns()` returns `&LPFX_FNS` directly; no OnceLock.

(Implementation complete)
