# Phase 1: Test Infrastructure

**Status:** Implemented (2026-04-13).

## Scope

Add the filetest and builder capabilities needed to test call allocation. No
allocator logic changes yet — just tooling.

## Implementation

### 1. Filetest `; import:` directive

In `filetest.rs`, parse a new directive that declares callee imports:

```
; import: helper(i32, i32) -> i32
; import: big_return(i32) -> vec4
```

The parser sets up the import in the LPIR module (`IrFunction.imports` or
equivalent) so `LpirOp::Call` referencing `helper` lowers to `VInst::Call` with
the correct arg/ret counts and `callee_uses_sret` flag.

Format: `; import: name(param_types) -> return_type`
- `param_types`: comma-separated `i32` (all scalars for now)
- `return_type`: `i32`, `void`, `vec2`, `vec4`, `mat4` etc.
- `callee_uses_sret` derived from return type word count > SRET_SCALAR_THRESHOLD

### 2. Render `EditPoint::After` in snapshot output

In `render.rs`, after the VInst line and write annotations, render After-edits:

```
; spill: t0 -> slot0          ; Before(2) edit
; read: i1 <- t0
    Call @helper i1
; write: i2 -> a0
; reload: slot0 -> t0         ; After(2) edit
```

Update `push_vinst_snapshot_block` / `push_vinst_snapshot_block_raw` to collect
and render `After(inst_idx)` edits following the instruction block.

### 3. Render `VInst::Call` in snapshot output

Ensure `VInst::Call` prints with symbol name and args:

```
    Call @helper i1, i2
```

Check the existing `VInst::format_with_allocs` handles Call. If not, add it.

### 4. Builder `.call()` method

In `fa_alloc/test/builder.rs`, add:

```rust
pub fn call(
    &mut self,
    target: &str,
    args: &[&str],
    rets: &[&str],
    callee_uses_sret: bool,
) -> &mut Self
```

This emits a `VInst::Call` with the given args/rets (vreg names like `"i0"`),
registers the target symbol, and sets `callee_uses_sret`.

### 5. Builder `abi_return` for callee-side sret

Add `abi_return(type)` to the builder so tests can declare the function's own
return method (needed later for callee-side sret testing).

## Validation

```bash
# Existing tests still pass
cargo test -p lpvm-native

# Builder can construct a call (add a smoke test)
cargo test -p lpvm-native builder::call
```

## Success Criteria

- `; import:` directive parsed without error
- Builder can emit VInst::Call
- After-edits render in snapshot output
- No regressions
