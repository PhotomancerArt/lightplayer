# Quick Guide: Tracing What Gets Compiled into embive-program

## TL;DR - Binary is BIG because:

1. **Debug info is included** (binary is "not stripped")
2. **Cranelift's optimizer code** (constructor_simplify: 147 KB alone)
3. **Verifier code** (19 KB)
4. **Register allocator** (regalloc2: ~55 KB)
5. **Debug/formatting code** (print_with_state, Debug::fmt, etc.)

**Current size:** 2.3 MB → **Possible size after optimization:** 300-700 KB

---

## Quick Commands to Trace What's Included

### 1. Show All Dependencies

```bash
cargo tree --package embive-program --target riscv32imac-unknown-none-elf
```

### 2. Show Size Breakdown by Function

```bash
cargo bloat --release --target riscv32imac-unknown-none-elf --package embive-program -n 50
```

Output shows top 50 space-consuming functions.

### 3. Show Size Breakdown by Crate

```bash
cargo bloat --release --target riscv32imac-unknown-none-elf --package embive-program --crates
```

### 4. Trace Compilation (See What Files Get Compiled)

```bash
# Clean and rebuild with verbose output
cargo clean -p embive-program
cargo build --package embive-program --target riscv32imac-unknown-none-elf --release -v 2>&1 | \
  grep "Running.*rustc" | head -50
```

### 5. Check Features Enabled on Dependencies

```bash
cargo tree --package embive-program --target riscv32imac-unknown-none-elf -e features
```

### 6. Find Why a Specific Crate is Included

```bash
# Replace CRATE_NAME with the crate you're investigating
cargo tree --package embive-program --target riscv32imac-unknown-none-elf -i CRATE_NAME
```

---

## Immediate Size Wins

### Strip Debug Info (BIGGEST IMPACT)

Your binary currently includes debug info! Add to `apps/embive-program/Cargo.toml`:

```toml
[profile.release]
strip = true  # Automatically strip debug info
```

Or manually strip after build:
```bash
# Install llvm-tools if needed
rustup component add llvm-tools

# Strip the binary
llvm-strip target/riscv32imac-unknown-none-elf/release/embive-program
```

Expected savings: **500 KB - 1.5 MB**

### Enable LTO and Optimize for Size

Add to `apps/embive-program/Cargo.toml`:

```toml
[profile.release]
opt-level = "z"     # Optimize for size
lto = true          # Link-time optimization
codegen-units = 1   # Better optimization (slower compile)
strip = true        # Strip symbols
panic = "abort"     # Smaller panic handler
```

Expected savings: **200-500 KB additional**

---

## Using the Analysis Script

I created `trace-compiled-files.sh` for comprehensive analysis:

```bash
./trace-compiled-files.sh
```

This creates `analysis-output/` directory with:
- `dep-tree.txt` - Full dependency tree
- `dep-tree-features.txt` - Dependencies with features
- `bloat-report.txt` - Top 30 space-consuming functions
- `bloat-crates.txt` - Space usage by crate
- `build-verbose.log` - Full build log
- `binary-size.txt` - Section sizes

---

## What's Actually Getting Compiled In

Based on cargo bloat analysis, here are the top space consumers:

### Cranelift Codegen Components (LARGE!)

| Component | Approximate Size | Can Remove? |
|-----------|-----------------|-------------|
| Optimizer passes | ~150 KB | Feature-gate in cranelift-codegen |
| RISC-V lowering | ~44 KB | ❌ (needed) |
| Verifier | ~20 KB | ✅ Already disabled in settings |
| Register allocator | ~55 KB | ⚠️ Needed but might have minimal mode |
| Instruction emission | ~27 KB | ❌ (needed) |
| Debug formatting | ~20 KB | ✅ Feature-gate Debug impls |

### Full Dependency List (from cargo tree)

```
embive-program
├── cranelift-codegen ← LARGEST (optimizer, lowering, regalloc)
├── cranelift-frontend
├── cranelift-jit
├── cranelift-module
├── cranelift-control
├── lp-toy-lang (parser using nom)
├── runtime-embive
├── hashbrown (hash maps)
└── target-lexicon
```

