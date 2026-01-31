# embive-program Size Optimization - Resource Index

## Your Question
> "Compiling cranelift for riscv32 embedded is big! Is there a way to trace the list of rust files that get included into embive-program, to see if we're including anything we shouldn't be?"

## Quick Answer

**Yes!** I've created several tools and analysis reports for you.

**Current binary size:** 3.1 MB (2.3 MB stripped)  
**Target size:** 300-500 KB  
**Main issues:** Debug info (1 MB), Cranelift optimizer code (150 KB)

---

## Start Here

### For Quick Overview
```bash
./show-compiled-crates.sh
```

Shows:
- 35 crates are compiled
- Binary is 3.1 MB with debug info
- Binary is "not stripped"

### For Full Analysis
```bash
./trace-compiled-files.sh
```

Creates `analysis-output/` with detailed reports.

---

## Documentation

| File | Purpose | Read This If... |
|------|---------|----------------|
| **`HOW_TO_TRACE_SIZE.md`** | Main guide | You want to understand how to trace what's compiled |
| **`QUICK_SIZE_GUIDE.md`** | Quick reference | You want commands and immediate fixes |
| **`SIZE_ANALYSIS.md`** | Detailed analysis | You want to see the full breakdown |

---

## Scripts

| Script | What It Does |
|--------|-------------|
| **`show-compiled-crates.sh`** | Quick overview of what's compiled (fast) |
| **`trace-compiled-files.sh`** | Comprehensive analysis (slower, creates reports) |
| **`analyze-size.sh`** | Alternative analysis tool |

---

## Key Findings

### What's Being Compiled (35 crates total)

**Large Dependencies:**
- `cranelift-codegen` - Main compiler (includes optimizer, verifier, lowering)
- `regalloc2` - Register allocator
- `cranelift-frontend`, `cranelift-jit`, `cranelift-module` - JIT support
- `lp-toy-lang` - Your parser (uses `nom`)

**Small Dependencies:**
- `runtime-embive` - Custom runtime
- `hashbrown`, `bumpalo` - Data structures
- Various small utility crates

**Nothing unexpected!** Earlier concerns about `esp_hal` and `lp_glsl_compiler` were false alarms - those were leftover artifacts from other builds.

### Top Space Consumers

1. **Debug info in binary:** ~1 MB (can strip!)
2. **Cranelift optimizer:** 147 KB (even though opt_level = "none")
3. **Register allocator:** ~55 KB combined
4. **RISC-V lowering:** 43 KB
5. **Verifier:** 19 KB
6. **Other codegen:** ~600 KB

---

## Immediate Action Items

### 1. Strip Debug Info (Saves ~1 MB)

Edit `apps/embive-program/Cargo.toml`:

```toml
[profile.release]
strip = true
```

### 2. Optimize for Size (Saves ~500 KB more)

```toml
[profile.release]
strip = true
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
```

### 3. Check Results

```bash
cargo build --package embive-program --target riscv32imac-unknown-none-elf --release
ls -lh target/riscv32imac-unknown-none-elf/release/embive-program
```

**Expected:** 3.1 MB → 500-800 KB

---

## Advanced Optimization

### Feature-Gate Cranelift Components

The optimizer code is compiled in even when unused. Consider:

1. Add features to `cranelift/codegen/Cargo.toml`:
   ```toml
   [features]
   default = ["optimize", "verify"]
   optimize = []  # Gates optimization passes
   verify = []    # Gates verifier
   ```

2. Use in `apps/embive-program/Cargo.toml`:
   ```toml
   cranelift-codegen = { 
       default-features = false, 
       features = ["riscv32"]  # No optimize, no verify
   }
   ```

**Potential savings:** 150-200 KB

---

## Tools Reference

### Cargo Commands
```bash
# Show dependencies
cargo tree -p embive-program --target riscv32imac-unknown-none-elf

# Show size by function
cargo bloat --release --target riscv32imac-unknown-none-elf -p embive-program -n 50

# Show size by crate
cargo bloat --release --target riscv32imac-unknown-none-elf -p embive-program --crates

# Why is X included?
cargo tree -p embive-program --target riscv32imac-unknown-none-elf -i CRATE_NAME

# Show features
cargo tree -p embive-program --target riscv32imac-unknown-none-elf -e features
```

### Required Tools
```bash
# Install cargo-bloat
cargo install cargo-bloat

# Install llvm-tools (for stripping)
rustup component add llvm-tools
```

---

## Expected Results

| Stage | Size | Savings |
|-------|------|---------|
| Current (with debug) | 3.1 MB | - |
| Strip debug info | ~2.0 MB | 1.1 MB |
| Add opt-level="z" + LTO | ~800 KB | 1.2 MB |
| Feature-gate optimizer | ~500 KB | 300 KB |
| **Target** | **300-500 KB** | **~2.5 MB saved** |

---

## Next Steps

1. ✅ Read `HOW_TO_TRACE_SIZE.md`
2. ✅ Run `./show-compiled-crates.sh`
3. ✅ Apply immediate fixes to Cargo.toml
4. ✅ Rebuild and verify size reduction
5. ⏭️ Consider feature-gating cranelift components
6. ⏭️ Share findings with cranelift team?

---

## Support

All analysis scripts and documentation are in the repository root:
- 3 markdown guides
- 3 analysis scripts
- Sample output in `analysis-output/` (after running `trace-compiled-files.sh`)

Run any script with `-h` or read the corresponding .md file for details.




