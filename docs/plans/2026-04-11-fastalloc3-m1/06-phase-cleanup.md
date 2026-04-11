# Phase 6: Cleanup

## Scope

Remove dead stub code, fix warnings, ensure formatting is clean.

## Cleanup checklist

- [ ] Remove `walk_region_stub` and `stub_entry`/`stub_detail` from `walk.rs`
      (or keep behind `#[cfg(test)]` if any test still uses them)
- [ ] Remove or deprecate `run_shell` in `mod.rs` (replaced by `allocate`)
- [ ] Fix any unused import warnings in `fa_alloc/` modules
- [ ] Fix any dead code warnings for helper functions
- [ ] Ensure `#[allow(dead_code)]` is only on things genuinely needed for M2+
- [ ] Grep for TODO/FIXME — resolve or document what milestone they're for
- [ ] Run `cargo +nightly fmt` on all changed files

## Validate

```bash
# All fa_alloc tests pass
cargo test -p lpvm-native-fa --lib -- fa_alloc

# No warnings
cargo check -p lpvm-native-fa 2>&1 | grep warning

# Format
cargo +nightly fmt -- --check
```

## Plan cleanup

Add summary to `summary.md`. Move plan files to `docs/plans-done/`.

## Commit

```
feat(native-fa): implement backward-walk register allocator core

- Add SpillAlloc for frame-pointer-relative spill slot management
- Add RegPool with LRU eviction for physical register tracking
- Replace stubbed fa_alloc walk with real allocation producing Vec<PInst>
- Handle straight-line code (Linear + Seq regions)
- Emit spill/reload for register pressure, param fixups for ABI regs
- Unit tests for allocation, spill, precoloring, trace output
```
