# How to Trace What Gets Compiled into embive-program

## The Problem

Your riscv32 embedded binary (`embive-program`) is **3.1 MB** (2.3 MB actual, rest is debug info). 

That's huge for embedded! Let's find out what's being included.

---

## Quick Answer: What's Taking Up Space?

Run the analysis script I created:

```bash
./show-compiled-crates.sh
```

**Results:**
- **35 crates** are compiled in
- Binary is **"not stripped"** (includes debug symbols)
- Cranelift optimization passes take **147 KB alone**

---

## Tools & Scripts I Created

### 1. `show-compiled-crates.sh` - Quick Overview
Shows what crates are compiled and basic binary info.

```bash
./show-compiled-crates.sh
```

### 2. `trace-compiled-files.sh` - Detailed Analysis
Creates comprehensive reports in `analysis-output/` directory.

```bash
./trace-compiled-files.sh
```

Generates:
- Dependency trees (with and without features)
- Size breakdown by function (cargo bloat)
- Size breakdown by crate
- List of source files compiled

### 3. Manual Commands

```bash
# What crates are included?
cargo tree --package embive-program --target riscv32imac-unknown-none-elf

# What functions take the most space?
cargo bloat --release --target riscv32imac-unknown-none-elf -p embive-program -n 50

# Why is X included?
cargo tree -p embive-program --target riscv32imac-unknown-none-elf -i CRATE_NAME
```

---

## What's Actually Being Compiled

**35 crates total:**

### Core Runtime (Small)
- `runtime-embive` - Your custom runtime
- `linked_list_allocator` - Heap allocator
- `critical-section` - Concurrency primitive

### Cranelift JIT (LARGE!)
- `cranelift-codegen` ← **BIGGEST** (optimizer, lowering, verifier)
- `cranelift-frontend` - IR builder
- `cranelift-jit` - JIT support
- `cranelift-module` - Module system
- `cranelift-control` - Control flow
- `cranelift-bforest` - Data structure
- `cranelift-entity` - Entity management
- `cranelift-bitset` - Bit sets
- `regalloc2` - Register allocator ← **LARGE**

### Your Code
- `lp-toy-lang` - Toy language parser
- `embive-program` - Main program

### Supporting Libraries
- `hashbrown` - Hash maps (no_std)
- `bumpalo` - Arena allocator
- `nom` - Parser combinator (for lp-toy-lang)
- `anyhow` - Error handling
- `libm` - Math functions (no_std)
- Plus ~15 small utility crates

---

## Top Space Consumers (from cargo bloat)

| Size | Component | Why Included |
|------|-----------|--------------|
| 147 KB | `constructor_simplify` | Cranelift optimizer ← Can we gate this? |
| 43 KB | `constructor_lower` | RISC-V lowering ← Needed |
| 21 KB | `regalloc2::init` | Register allocator ← Needed |
| 19 KB | `Verifier::run` | IR verifier ← Already disabled? |
| 19 KB | `regalloc2::run` | Register allocator ← Needed |
| 15 KB | `fastalloc::run` | Fast register allocator ← Needed |
| 14 KB | `emit_uncompressed` | RISC-V emission ← Needed |
| 13 KB | `VCode::emit` | Code emission ← Needed |
| 13 KB | `run_toy_demo` | Your code ← Needed |
| 12 KB | `compile::compile` | Cranelift compile ← Needed |

**Total in top 10:** ~320 KB  
**Remaining 2089 functions:** ~700 KB  
**Debug info:** ~800 KB - 1.5 MB (in the binary file, can be stripped!)

---

## SOLUTIONS: How to Reduce Size

### IMMEDIATE: Strip Debug Info (Saves ~1 MB)

Edit `apps/embive-program/Cargo.toml`:

```toml
[profile.release]
strip = true
```

**Before:** 3.1 MB → **After:** ~2 MB

### QUICK: Optimize for Size (Saves ~500 KB more)

```toml
[profile.release]
strip = true
opt-level = "z"     # Optimize for size instead of speed
lto = true          # Link-time optimization
codegen-units = 1   # Better dead code elimination
```

**After previous + this:** ~1.5 MB → ~500-800 KB

### MEDIUM: Investigate Feature Gating in Cranelift

The `constructor_simplify` optimization pass (147 KB) is compiled in even though you have:

```rust
flag_builder.set("opt_level", "none").unwrap();
```

This means the optimization *code* is included, just not *used*. 

**Potential fix:** Modify cranelift-codegen to gate optimization passes behind a cargo feature:

```toml
# In cranelift/codegen/Cargo.toml (would need to add):
[features]
default = ["optimize"]
optimize = []  # Gates optimization passes
```

Then in embive-program:
```toml
cranelift-codegen = { 
    default-features = false, 
    features = ["riscv32"]  # Don't include "optimize"
}
```

**Potential savings:** ~150 KB

### ADVANCED: Remove Debug Trait Implementations

Many Debug trait implementations are compiled in even though you're not using them in release mode.

**Potential fix:** Use `#[cfg(debug_assertions)]` or a feature flag to conditionally include Debug impls.

**Potential savings:** ~50-100 KB

---

## Example Workflow

1. **Check current state:**
   ```bash
   ./show-compiled-crates.sh
   ```

2. **Detailed analysis:**
   ```bash
   ./trace-compiled-files.sh
   ls -lh analysis-output/
   ```

3. **Apply quick fixes to Cargo.toml:**
   ```toml
   [profile.release]
   strip = true
   opt-level = "z"
   lto = true
   codegen-units = 1
   panic = "abort"
   ```

4. **Rebuild and check:**
   ```bash
   cargo build --package embive-program --target riscv32imac-unknown-none-elf --release
   ls -lh target/riscv32imac-unknown-none-elf/release/embive-program
   ```

5. **Analyze what changed:**
   ```bash
   cargo bloat --release --target riscv32imac-unknown-none-elf -p embive-program -n 30
   ```

---

## References

- **Detailed analysis:** See `SIZE_ANALYSIS.md`
- **Quick tips:** See `QUICK_SIZE_GUIDE.md`
- **Scripts:**
  - `show-compiled-crates.sh` - Quick overview
  - `trace-compiled-files.sh` - Full analysis
  - `analyze-size.sh` - Alternative analysis tool

---

## Key Findings Summary

✅ **35 crates** are compiled (this is reasonable)  
✅ **Main culprit:** Debug info in binary (~1 MB)  
✅ **Second culprit:** Cranelift optimization code (~150 KB)  
⚠️ **Verifier** seems to still be compiled in (19 KB)  
✅ **No unexpected dependencies** (earlier .rlib files were from other builds)

**Target:** Get from 3.1 MB → **300-500 KB** with proper optimization flags and feature gating.




