# Phase 1: `LpvmInstance` — `call_q32` + `debug_state`

## Scope of phase

Extend `LpvmInstance` in `lp-shader/lpvm/src/instance.rs` with:

- **`call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error>`**  
  - Document: arguments are **concatenated** flattened words in parameter order, same as successive `flatten_q32_arg` results for each formal parameter.  
  - Return: flattened return value words only (`void` → empty `Vec`). If `GlslReturn` / `out` params need representation later, extend the signature in this phase only if already required by `decode_q32_return`; otherwise note a follow-up.

- **`debug_state(&self) -> Option<alloc::string::String>`**  
  - Default: `None`.

**Default `call_q32`:** Implement in terms of `call` only if feasible without panicking (e.g. convert flat `i32` → `LpsValueQ32` per signature — may need metadata hook). If a correct default is too heavy, provide `unimplemented!` behind a clear comment only for **non-defaulting** backends in later phases — prefer a **lossy F32 round-trip default** that compiles for all backends until overridden.

Update `lpvm` crate docs in `lib.rs` to describe the two call paths.

## Code Organization Reminders

- One concept per file where possible; keep trait methods on `instance.rs`.
- Default bodies at bottom of impl block or use trait default methods.
- TODO only for deliberate temporary stubs (avoid leaving `unimplemented!` in default release path).

## Implementation Details

- Reuse **`CallError`** from `lpvm_abi` where type mismatches mirror `call` path, or map to `Self::Error` consistently.
- Consider a shared **private helper** on `lpvm` that converts flat args to `LpsValueQ32` given `&[FnParam]` + `&LpsFnSig` if needed for default `call_q32` (may duplicate filetest logic — keep minimal).
- Add **unit tests** on `lpvm` if any pure helpers are extracted (e.g. word-count validation vs `glsl_component_count` sum).

## Validate

```bash
cargo check -p lpvm
cargo test -p lpvm
```

Fix new warnings in touched files.
