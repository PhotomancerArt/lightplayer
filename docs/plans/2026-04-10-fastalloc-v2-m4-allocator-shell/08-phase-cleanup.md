# Phase 8: Cleanup

## Scope

Final cleanup: remove old `alloc.rs`, fix warnings, verify all tests pass.

## Implementation

### 1. Remove old `alloc.rs`

```bash
rm lp-shader/lpvm-native/src/isa/rv32fa/alloc.rs
```

### 2. Verify module structure

Ensure `rv32fa/mod.rs` exports the new alloc module:

```rust
pub mod alloc;  // This now points to alloc/ directory
```

### 3. Fix any warnings

Check for unused imports, dead code, etc:

```bash
cargo clippy -p lpvm-native --lib
```

### 4. Verify all rv32fa tests pass

```bash
cargo test -p lpvm-native --lib -- rv32fa
```

### 5. Verify CLI works

```bash
# Build
cargo build -p lp-cli

# Test with actual shader
./target/debug/lp-cli shader-rv32fa lp-shader/lps-filetests/filetests/debug/native-rv32-iadd.glsl --show-cfg --show-liveness
```

### 6. Check for TODOs

```bash
grep -r "TODO" lp-shader/lpvm-native/src/isa/rv32fa/alloc/
```

Only acceptable TODOs are ones marking where M5 work begins (real allocation decisions).

## Success Criteria

1. Old `alloc.rs` deleted
2. No compiler warnings
3. All 17+ rv32fa tests pass
4. CLI `--show-cfg` and `--show-liveness` work
5. Trace displays stubbed decisions correctly
6. Code is clean and ready for M5

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa
cargo build -p lp-cli
./target/debug/lp-cli shader-rv32fa file.glsl --show-cfg
```
