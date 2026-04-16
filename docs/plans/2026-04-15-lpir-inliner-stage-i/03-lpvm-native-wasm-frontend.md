# Phase 3 — lpvm-native, lpvm-wasm, lps-frontend

## Scope of phase

Migrate the main compiler front: native lowering and compile pipeline, WASM emit/compile (IR↔meta ordering), and GLSL lowering that builds `CalleeRef` / iterates IR functions.

## Code organization reminders

- In `lower.rs`, keep `resolve_callee_name` and `callee_return_uses_sret` structure; swap implementation to enum match.
- For `lpvm-wasm/compile.rs`, document or preserve **zip order** between `ir.functions` and `meta.functions`—after map change, define order explicitly (e.g. sort by `FuncId` then zip with meta sorted the same way, or match by **name** if that is the existing contract—**verify in code before shipping**).

## Implementation details

- **`lpvm-native`:** `lower.rs`, `compile.rs`, `link.rs`, `regalloc/render.rs` (clone or iterate map), `debug_asm.rs`, `rt_jit/*`, `rt_emu/*`—replace `functions[idx]` with `FuncId`→lookup or ordered vec of `(FuncId, &IrFunction)` where linear index is still needed for ABI tables.
- **`lpvm-wasm`:** `emit/mod.rs`, `emit/imports.rs`, `emit/ops.rs`, `compile.rs`, runtime `instance.rs` files.
- **`lps-frontend`:** `lower.rs`, `lower_ctx.rs`, `lower_lpfx.rs`—construct typed `CalleeRef`; any `ir.functions.len()` / indexing in tests (`lib.rs`).

## Tests to write

- Rely on crate tests; fix breakages from API change.

## Validate

When these crates compile again:

```bash
cargo test -p lpvm-native
cargo test -p lpvm-wasm
cargo test -p lps-frontend
```
