# ESP32-C6 Codegen Size Analysis

## Summary

Analysis of what code from `cranelift-codegen` is included in the `esp32c6-glsl-jit` binary.

## ISA Backend Analysis

### ✅ Only RISC-V32 ISA is Included

**Confirmed:**
- `cargo tree` shows only `riscv32` feature enabled for `cranelift-codegen`
- Other ISAs (`x64`, `aarch64`, `riscv64`, `s390x`, `pulley`) are properly gated with `#[cfg(feature = "...")]`
- No ISA-specific code from other architectures is being compiled

**ISA Gating:**
```rust
// cranelift/codegen/src/isa/mod.rs
#[cfg(feature = "x86")]
pub mod x64;

#[cfg(feature = "arm64")]
pub mod aarch64;

#[cfg(feature = "riscv32")]
pub mod riscv32;  // ✅ Only this is enabled

#[cfg(feature = "riscv64")]
pub mod riscv64;

#[cfg(feature = "s390x")]
mod s390x;
```

### Current Configuration

From `apps/esp32c6-glsl-jit/Cargo.toml`:
```toml
cranelift-codegen = { path = "../../cranelift/codegen", default-features = false, features = ["riscv32"] }
```

**Result:** Only RISC-V32 ISA backend code is included. ✅

## Optimizer Code Analysis

### ⚠️ Optimizer Code Still Included Despite Being Disabled

**Current Settings:**
- `opt_level` is set to `"none"` in `main.rs` (line 51)
- `enable_verifier` is set to `"false"` (line 53)

**Problem:**
- `constructor_simplify` (optimizer) takes up **147.5 KiB** (from cargo-bloat analysis)
- `Verifier::run` takes up **19.2 KiB**
- Even though these aren't being *called*, the code is still compiled into the binary

**Why:**
- Rust's dead code elimination doesn't always remove large functions, especially when they're part of a public API or have complex call graphs
- The optimizer code is always compiled as part of `cranelift-codegen`, even if not used

**Potential Solutions:**

1. **Feature-gate the optimizer** (requires upstream changes):
   - Add a `optimizer` feature to `cranelift-codegen`
   - Gate `egraph.rs` and related optimizer code behind this feature
   - Could save ~147.5 KiB

2. **Feature-gate the verifier** (requires upstream changes):
   - Add a `verifier` feature to `cranelift-codegen`
   - Gate verifier code behind this feature
   - Could save ~19.2 KiB

3. **Use LTO more aggressively**:
   - Already enabled in release profile
   - May help with dead code elimination, but limited effectiveness

## Size Breakdown (from cargo-bloat)

From the earlier analysis:
- `cranelift_codegen`: **891.3 KiB** (57.1% of .text)
  - `constructor_simplify`: 147.5 KiB (optimizer, unused)
  - `constructor_lower`: 47.5 KiB (required for codegen)
  - `Verifier::run`: 19.2 KiB (verifier, unused)

## Recommendations

1. **✅ No action needed for ISA backends** - Only RISC-V32 is included, which is correct.

2. **Consider feature-gating optimizer** (if upstream changes are acceptable):
   - Add `optimizer` feature to `cranelift-codegen/Cargo.toml`
   - Gate optimizer code behind `#[cfg(feature = "optimizer")]`
   - Update `lp-glsl-compiler` to not enable optimizer feature
   - **Potential savings: ~147.5 KiB**

3. **Consider feature-gating verifier** (if upstream changes are acceptable):
   - Add `verifier` feature to `cranelift-codegen/Cargo.toml`
   - Gate verifier code behind `#[cfg(feature = "verifier")]`
   - Update `lp-glsl-compiler` to not enable verifier feature
   - **Potential savings: ~19.2 KiB**

4. **Total potential savings: ~166.7 KiB** (if both optimizer and verifier are feature-gated)

## Conclusion

- ✅ **ISA backends are correctly configured** - only RISC-V32 is included
- ⚠️ **Optimizer and verifier code is included but unused** - could be feature-gated to save ~166.7 KiB
- The current binary size (1.7 MB) fits within the 4MB flash constraint, but further optimization is possible


