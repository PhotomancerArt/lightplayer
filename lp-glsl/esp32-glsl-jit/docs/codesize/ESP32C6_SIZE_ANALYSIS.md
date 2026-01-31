# ESP32-C6 GLSL JIT Binary Size Analysis

## Overview

This document analyzes the binary size of `esp32c6-glsl-jit` for the `riscv32imac-unknown-none-elf` target to identify optimization opportunities.

**Target Constraint:** 4MB total flash, with much needed by other code.

---

## Current State

### Build Status
⚠️ **Build currently failing** due to `minimal-lexical` compatibility issues with Rust nightly/edition 2024. This needs to be resolved before we can measure actual binary size.

### Dependencies Analysis

Based on `cargo tree` analysis, here are the key dependencies:

#### Core Cranelift Components
- `cranelift-codegen` (with `riscv32` feature only)
- `cranelift-frontend` (with `core` feature)
- `cranelift-module` (with `core` feature)
- `cranelift-control`

#### GLSL Compiler
- `lp-glsl-compiler` (with `core` feature)
- `glsl` parser (from git, branch `feature/spans`)
  - Pulls in `nom` parser combinator
  - Pulls in `nom_locate`
  - **Issue:** `nom` → `minimal-lexical` has compatibility issues

#### ESP32 HAL & Runtime
- `esp-hal` (ESP32-C6 HAL)
- `esp-hal-embassy` (async runtime)
- `embassy-executor` (async executor)
- `embassy-time` (time driver)
- `esp-alloc` (heap allocator)
- `defmt` (debug formatting - **can be optimized**)
- `panic-rtt-target` (panic handler)
- `rtt-target` (RTT logging)

#### Supporting Libraries
- `hashbrown` (hash maps, no_std)
- `target-lexicon`
- `portable-atomic`
- `critical-section`

---

## Expected Size Contributors (Based on Similar Analysis)

Based on analysis of `embive-program` (similar embedded target), here's what to expect:

### 1. Debug Info (LARGEST - ~1-1.5 MB)
**Status:** Likely included if `strip = true` not set  
**Action:** Add `strip = true` to `[profile.release]`  
**Expected Savings:** ~1-1.5 MB

### 2. Cranelift Optimizer Code (~150-200 KB)
**Status:** Code compiled in even when `opt_level = "none"`  
**Issue:** Optimization passes are always compiled, just not used  
**Action:** Feature-gate optimization passes in `cranelift-codegen`  
**Expected Savings:** ~150-200 KB

### 3. Register Allocator (~50-60 KB)
**Status:** Required for code generation  
**Action:** None (required)  
**Expected Size:** ~50-60 KB

### 4. RISC-V Lowering (~40-50 KB)
**Status:** Required for RISC-V backend  
**Action:** None (required)  
**Expected Size:** ~40-50 KB

### 5. Verifier (~15-20 KB)
**Status:** May be compiled even when disabled  
**Action:** Feature-gate verifier code  
**Expected Savings:** ~15-20 KB

### 6. Debug/Formatting Code (~20-50 KB)
**Status:** `defmt` and Debug trait implementations  
**Action:** Consider disabling `defmt` in release or feature-gating Debug impls  
**Expected Savings:** ~20-50 KB

### 7. ESP32 HAL & Runtime (~100-200 KB)
**Status:** Required for ESP32-C6 operation  
**Action:** Minimize features, but likely needed  
**Expected Size:** ~100-200 KB

### 8. GLSL Parser (~50-100 KB)
**Status:** Required for GLSL compilation  
**Action:** None (required)  
**Expected Size:** ~50-100 KB

---

## Optimization Recommendations

### Immediate Actions (High Impact)

#### 1. Add Release Profile Optimization

Add to `apps/esp32c6-glsl-jit/Cargo.toml`:

```toml
[profile.release]
strip = true              # Strip debug symbols (saves ~1-1.5 MB)
opt-level = "z"          # Optimize for size (saves ~200-500 KB)
lto = true               # Link-time optimization (saves ~100-300 KB)
codegen-units = 1        # Better dead code elimination (saves ~50-100 KB)
panic = "abort"          # Smaller panic handler (saves ~10-20 KB)
```

