# Binary Size Analysis for embive-program (RISC-V32)

## Summary

**Binary Size:** 2.3 MB (3.1 MB file size)
**Text Section:** 1.1 MB (36.2% of total file)

This is very large for an embedded target! This analysis identifies what's being compiled in and opportunities for size reduction.

## Key Findings

### 1. Top Space Consumers (Functions)

The largest functions in the binary are:

| Size | % of .text | Function | Crate |
|------|-----------|----------|-------|
| 147.5 KiB | 12.7% | `constructor_simplify` | cranelift_codegen |
| 43.5 KiB | 3.7% | `constructor_lower` (RISC-V) | cranelift_codegen |
| 20.9 KiB | 1.8% | `regalloc2::Env::init` | cranelift_codegen |
| 19.2 KiB | 1.7% | `Verifier::run` | cranelift_codegen |
| 18.9 KiB | 1.6% | `regalloc2::Env::run` | cranelift_codegen |
| 15.2 KiB | 1.3% | `fastalloc::run` | regalloc2 |
| 14.4 KiB | 1.2% | `emit_uncompressed` (RISC-V) | cranelift_codegen |
| 13.0 KiB | 1.1% | `VCode::emit` | cranelift_codegen |
| 12.8 KiB | 1.1% | `run_toy_demo` | embive_program |
| 12.5 KiB | 1.1% | `compile::compile` | cranelift_codegen |

**Total shown:** ~320 KiB in top 10 functions alone
**Remaining:** 705.9 KiB in 2089 smaller functions

### 2. Dependency Tree

Direct dependencies included:

```
embive-program
├── runtime-embive (custom runtime)
├── lp-toy-lang (toy language frontend)
├── cranelift-codegen (JIT compiler)
│   ├── regalloc2 (register allocator)
│   ├── bumpalo (arena allocator)
│   ├── hashbrown (hash maps)
│   └── wasmtime-internal-math (math operations)
├── cranelift-frontend (IR builder)
├── cranelift-jit (JIT support)
├── cranelift-module (module system)
├── cranelift-control (control flow)
├── hashbrown (hash maps)
└── target-lexicon (target triple)
```

### 3. Unexpected Dependencies Found

Looking at the compiled artifacts, several UNEXPECTED crates were pulled in:

- **`esp_hal`** - ESP32 HAL library (WHY?!)
- **`embassy_sync`** - Embassy async runtime
- **`lp_glsl_compiler`** - GLSL compiler
- **`fugit`** - Time library  
- **`nb`** - Non-blocking trait
- **`generic_array`**

These suggest that features or dependencies are being included that shouldn't be for this embedded target.

## Size Reduction Opportunities

### HIGH IMPACT: Remove Verifier and Debug Code

The verifier alone is 19.2 KiB. In `apps/embive-program/Cargo.toml`:

```toml
cranelift-codegen = { 
    path = "../../cranelift/codegen", 
    default-features = false, 
    features = ["riscv32", "no-std"]  # Remove "verifier" feature if present
}
```

And in the code (`toy_demo.rs`):

```rust
flag_builder.set("enable_verifier", "false").unwrap(); // Already done ✓
```

### HIGH IMPACT: Reduce Optimization Passes

The `constructor_simplify` optimization pass is 147.5 KiB (largest function!). You may be able to gate optimization passes behind features in cranelift-codegen.

Current setting in `toy_demo.rs`:
```rust
flag_builder.set("opt_level", "none").unwrap(); // Already minimal ✓
```

But the optimization code is still compiled in! You'd need to feature-gate the optimization passes themselves in cranelift-codegen.

### HIGH IMPACT: Remove Debugging/Formatting Code

Functions like `print_with_state` (8.3 KiB) and `Debug::fmt` implementations are taking significant space. Consider:

1. Gating `Debug` trait implementations behind a feature
2. Removing instruction printing code for embedded builds
3. Using panic_immediate_abort to remove panic formatting