Supporting crates:
- `regalloc2` - Register allocator (pulled by cranelift-codegen)
- `bumpalo` - Arena allocator (pulled by cranelift-codegen)
- `nom` - Parser combinator (pulled by lp-toy-lang)
- `hashbrown` - Hash maps
- `libm` - Math functions (no_std)
- `anyhow` - Error handling

**Total unique crates:** ~25-30

---

## Advanced: Feature Flags Analysis

### Check What Features Are Enabled

```bash
# On embive-program:
cargo tree -p embive-program --target riscv32imac-unknown-none-elf -e features -i embive-program

# On cranelift-codegen:
cargo tree -p embive-program --target riscv32imac-unknown-none-elf -e features -i cranelift-codegen
```

### Current Feature Configuration

From `apps/embive-program/Cargo.toml`:

```toml
cranelift-codegen = { 
    default-features = false, 
    features = ["riscv32"]  # Only RISC-V32 backend
}
cranelift-frontend = { 
    default-features = false, 
    features = ["core"]  # no_std support
}
cranelift-jit = { 
    default-features = false, 
    features = ["core"]
}
cranelift-module = { 
    default-features = false, 
    features = ["core"]
}
```

This is already pretty minimal! The size comes from:
1. Debug info in the binary itself
2. Code that's compiled in even with minimal features

---

## Compiler Flags for Minimum Size

Create or edit `.cargo/config.toml` in the project root:

```toml
[target.riscv32imac-unknown-none-elf]
rustflags = [
    "-C", "link-arg=-Wl,--gc-sections",  # Remove unused sections
    "-Z", "trap-unreachable=no",         # Smaller code gen (nightly only)
]

[build]
target = "riscv32imac-unknown-none-elf"

[profile.release]
opt-level = "z"         # Optimize for size
lto = "fat"            # Aggressive LTO
codegen-units = 1      # Single codegen unit
strip = true           # Strip symbols
panic = "abort"        # Abort on panic
overflow-checks = false # Remove overflow checks
```

**Note:** Some flags require nightly Rust.

---

## Debugging Size Issues

### If a crate shows up unexpectedly:

```bash
# Find out why it's included:
cargo tree -p embive-program --target riscv32imac-unknown-none-elf -i UNEXPECTED_CRATE -e normal
```

### Compare with another minimal no_std binary:

```bash
# Build another minimal example:
cargo build --package test-nostd-host --release

# Compare sizes:
ls -lh target/release/test-nostd-host
ls -lh target/riscv32imac-unknown-none-elf/release/embive-program
```

### Use cargo expand to see what macros generate:

```bash
cargo install cargo-expand
cargo expand --package embive-program --target riscv32imac-unknown-none-elf | less
```

---

## Expected Results After Optimization

| Optimization | Current | After | Savings |
|-------------|---------|-------|---------|
| Original | 2.3 MB | - | - |
| + Strip debug info | 2.3 MB | ~800 KB | ~1.5 MB |
| + LTO opt-level=z | ~800 KB | ~500 KB | ~300 KB |
| + Remove unused features | ~500 KB | ~400 KB | ~100 KB |
| **Final target** | - | **300-500 KB** | **~2 MB saved** |

---

## Tools You Need

```bash
# Install cargo-bloat for size analysis:
cargo install cargo-bloat

# Install llvm-tools for stripping:
rustup component add llvm-tools

# Install cargo-expand for macro debugging:
cargo install cargo-expand

# Install cargo-tree (already part of cargo):
# (no installation needed)
```

---

## Next Steps

1. **Immediate:** Add `strip = true` and `opt-level = "z"` to release profile
2. **Quick:** Run `cargo bloat --crates` to confirm what's using space
3. **Investigation:** Look at feature-gating debug/verification code in cranelift
4. **Advanced:** Consider building a minimal cranelift backend with only needed features

For detailed analysis results, see `SIZE_ANALYSIS.md`.




