# M5: Validation and Cleanup

## Scope of Work

Run comprehensive tests, handle edge cases, and prepare for removing the old pipeline.

## Testing Plan

### 1. Expand Filetest Coverage

Run the full filetest suite with `RegAllocAlgorithm::Fast`:

```bash
# All scalar int tests
cargo test -p lps-filetests --test filetest -- scalar/int/

# All scalar float tests
cargo test -p lps-filetests --test filetest -- scalar/float/

# Complex shaders (rainbow_flat - no control flow)
cargo test -p lps-filetests --test filetest -- rainbow_flat
```

### 2. Edge Cases to Verify

| Case | Expected Behavior |
|------|-------------------|
| Empty function | FrameSetup + FrameTeardown only |
| Single return | Minimal prologue/epilogue |
| Many args (>8) | Stack args handled correctly |
| SRET return (vec3/vec4) | Hidden pointer in a0, s1 preserved |
| Deeply nested calls | Caller-save spill/reload balanced |
| High register pressure | LRU eviction, spills to stack |
| IConst32 everywhere | Rematerialization, no stack slots |
| Call with many live vars | Correct spill set (only caller-saved) |

### 3. Performance Comparison

Measure vs old pipeline:

```bash
# Instruction count
cargo test -p lps-filetests --test filetest -- rainbow_flat --nocapture 2>&1 | grep "instruction count"

# Compile time (rough)
time cargo test -p lps-filetests --test filetest -- rainbow_flat
```

### 4. Remove Old Pipeline (when ready)

After all filetests pass:

```bash
# 1. Move rv32/ to archive or delete
rm -rf lp-shader/lpvm-native/src/isa/rv32/

# 2. Rename rv32fa/ to rv32/
mv lp-shader/lpvm-native/src/isa/rv32fa lp-shader/lpvm-native/src/isa/rv32

# 3. Update all module references
# 4. Remove RegAllocAlgorithm::LinearScan and ::Greedy from config
# 5. Remove old regalloc/ modules (greedy.rs, linear_scan.rs, adapter.rs)
```

## Cleanup Checklist

- [ ] No TODO comments left in new code
- [ ] No debug println! or eprintln!
- [ ] All warnings fixed
- [ ] rustfmt applied
- [ ] All tests pass
- [ ] Documentation updated
- [ ] Plan moved to docs/plans-done/

## Final Validation

```bash
cd lp-shader/lpvm-native

# Check (no_std + alloc)
cargo check --target riscv32imac-unknown-none-elf --features esp32c6,server

# Tests
cargo test --lib

# Filetests (comprehensive)
cargo test -p lps-filetests --test filetest
```

## Success Criteria

1. All filetests pass with `RegAllocAlgorithm::Fast`
2. Instruction count is competitive with (or better than) old pipeline
3. Debug trace is useful for understanding any remaining issues
4. Old rv32/ pipeline can be safely removed
5. Code is clean, documented, and maintainable

## Post-M5: Future Work

- M6: Control flow support (if/else, loops)
- M7: Optimizations (param-to-callee-saved, better eviction heuristics)
- M8: Float support when added to VInst
