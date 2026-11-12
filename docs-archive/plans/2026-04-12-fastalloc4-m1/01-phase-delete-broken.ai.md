# Phase 1: Delete Broken Files

## Scope

Delete the files with the broken direct-emission architecture. This is purely
deletion — no replacements yet.

## Files to Delete

```bash
rm lp-shader/lpvm-native/src/fa_alloc/walk.rs      # 1633 lines
rm lp-shader/lpvm-native/src/rv32/inst.rs           # 240 lines
rm lp-shader/lpvm-native/src/rv32/rv32_emit.rs
rm lp-shader/lpvm-native/src/rv32/debug/pinst.rs
```

## Code Organization Reminders

- This phase is deletion only. We'll fix the compilation errors in later phases.
- Some imports will break — that's expected. We'll clean them up in Phase 5.

## Implementation

1. Delete `fa_alloc/walk.rs`
2. Delete `rv32/inst.rs`
3. Delete `rv32/rv32_emit.rs`
4. Delete `rv32/debug/pinst.rs`

## Validation

```bash
cargo check -p lpvm-native 2>&1 | head -50
```

Expected: Many errors about missing modules, types, and functions. That's fine
for this phase. We just want to see the deletions took effect.

## Temporary Code

None in this phase.
