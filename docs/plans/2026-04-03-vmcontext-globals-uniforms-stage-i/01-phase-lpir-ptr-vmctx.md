# Phase 1: LPIR `ptr` type and vmctx typing

## Objective

Introduce **`IrType::Pointer`** and surface it in text as **`ptr`**. **`v0` (VMCTX_VREG)** defaults to **`ptr`**. Validation injects **`ptr`** (not `i32`) as the leading logical import parameter when `ImportDecl::needs_vmctx`. Interp carries **`ptr` as i32 bits** (same storage as today’s scalars or a single `Value` representation — no `usize`).

## Tasks

1. **`types.rs`** — Add `IrType::Pointer` (or name `Ptr` in Rust enum). Ensure `Display`, equality, and any `IrType` matches cover the new variant.
2. **`parse.rs` / `print.rs`** — Accept and emit `ptr` in function headers, vreg type lists, and return lists (mirror `i32`/`f32` patterns).
3. **`builder.rs`** — `FunctionBuilder::new`: `vreg_types[0] = IrType::Pointer` for vmctx; update comment (remove “32-bit VMContext pointer” as a type width claim — it is target-abstract `ptr`).
4. **`validate.rs`** — `needs_vmctx` import param injection: push `IrType::Pointer` instead of `I32`. Ensure callee matching uses `ptr` for that slot. Keep **`Op::SlotAddr` result type as `I32` for this phase** unless you batch with Phase 3 (if SlotAddr moves in phase 1, update memory-related expectations in tests accordingly).
5. **`interp.rs`** — For `ptr` vregs, use the same scalar path as `i32` (document in code comment: abstract address / offset, not host pointer width).
6. **Tests** — `tests/validate.rs`, `tests/all_ops_roundtrip.rs`, `tests/interp.rs`: fixtures use `ptr` for `v0` where appropriate; add a small roundtrip for `ptr` in signatures if missing.

## Exit criteria

- `cargo test -p lpir` passes.
- Text roundtrip: function with `(ptr, …)` params and `ptr` returns parses and prints consistently.

## Dependency note

If Phase 1 lands alone, **`lpir-cranelift` and `lp-glsl-naga` may fail** until they map `Pointer` → Cranelift type. Prefer **Phase 2 in the same merge train** or feature branch that includes both.
