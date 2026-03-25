# Phase 6: Level 3 `direct_call()`

## Scope

- **`DirectCall`** struct: holds what **`lp-engine`** needs (raw code pointer,
  **`CallConv`**, **`Type`** pointer type, and a **`call(args, results)`** closure
  or **`unsafe fn`**).
- Implement **`JitModule::direct_call(&self, name: &str) -> Option<DirectCall>`**.
- Use **`lp-glsl-jit-util`** where it matches multi-return / struct-return on the
  host target; otherwise document **TODO** for exotic signatures.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

Mirror **`lp-glsl-cranelift`** **`get_direct_call_info`** behavior where
possible so **`lp-engine`** migration (Stage VI) is mechanical.

### Tests

- Smoke: **`direct_call("add")`** on a scalar function, invoke with **`u32`**
  buffers, compare to **`call()`** for same inputs (Q32 mode).

## Validate

```
cargo check -p lpir-cranelift
cargo test -p lpir-cranelift
```