**Expected Total Savings:** ~1.5-2.5 MB

#### 2. Minimize Defmt Usage

Consider disabling `defmt` in release builds or using a feature flag:

```toml
[features]
default = ["defmt"]
```

Then conditionally compile defmt:

```rust
#[cfg(feature = "defmt")]
use defmt::info;
```

**Expected Savings:** ~20-50 KB

### Medium-Term Actions

#### 3. Feature-Gate Cranelift Optimizer

Modify `cranelift/codegen/Cargo.toml` to gate optimization passes:

```toml
[features]
default = ["optimize"]
optimize = []  # Gates optimization pass code
```

Then in `apps/esp32c6-glsl-jit/Cargo.toml`:

```toml
cranelift-codegen = { 
    path = "../../cranelift/codegen", 
    default-features = false, 
    features = ["riscv32"]  # Don't include "optimize"
}
```

**Expected Savings:** ~150-200 KB

#### 4. Feature-Gate Verifier

Similar approach for verifier code:

```toml
[features]
default = ["verify"]
verify = []  # Gates verifier code
```

**Expected Savings:** ~15-20 KB

### Advanced Actions

#### 5. Minimize ESP32 HAL Features

Review `esp-hal` and `esp-hal-embassy` features - disable unused ones:

```toml
esp-hal = { version = "=1.0.0-rc.0", default-features = false, features = ["esp32c6"] }
esp-hal-embassy = { version = "0.9.0", default-features = false, features = ["esp32c6"] }
```

**Expected Savings:** Variable, depends on unused features

#### 6. Review GLSL Parser Features

Check if `glsl` parser can be configured with fewer features:

```toml
glsl = { 
    git = "https://github.com/Yona-Appletree/glsl-parser.git", 
    branch = "feature/spans", 
    default-features = false 
}
```

**Expected Savings:** Variable

---

## Expected Final Size

### Before Optimization
- **Estimated:** 2.5-3.5 MB (with debug info)
- **Stripped:** ~1.5-2.0 MB

### After Immediate Optimizations
- **Estimated:** ~500-800 KB

### After All Optimizations
- **Target:** ~300-500 KB

---

## Build Issue Resolution

### Current Problem
`minimal-lexical` (dependency of `nom` → `glsl`) has compilation errors with Rust nightly/edition 2024.

### Potential Solutions

1. **Update `nom` dependency** in `glsl` parser to a version compatible with edition 2024
2. **Pin `minimal-lexical` version** in `Cargo.lock` or use a compatible version
3. **Use a different GLSL parser** that doesn't depend on `nom`
4. **Fork and fix** the `glsl` parser dependency

---

## Analysis Tools

### Run Size Analysis Script
```bash
./analyze-esp32c6-size.sh
```

### Manual Commands

```bash
# Check dependencies
cargo tree --package esp32c6-glsl-jit --target riscv32imac-unknown-none-elf

# Check features
cargo tree --package esp32c6-glsl-jit --target riscv32imac-unknown-none-elf -e features

# Check binary size (after build succeeds)
ls -lh apps/esp32c6-glsl-jit/target/riscv32imac-unknown-none-elf/release/esp32c6-glsl-jit

# Size breakdown by function (requires cargo-bloat)
cargo bloat --release --target riscv32imac-unknown-none-elf --package esp32c6-glsl-jit -n 50

# Size breakdown by crate
cargo bloat --release --target riscv32imac-unknown-none-elf --package esp32c6-glsl-jit --crates
```

---

## Next Steps

1. **Fix build issue** - Resolve `minimal-lexical` compatibility
2. **Build binary** - Get actual size measurements
3. **Apply immediate optimizations** - Add release profile settings
4. **Measure impact** - Compare before/after sizes
5. **Implement feature gating** - Gate optimizer/verifier code
6. **Iterate** - Continue optimizing until target size is met

---

## References

- `HOW_TO_TRACE_SIZE.md` - General size analysis guide
- `QUICK_SIZE_GUIDE.md` - Quick optimization tips
- `SIZE_ANALYSIS.md` - Detailed analysis methodology
- `analyze-esp32c6-size.sh` - Custom analysis script for this target


