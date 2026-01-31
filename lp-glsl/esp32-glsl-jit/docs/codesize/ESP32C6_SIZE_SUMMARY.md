# ESP32-C6 GLSL JIT Size Analysis Summary

## Current Status

⚠️ **Build Issue:** The build currently fails due to `minimal-lexical` compatibility issues with Rust nightly/edition 2024. This needs to be resolved before we can measure actual binary size.

**Dependency Chain:** `glsl` → `nom` → `minimal-lexical v0.2.1`

---

## Immediate Actions Taken

### ✅ Added Release Profile Optimizations

Added to `apps/esp32c6-glsl-jit/Cargo.toml`:

```toml
[profile.release]
strip = true              # Strip debug symbols
opt-level = "z"          # Optimize for size
lto = true               # Link-time optimization
codegen-units = 1        # Better dead code elimination
panic = "abort"          # Smaller panic handler
```

**Expected Impact:** ~1.5-2.5 MB reduction once build succeeds

---

## Dependency Analysis

### Key Dependencies

1. **Cranelift Components** (~400-600 KB estimated)
   - `cranelift-codegen` (riscv32 feature only) ✅ Minimal
   - `cranelift-frontend` (core feature) ✅ Minimal
   - `cranelift-module` (core feature) ✅ Minimal
   - `cranelift-control` ✅ Minimal

2. **GLSL Compiler** (~50-100 KB estimated)
   - `lp-glsl-compiler` (core feature) ✅ Minimal
   - `glsl` parser (from git) ⚠️ Includes `nom` parser

3. **ESP32 HAL & Runtime** (~100-200 KB estimated)
   - `esp-hal`, `esp-hal-embassy`
   - `embassy-executor`, `embassy-time`
   - `defmt` (debug formatting) ⚠️ Can be optimized

4. **Supporting Libraries** (~50-100 KB estimated)
   - `hashbrown`, `target-lexicon`, `portable-atomic`

### Feature Configuration Status

✅ **Good:** Cranelift dependencies already use minimal features:
- `default-features = false`
- Only `riscv32` backend enabled
- `core` features for no_std support

⚠️ **Could Improve:**
- `defmt` is enabled everywhere - consider feature-gating for release
- ESP32 HAL features could be reviewed for unused ones

---

## Expected Size Breakdown (After Build Fix)

### Before Optimizations
- **With debug info:** ~2.5-3.5 MB
- **Stripped:** ~1.5-2.0 MB

### After Release Profile Optimizations (Already Applied)
- **Estimated:** ~500-800 KB

### Potential Further Optimizations

1. **Feature-gate Cranelift optimizer** (~150-200 KB savings)
   - Code is compiled even when `opt_level = "none"`
   - Requires modifying `cranelift/codegen/Cargo.toml`

2. **Feature-gate verifier** (~15-20 KB savings)
   - Code compiled even when `enable_verifier = "false"`
   - Requires modifying `cranelift/codegen/Cargo.toml`

3. **Minimize defmt usage** (~20-50 KB savings)
   - Consider disabling in release builds
   - Or use feature flags to conditionally compile

4. **Review ESP32 HAL features** (variable savings)
   - Disable unused features
   - Minimize embassy executor features

### Target Size
- **Goal:** ~300-500 KB (fits comfortably in 4MB flash)

---

## Build Issue Resolution

### Problem
`minimal-lexical v0.2.1` (dependency of `nom` → `glsl`) fails to compile with Rust nightly/edition 2024.

### Solutions to Try

1. **Update `nom` dependency** in `glsl` parser
   - Check if newer `nom` version fixes compatibility
   - Or pin compatible `minimal-lexical` version

2. **Use dependency override** in `Cargo.toml`:
   ```toml
   [patch.crates-io]
   minimal-lexical = { version = "0.2.2" }  # or compatible version
   ```

3. **Fork and update** the `glsl` parser dependency
   - Update `nom` to compatible version
   - Or replace `nom` with alternative parser

4. **Check Rust toolchain**
   - Ensure nightly toolchain is up to date
   - Try stable toolchain if nightly issues persist

---

## Analysis Tools Created

### 1. Size Analysis Script
`analyze-esp32c6-size.sh` - Comprehensive analysis script

**Usage:**
```bash
./analyze-esp32c6-size.sh
```

**Outputs:**
- Dependency tree
- Feature tree
- Binary size (when build succeeds)
- Cargo bloat analysis (when build succeeds)
- Release profile check

### 2. Analysis Documentation
- `ESP32C6_SIZE_ANALYSIS.md` - Detailed analysis document
- `ESP32C6_SIZE_SUMMARY.md` - This summary

---

## Next Steps

### Priority 1: Fix Build
1. Resolve `minimal-lexical` compatibility issue
2. Get successful release build
3. Measure actual binary size

### Priority 2: Measure & Analyze
1. Run `./analyze-esp32c6-size.sh`
2. Run `cargo bloat` to identify large functions/crates
3. Compare with expected sizes

### Priority 3: Further Optimizations
1. Implement feature-gating for optimizer/verifier
2. Minimize defmt usage
3. Review ESP32 HAL features
4. Iterate until target size achieved

---

## Commands for Analysis (After Build Succeeds)

```bash
# Check binary size
ls -lh apps/esp32c6-glsl-jit/target/riscv32imac-unknown-none-elf/release/esp32c6-glsl-jit

# Size breakdown by function
cargo bloat --release --target riscv32imac-unknown-none-elf --package esp32c6-glsl-jit -n 50

# Size breakdown by crate
cargo bloat --release --target riscv32imac-unknown-none-elf --package esp32c6-glsl-jit --crates

# Dependency tree
cargo tree --package esp32c6-glsl-jit --target riscv32imac-unknown-none-elf

# Features enabled
cargo tree --package esp32c6-glsl-jit --target riscv32imac-unknown-none-elf -e features
```

---

## References

- `ESP32C6_SIZE_ANALYSIS.md` - Detailed analysis
- `HOW_TO_TRACE_SIZE.md` - General size analysis guide
- `QUICK_SIZE_GUIDE.md` - Quick optimization tips
- `analyze-esp32c6-size.sh` - Analysis script