In `.cargo/config.toml` or as a build flag:
```toml
[profile.release]
panic = "abort"
# Add this to remove panic messages:
# panic = "immediate-abort"  # Requires nightly
```

### MEDIUM IMPACT: Investigate Unexpected Dependencies

**Critical:** Find out why these are included and remove them:

```bash
# Check why esp_hal is included:
cargo tree --package embive-program --target riscv32imac-unknown-none-elf -i esp_hal

# Check why embassy_sync is included:
cargo tree --package embive-program --target riscv32imac-unknown-none-elf -i embassy_sync

# Check why lp_glsl_compiler is included:
cargo tree --package embive-program --target riscv32imac-unknown-none-elf -i lp_glsl_compiler
```

These dependencies alone could be hundreds of KB!

### MEDIUM IMPACT: Strip Binary

```bash
# Using rust-strip or llvm-strip:
rust-strip target/riscv32imac-unknown-none-elf/release/embive-program
# Or:
llvm-strip target/riscv32imac-unknown-none-elf/release/embive-program
```

This removes debug symbols and metadata.

### MEDIUM IMPACT: Use Minimal Register Allocator

The register allocator (regalloc2) contributes significantly:
- `regalloc2::Env::init`: 20.9 KiB
- `regalloc2::Env::run`: 18.9 KiB  
- `fastalloc::run`: 15.2 KiB

Consider whether regalloc2 has feature flags for minimal configurations.

### LOW IMPACT: Link-Time Optimization (LTO)

Enable LTO in `Cargo.toml`:

```toml
[profile.release]
lto = true
codegen-units = 1
```

This can reduce size by 10-30% through better dead code elimination and inlining.

### LOW IMPACT: Abort on Panic

Already should be set for no_std, but verify:

```toml
[profile.release]
panic = "abort"
```

## Detailed Analysis Commands

### Trace All Compiled Files

Use the provided script:
```bash
./trace-compiled-files.sh
```

This will create `analysis-output/` with detailed reports.

### Manual Analysis

```bash
# Show dependency tree:
cargo tree --package embive-program --target riscv32imac-unknown-none-elf

# Show what features are enabled:
cargo tree --package embive-program --target riscv32imac-unknown-none-elf -e features

# Size breakdown (top 50 functions):
cargo bloat --release --target riscv32imac-unknown-none-elf --package embive-program -n 50

# Size by crate:
cargo bloat --release --target riscv32imac-unknown-none-elf --package embive-program --crates

# Rebuild with verbose to see files:
cargo clean -p embive-program
cargo build --package embive-program --target riscv32imac-unknown-none-elf --release -v 2>&1 | grep "Compiling"
```

### Find Specific Dependencies

```bash
# Why is X included?
cargo tree --package embive-program --target riscv32imac-unknown-none-elf -i CRATE_NAME

# What pulls in X?
cargo tree --package embive-program --target riscv32imac-unknown-none-elf -i CRATE_NAME -e normal
```

## Recommended Next Steps

1. **Immediately:** Investigate and remove unexpected dependencies (esp_hal, embassy_sync, lp_glsl_compiler)
2. **High Priority:** Feature-gate cranelift optimization passes and verifier
3. **High Priority:** Remove Debug trait implementations and formatting code for embedded
4. **Medium Priority:** Strip the binary
5. **Medium Priority:** Enable LTO
6. **Low Priority:** Investigate minimal regalloc2 configuration

## Expected Size Reduction

Conservative estimate:
- Remove unexpected deps: -500 KB to -1 MB
- Strip binary: -200 KB to -500 KB
- Feature-gate optimizations: -150 KB
- Remove debug/formatting: -100 KB
- Enable LTO: -200 KB (10-30% of remaining)

**Potential final size: 300-700 KB** (vs current 2.3 MB)

## Tools Used

- `cargo tree`: Dependency analysis
- `cargo bloat`: Binary size breakdown
- `cargo build -v`: Compilation tracing
- File inspection of `.rlib` artifacts




