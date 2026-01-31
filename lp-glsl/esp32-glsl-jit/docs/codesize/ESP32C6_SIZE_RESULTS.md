# ESP32-C6 GLSL JIT Binary Size Analysis Results

## Current Binary Size

**Binary:** `target/riscv32imac-unknown-none-elf/release/esp32c6-glsl-jit`
- **Total Size:** 1.7 MB (stripped)
- **Previous Size:** 4.0 MB (not stripped)
- **Savings from stripping:** ~2.3 MB

**Status:** ✅ Binary is now stripped (debug symbols removed)

## Size Breakdown by Crate

Based on `cargo bloat --crates` analysis:

| Crate | Size | % of .text | Notes |
|-------|------|------------|-------|
| `cranelift_codegen` | 891.3 KiB | 57.1% | Largest contributor |
| `glsl` | 202.3 KiB | 13.0% | GLSL parser |
| `lp_glsl_compiler` | 165.5 KiB | 10.6% | GLSL compiler |
| `std` | 135.0 KiB | 8.7% | ⚠️ Should be no_std - investigate |
| `regalloc2` | 76.8 KiB | 4.9% | Register allocator |
| `cranelift_frontend` | 43.5 KiB | 2.8% | IR builder |
| `esp_hal` | 10.0 KiB | 0.6% | ESP32 HAL |
| Other crates | ~50 KiB | 3.2% | Various small crates |

**Total .text section:** ~1.5 MB

## Top Functions by Size

Based on `cargo bloat -n 50` analysis:

| Function | Size | Crate | Issue |
|----------|------|-------|-------|
| `constructor_simplify` | 147.5 KiB | cranelift_codegen | ⚠️ Optimizer code (opt_level="none" but code still included) |
| `constructor_lower` | 47.5 KiB | cranelift_codegen | RISC-V lowering (required) |
| `regalloc2::ion::init` | 20.9 KiB | regalloc2 | Register allocator (required) |
| `Verifier::run` | 19.2 KiB | cranelift_codegen | ⚠️ Verifier (enable_verifier="false" but code still included) |
| `regalloc2::ion::run` | 18.9 KiB | regalloc2 | Register allocator (required) |
| `regalloc2::fastalloc::run` | 15.2 KiB | regalloc2 | Register allocator (required) |
| `emit_uncompressed` | 14.4 KiB | cranelift_codegen | RISC-V emission (required) |
| `VCode::emit` | 13.0 KiB | cranelift_codegen | Code emission (required) |
| `compile` | 12.5 KiB | cranelift_codegen | Compilation (required) |

**Top 10 functions:** ~310 KiB

## Issues Identified

### 1. Optimizer Code Included (147.5 KiB)
**Problem:** `constructor_simplify` is compiled in even though `opt_level = "none"` is set.

**Impact:** ~147.5 KiB of unused optimization code

**Solution:** Feature-gate optimization passes in `cranelift/codegen/Cargo.toml`

### 2. Verifier Code Included (19.2 KiB)
**Problem:** `Verifier::run` is compiled in even though `enable_verifier = "false"` is set.

**Impact:** ~19.2 KiB of unused verification code

**Solution:** Feature-gate verifier in `cranelift/codegen/Cargo.toml`

### 3. std Crate Included (135.0 KiB)
**Problem:** `std` crate shows up in size analysis, but this is a no_std target.

**Impact:** ~135.0 KiB (likely from build dependencies or proc-macros)

**Investigation needed:** Check if this is from build-time dependencies or actual runtime code.

### 4. Debug Formatting Code
**Problem:** Various `Debug::fmt` and `print_with_state` functions are included.

**Impact:** ~10-20 KiB

**Solution:** Feature-gate Debug implementations or remove them for embedded targets.

## Optimization Opportunities

### Immediate (Already Applied)
- ✅ **Strip debug symbols:** Saved ~2.3 MB (4.0 MB → 1.7 MB)
- ✅ **Optimize for size (`opt-level = "z`):** Applied
- ✅ **Single codegen unit (`codegen-units = 1`):** Applied

### High Priority
1. **Feature-gate optimizer** (~147.5 KiB savings)
   - Add `optimize` feature to `cranelift/codegen/Cargo.toml`
   - Disable when `opt_level = "none"`

2. **Feature-gate verifier** (~19.2 KiB savings)
   - Add `verify` feature to `cranelift/codegen/Cargo.toml`
   - Disable when `enable_verifier = "false"`

3. **Investigate std inclusion** (~135.0 KiB potential savings)
   - Determine if `std` is from build deps or runtime
   - Remove if possible

### Medium Priority
4. **Feature-gate Debug implementations** (~10-20 KiB savings)
   - Conditionally compile Debug traits for embedded targets

5. **Enable LTO** (~100-300 KiB additional savings)
   - Note: Can't be set in package profile, need workspace-level or `.cargo/config.toml`

## Expected Final Size

### Current: 1.7 MB
### After High Priority Optimizations: ~1.4-1.5 MB
### After All Optimizations: ~1.2-1.3 MB

**Target:** Fit within 4MB flash with room for other code.

## Recommendations

1. **Immediate:** The current 1.7 MB is acceptable for 4MB flash, but can be improved.

2. **Next Steps:**
   - Feature-gate optimizer and verifier in cranelift-codegen
   - Investigate `std` inclusion
   - Consider enabling LTO via `.cargo/config.toml`

3. **Long-term:**
   - Consider creating a minimal cranelift backend with only essential features
   - Review GLSL parser for size optimization opportunities

## Analysis Commands

```bash
# Check binary size
ls -lh target/riscv32imac-unknown-none-elf/release/esp32c6-glsl-jit

# Size by crate
cargo bloat --release --target riscv32imac-unknown-none-elf --package esp32c6-glsl-jit --crates

# Size by function
cargo bloat --release --target riscv32imac-unknown-none-elf --package esp32c6-glsl-jit -n 50

# Dependency tree
cargo tree --package esp32c6-glsl-jit --target riscv32imac-unknown-none-elf
```


