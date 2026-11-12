# Phase 4: WASM `LpvmInstance` — defaults

## Scope of phase

Update **wasmtime** and **browser** `LpvmInstance` implementations to satisfy the extended trait:

- **`call_q32`:** Use **trait default** (F32 round-trip or shared helper from `lpvm`) unless a straight exact path is trivial; filetests for `wasm.q32` should still pass.
- **`debug_state`:** `None`.

## Code Organization Reminders

- Keep wasmtime and browser in sync (same default behavior).

## Implementation Details

- If default from `lpvm` is insufficient for `wasm.q32` filetests, implement exact path in wasmtime instance by reusing the same `LpsValueQ32` + flatten path as native `call`.

## Validate

```bash
cargo check -p lpvm-wasm
cargo test -p lpvm-wasm
```
