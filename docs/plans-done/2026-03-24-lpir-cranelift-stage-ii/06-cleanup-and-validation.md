# Phase 6: Cleanup & Validation

## Scope

Final review, remove temporary code, fix warnings, verify all tests pass.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Cleanup

- Grep the git diff for `TODO`, `FIXME`, `HACK`, `dbg!`, `println!`,
  `#[allow(dead_code)]`, or other temporary markers. Remove or resolve them.
- Remove any `#[allow(unused)]` that is no longer needed.
- Check that the catch-all `_ =>` in the op dispatch has a clear error message
  listing what's not yet supported (imports — Stage III).
- Verify `emit/mod.rs` doc comment accurately describes the stage.
- Verify `lib.rs` doc comment is updated.

### 2. Formatting

```
cargo +nightly fmt -p lpvm-cranelift
```

### 3. Warnings

```
cargo clippy -p lpvm-cranelift -- -D warnings
```

Fix all warnings.

### 4. Full test run

```
cargo test -p lpvm-cranelift
```

All tests pass.

### 5. Broader check

Make sure nothing else broke:

```
cargo check -p lpir
cargo test -p lpir
```

### 6. Plan cleanup

Add a summary of the completed work to
`docs/plans/2026-03-24-lpvm-cranelift-stage-ii/summary.md`.

Move the plan directory to `docs/plans-done/`.

### 7. Commit

```
feat(lpvm-cranelift): LPIR → CLIF emitter core

- Restructured emit.rs into emit/ module (scalar, control, memory, call)
- Structured control flow: if/else, loops, break/continue/brifnot, switch
- Memory ops: stack slots, load/store, memcpy
- Local function calls with multi-return
- EmitCtx for FuncRef/StackSlot/module context
- seal_all_blocks() strategy for SSA construction
- Tests for each feature
```

## Validate

```
cargo +nightly fmt -p lpvm-cranelift
cargo clippy -p lpvm-cranelift -- -D warnings
cargo test -p lpvm-cranelift
cargo check -p lpir
cargo test -p lpir
```
