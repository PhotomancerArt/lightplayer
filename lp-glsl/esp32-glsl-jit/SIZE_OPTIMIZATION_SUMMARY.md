# ESP32 Binary Size Optimization Summary

## Problem
Binary overflowed ROM by 214444 bytes (~209 KB). Current binary size: 4.1 MB

## Optimizations Applied

### 1. ✅ Added LTO (Link-Time Optimization)
**Location:** `Cargo.toml` (workspace root)
**Change:** Added `lto = true` to `[profile.release.package.esp32-glsl-jit]`
**Expected Savings:** ~100-300 KB through better dead code elimination

### 2. ✅ Added Panic Abort
**Location:** `Cargo.toml` (workspace root)
**Change:** Added `panic = "abort"` to reduce panic handler size
**Expected Savings:** ~10-20 KB

### 3. ✅ Added Linker Flags
**Location:** `apps/esp32-glsl-jit/.cargo/config.toml` (new file)
**Flags Added:**
- `-Wl,--gc-sections` - Remove unused sections
- `-Wl,--strip-unneeded` - Remove unused symbols
**Expected Savings:** ~50-100 KB

### 4. ✅ Existing Optimizations (Already Applied)
- `strip = true` - Strip debug symbols (~1-1.5 MB savings)
- `opt-level = "z"` - Optimize for size (~200-500 KB savings)
- `codegen-units = 1` - Single codegen unit (~50-100 KB savings)

## Total Expected Savings
- **From new optimizations:** ~160-420 KB
- **From existing optimizations:** ~1.3-2.1 MB
- **Total potential reduction:** ~1.5-2.5 MB

**Expected final size:** ~1.6-2.6 MB (down from 4.1 MB)

## Next Steps if Still Too Large

### High Impact (Requires Code Changes)

#### 1. Feature-Gate Cranelift Optimizer Code (~150-200 KB savings)
The `constructor_simplify` function (~147 KB) is compiled even when `opt_level = "none"` because the egraph code is always compiled.

**Solution:** Add a feature flag to `cranelift/codegen/Cargo.toml`:
```toml
[features]
default = ["riscv32", "optimizer"]
optimizer = []  # Gates egraph optimization code
```

Then conditionally compile egraph code:
```rust
#[cfg(feature = "optimizer")]
mod egraph;
```

**Status:** Requires modifying cranelift-codegen

#### 2. Feature-Gate Verifier Code (~15-20 KB savings)
Verifier code is compiled even when `enable_verifier = "false"`.

**Solution:** Similar feature-gating approach in cranelift-codegen

#### 3. Minimize Defmt Usage (~20-50 KB savings)
Defmt is enabled everywhere. Consider:
- Feature-gating defmt for release builds
- Using conditional compilation: `#[cfg(feature = "defmt")]`

**Status:** Requires modifying Cargo.toml and source code

### Medium Impact

#### 4. Review ESP32 HAL Features
Check if all ESP32 HAL features are needed:
- `esp-hal` features: `["defmt", "esp32c6", "unstable"]`
- `esp-hal-embassy` features: `["defmt", "esp32c6"]`
- Consider removing `unstable` if not needed

#### 5. Review Embassy Executor Features
- `task-arena-size-20480` - Consider if smaller arena size is sufficient

### Low Impact

#### 6. Additional Linker Flags (if supported)
```toml
rustflags = [
    "-C", "link-arg=-Wl,--gc-sections",
    "-C", "link-arg=-Wl,--strip-unneeded",
    "-C", "link-arg=-Wl,--as-needed",  # Only link needed libraries
    "-C", "link-arg=-Wl,--no-keep-memory",  # Reduce memory usage during linking
]
```

## Analysis Tools

### Quick Analysis
```bash
cd apps/esp32-glsl-jit
./analyze-binary-size.sh
```

### Manual Commands
```bash
# Size by crate
cargo bloat --release --target riscv32imac-unknown-none-elf --package esp32-glsl-jit --crates

# Size by function
cargo bloat --release --target riscv32imac-unknown-none-elf --package esp32-glsl-jit -n 50

# Dependency tree
cargo tree --package esp32-glsl-jit --target riscv32imac-unknown-none-elf

# Features enabled
cargo tree --package esp32-glsl-jit --target riscv32imac-unknown-none-elf -e features
```

## Current Configuration

### Profile Settings (`Cargo.toml` workspace root)
```toml
[profile.release.package.esp32-glsl-jit]
strip = true
opt-level = "z"
codegen-units = 1
lto = true
panic = "abort"
```

### Linker Flags (`apps/esp32-glsl-jit/.cargo/config.toml`)
```toml
[target.riscv32imac-unknown-none-elf]
rustflags = [
    "-C", "link-arg=-Wl,--gc-sections",
    "-C", "link-arg=-Wl,--strip-unneeded",
]
```

## Testing

After applying optimizations:
1. Clean build: `cargo clean -p esp32-glsl-jit`
2. Rebuild: `cargo build --package esp32-glsl-jit --target riscv32imac-unknown-none-elf --release`
3. Check size: `ls -lh target/riscv32imac-unknown-none-elf/release/esp32-glsl-jit`
4. Analyze: `./analyze-binary-size.sh`

## References
- Previous analysis: `docs/codesize/ESP32C6_SIZE_ANALYSIS.md`
- Size analysis guide: `docs/codesize/HOW_TO_TRACE_SIZE.md`
- Quick guide: `docs/codesize/QUICK_SIZE_GUIDE.md`







