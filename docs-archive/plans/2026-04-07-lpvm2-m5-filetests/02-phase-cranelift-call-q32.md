# Phase 2: `CraneliftInstance` — `call_q32` + `debug_state`

## Scope of phase

Implement **`call_q32`** and **`debug_state`** on `CraneliftInstance` in `lp-shader/lpvm-cranelift/src/lpvm_instance.rs` (or equivalent).

- **`call_q32`:** Exact Q32 path: reuse the same **machine invocation** used when `call` converts `LpsValueF32` → `LpsValueQ32` → flatten (e.g. delegate to `JitModule` / internal `call` with `LpsValueQ32` built from flat words via **`decode`-inverse** or a dedicated **flat → `LpsValueQ32`** builder using metadata). Must **not** round-trip float lanes through `f32`.
- **`debug_state`:** Return `None` initially, or a short JIT/host string if cheap (optional).

## Code Organization Reminders

- Prefer delegating to existing `JitModule::call` with `LpsValueQ32` if that API exists and accepts semantic Q32 args.
- Helpers for flat `i32` ↔ `LpsValueQ32` at bottom of file or module.

## Implementation Details

- Mirror **`lpvm_abi`** layout: for each parameter, `flatten_q32_arg` defines word count; **`call_q32` args slice** must be split per parameter before building `LpsValueQ32` values (or call an existing internal that already takes flat `Vec<i32>`).
- Return: run existing decode path, then **re-flatten** return `LpsValueQ32` to `Vec<i32>` for the trait, or call a small **`encode_q32_return`** if added alongside `decode_q32_return` in `lpvm_abi` (only if missing — prefer reusing existing encode logic from tests or cranelift `call.rs`).

## Validate

```bash
cargo check -p lpvm-cranelift --features glsl,std
cargo test -p lpvm-cranelift
```
