# Phase 8: Cleanup

## Scope

Final cleanup: fix warnings, verify all tests pass, document remaining TODOs.

## Implementation

### 1. Fix warnings

```bash
cargo clippy -p lpvm-native-fa --lib -- -W clippy::all
```

Fix any new unused imports, dead code, redundant casts.

### 2. Verify all tests pass

```bash
cargo test -p lpvm-native-fa --lib
```

### 3. Verify existing filetests still pass

```bash
cargo test -p lpvm-native-fa
```

### 4. Verify CLI works

```bash
cargo build -p lp-cli

# Region tree display
./target/debug/lp-cli shader-rv32fa lp-shader/lps-filetests/filetests/debug/native-rv32-iadd.glsl --show-region

# Liveness display
./target/debug/lp-cli shader-rv32fa lp-shader/lps-filetests/filetests/debug/native-rv32-iadd.glsl --show-liveness
```

### 5. Check TODOs

```bash
grep -r "TODO" lp-shader/lpvm-native-fa/src/alloc/
```

Only acceptable TODOs are ones marking where M5 work begins (IfThenElse/Loop liveness, real allocation decisions).

### 6. Fix link.rs warnings from M3.2

The 4 warnings in `link.rs` (unused fields on `ElfBuilder`/`ElfSection`, redundant `u32 as u32` casts) — fix these while we're cleaning up.

## Success Criteria

1. No new compiler warnings in `lpvm-native-fa`
2. All existing tests still pass
3. New `alloc/` module tests pass
4. CLI `--show-region` and `--show-liveness` produce output
5. Region tree is populated for all lowered functions
6. Code is clean and ready for M5

## Validate

```bash
cargo test -p lpvm-native-fa
cargo build -p lp-cli
```
