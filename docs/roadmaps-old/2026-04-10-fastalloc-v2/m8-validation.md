# M8: Validation and Cleanup

## Scope of Work

Run comprehensive filetests, fix edge cases, clean up code, and prepare for removing the old pipeline.

## Testing Plan

### Phase 1: Core Filetests

```bash
# Scalar int operations
cargo test -p lps-filetests --test filetest -- scalar/int/

# Scalar float operations
cargo test -p lps-filetests --test filetest -- scalar/float/

# Specific tests known to work with straight-line code
cargo test -p lps-filetests --test filetest -- native-rv32-iadd
cargo test -p lps-filetests --test filetest -- native-rv32-sub
cargo test -p lps-filetests --test filetest -- native-rv32-mul
```

### Phase 2: Debug Filetests

Test the minimal failing cases:

```bash
# The debug cases
cargo test -p lps-filetests --test filetest -- debug1  # Minimal GLSL from v1
cargo test -p lps-filetests --test filetest -- debug/rainbow_flat  # No control flow rainbow
```

### Phase 3: Expected Control Flow Failures

These should fail with clear messages:

```bash
# Tests with if/else (control flow)
cargo test -p lps-filetests --test filetest -- control_flow/if_simple 2>&1 | grep "FastallocUnsupportedControlFlow"

# Tests with loops
cargo test -p lps-filetests --test filetest -- control_flow/loop_simple 2>&1 | grep "FastallocUnsupportedControlFlow"
```

### Phase 4: Edge Cases

| Edge Case | Test |
|-----------|------|
| Empty function | `fn empty() {}` |
| Single return | `fn ret() -> float { return 1.0; }` |
| Many locals | Stress test with 20+ vregs |
| Deep call chain | Function calling function calling builtin |
| SRET return | `vec3` return type |
| Large constants | Values needing lui+addi |
| Zero args / many args | Edge of ABI |

## Performance Comparison

Measure vs old pipeline:

```bash
# Instruction count (from filetest output)
cargo test -p lps-filetests --test filetest -- rainbow_flat --nocapture 2>&1 | grep -E "(instruction count|cycles)"

# Compile time
time cargo test -p lps-filetests --test filetest -- rainbow_flat
```

## Cleanup Checklist

### Code Quality

- [ ] No `todo!()` or `unimplemented!()` left in production paths
- [ ] No `println!` or `eprintln!` (use `log::debug`)
- [ ] All warnings fixed (`cargo check --lib` clean)
- [ ] rustfmt applied (`cargo fmt`)
- [ ] Clippy clean (`cargo clippy --lib`)

### Documentation

- [ ] Module-level docs for rv32fa/
- [ ] Function docs for public APIs
- [ ] Examples in doc comments where helpful
- [ ] README or ARCHITECTURE.md if needed

### Tests

- [ ] Unit tests for all parser/formatter pairs (roundtrip)
- [ ] Unit tests for each emitter instruction variant
- [ ] Unit tests for allocator decisions (spill, reload, remat)
- [ ] Unit tests for call clobber handling
- [ ] Filetests passing for straight-line cases
- [ ] Filetests correctly failing for control flow cases

### Temporaries Removed

```bash
# Find and review all TODO/FIXME/TEMP
grep -r "TODO\|FIXME\|TEMP\|XXX" lp-shader/lpvm-native/src/isa/rv32fa/ | grep -v "target/"

# Find debug prints
grep -r "println!\|eprintln!" lp-shader/lpvm-native/src/isa/rv32fa/ | grep -v "target/"
```

## Remove Old Pipeline (Optional)

When ready, delete the old code:

```bash
# 1. Move rv32/ to archive or delete
rm -rf lp-shader/lpvm-native/src/isa/rv32/

# 2. Rename rv32fa/ to rv32/
mv lp-shader/lpvm-native/src/isa/rv32fa lp-shader/lpvm-native/src/isa/rv32

# 3. Update all references
sed -i 's/rv32fa/rv32/g' lp-shader/lpvm-native/src/isa/mod.rs

# 4. Remove old regalloc modules
rm lp-shader/lpvm-native/src/regalloc/greedy.rs
rm lp-shader/lpvm-native/src/regalloc/linear_scan.rs
rm lp-shader/lpvm-native/src/regalloc/adapter.rs

# 5. Update config
# - Remove RegAllocAlgorithm::LinearScan and ::Greedy
# - Make Fast the default and only option
```

## Final Validation

```bash
# Full test suite
cargo test -p lpvm-native --lib
cargo test -p lps-filetests --test filetest

# no_std build
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# Firmware build
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

## Success Criteria

1. **All straight-line filetests pass**
2. **Control flow cases fail with clear error** (not panic, not obscure)
3. **Trace is useful for debugging** (can understand allocator decisions)
4. **Instruction count competitive** (within 10-20% of old pipeline)
5. **Compile time reasonable** (not significantly slower)
6. **Code is clean** (no warnings, no TODOs, well documented)
7. **Ready for M3** (control flow support in future roadmap)

## Post-M8: Future Work

- **M3 (Control Flow)**: if/else, loops, block boundaries
- **M4 (Optimizations)**: Better heuristics, callee-saved preference for params
- **M5 (Float Support)**: When VInst adds float variants

## Move Plan to Done

```bash
mkdir -p docs/plans-done/2026-04-10-fastalloc-v2
cp docs/roadmaps/2026-04-10-fastalloc-v2/* docs/plans-done/2026-04-10-fastalloc-v2/
echo "# Summary" > docs/plans-done/2026-04-10-fastalloc-v2/summary.md
# ... write summary ...
```
