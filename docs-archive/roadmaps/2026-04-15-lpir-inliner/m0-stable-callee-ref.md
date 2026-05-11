# M0 — Stable CalleeRef Refactor

**Prerequisite milestone.** Decouple function/import identification from
`Vec` indices so that function deletion (dead function elimination after
inlining) does not invalidate references.

## Problem

`CalleeRef(u32)` is a single flat index: `[0..n_imports)` for imports,
`[n_imports..n_imports+n_funcs)` for local functions. Every consumer does
arithmetic to split the two:

```rust
// In LpirModule:
pub fn callee_as_import(&self, callee: CalleeRef) -> Option<usize> {
    let i = callee.0 as usize;
    if i < self.imports.len() { Some(i) } else { None }
}
pub fn callee_as_function(&self, callee: CalleeRef) -> Option<usize> {
    let i = callee.0 as usize;
    let n = self.imports.len();
    if i >= n { Some(i - n) } else { None }
}
```

Deleting a function shifts every `CalleeRef` that refers to a later function.
The inliner needs to delete fully-inlined functions without breaking the
module.

## Solution

Replace the flat index with a typed enum:

```rust
// In lpir/src/types.rs:
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ImportId(pub u16);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FuncId(pub u16);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CalleeRef {
    Import(ImportId),
    Local(FuncId),
}
```

`ImportId` and `FuncId` are stable identifiers — they don't change when
other items are added or removed. `LpirModule` stores functions in a
structure keyed by `FuncId` (either a `BTreeMap<FuncId, IrFunction>` or a
`Vec` with a separate id→index mapping — the simpler option is fine since
function counts are small).

## Changes by file

### `lpir` crate

| File | Change |
|------|--------|
| `types.rs` | Add `ImportId(u16)`, `FuncId(u16)`, change `CalleeRef` to enum. Add `Display` impls. |
| `lpir_module.rs` | Replace `callee_ref_import`/`callee_ref_function`/`callee_as_import`/`callee_as_function` with direct `CalleeRef::Import`/`CalleeRef::Local` construction. Consider storing `FuncId` on `IrFunction`. |
| `lpir_op.rs` | No structural change — `Call { callee: CalleeRef, .. }` stays the same, just the type changes shape. |
| `builder.rs` | `add_import` returns `CalleeRef::Import(ImportId(n))`. `add_function` returns `CalleeRef::Local(FuncId(n))`. No split arithmetic. |
| `print.rs` | `callee_name()` matches on `CalleeRef::Import` / `CalleeRef::Local` instead of index arithmetic. |
| `parse.rs` | `names: Vec<(String, CalleeRef)>` — construct `CalleeRef::Import(ImportId(i))` or `CalleeRef::Local(FuncId(i))`. |
| `validate.rs` | `validate_call` matches on the enum instead of comparing against import count. |
| `interp.rs` | Same — match on `CalleeRef::Import` / `CalleeRef::Local`. |
| `const_fold.rs` | No change (doesn't touch CalleeRef). |
| `lib.rs` | Re-export `ImportId`, `FuncId`. |

### `lpvm-native` crate

| File | Change |
|------|--------|
| `lower.rs` | `resolve_callee_name` and `callee_return_uses_sret` match on enum instead of index math. |
| `regalloc/render.rs` | Comment about CalleeRef indices — update comment, logic unchanged. |

### `lpvm-wasm` crate

| File | Change |
|------|--------|
| `emit/ops.rs` | `wasm_func_index` matches on enum. |
| `emit/imports.rs` | `find_import_callee_ref` returns `CalleeRef::Import(ImportId(i))`. |

### `lps-frontend` crate

| File | Change |
|------|--------|
| `lower.rs` | `func_map` builds `CalleeRef::Local(FuncId(i))`. `register_math_imports` returns `CalleeRef::Import(ImportId(i))`. |
| `lower_ctx.rs` | Type signatures change (`BTreeMap<Handle<Function>, CalleeRef>` is fine — CalleeRef type just changed shape). |
| `lower_lpfn.rs` | Same — construct `CalleeRef::Import` instead of `CalleeRef(i)`. |

### Tests

| Location | Change |
|----------|--------|
| `lpir/src/tests/` | Update any test that constructs `CalleeRef(n)` to use the enum. |
| `lps-filetests/` | No change — filetests parse GLSL, don't construct CalleeRef directly. |
| `lpvm-native/src/compile.rs` tests | No direct CalleeRef construction in current tests. |

## Validation

```bash
cargo test -p lpir
cargo test -p lpvm-native
cargo test -p lpvm-wasm
cargo test -p lps-frontend
cargo test -p lps-filetests -- --test-threads=4
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

## Estimated scope

~15 files touched, mostly mechanical find-and-replace of index arithmetic
with enum match arms. No behavioral change — all existing tests should
pass without modification to expected outputs.
