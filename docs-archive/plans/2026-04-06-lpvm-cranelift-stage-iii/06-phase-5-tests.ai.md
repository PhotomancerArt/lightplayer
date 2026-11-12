# Phase 5: Integration and Validation

## Goal

Wire all new types together, ensure old API still works, validate against
host and embedded targets.

## Integration Checklist

1. **New API wired:**
   - `CraneliftEngine` → `CraneliftModule` → `CraneliftInstance`
   - All trait methods functional

2. **Old API preserved:**
   - `jit()`, `jit_from_ir()` still work
   - `JitModule::call()` still works
   - `DirectCall::call_i32_buf()` still works

3. **Re-exports in lib.rs:**
   ```rust
   // New trait-based API
   pub use engine::CraneliftEngine;
   pub use module::CraneliftModule;
   pub use instance::CraneliftInstance;
   
   // Old API (stays until M7)
   pub use jit_module::JitModule;
   pub use direct_call::DirectCall;
   pub use values::{GlslQ32, CallResult};
   pub use compile::{jit, jit_from_ir, jit_from_ir_owned};
   ```

## Validation Commands

```bash
# Host build
cargo check -p lpvm-cranelift
cargo test -p lpvm-cranelift

# Embedded JIT (the product)
cargo check -p lpvm-cranelift --target riscv32imac-unknown-none-elf

# Firmware still works (uses old API for now)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# Filetests still work (uses old API for now)
cargo check -p lps-filetests
cargo test -p lps-filetests --no-run
```

## Test Coverage

1. **Unit tests in crate:**
   - Simple I32 add function
   - F32 operations (if F32 supported)
   - Q32 operations (if Q32 mode)
   - VMContext fuel get/set
   - Multiple instances from one module
   - DirectCall hot path

2. **Edge cases:**
   - Function not found
   - Wrong number of arguments
   - Memory allocation failure

3. **Cross-validation:**
   - Same LPIR via old API and new trait API produces same result

## What to Watch For

- Double-check that `DirectCall` from new API matches old API behavior
- Ensure VMContext pointer is correct (first arg to functions)
- Verify fuel location in memory matches what compiled code expects
- No `std` leakage in embedded build

## Done When

- All validation commands pass
- Unit tests cover new trait API
- Old API still works (backward compatible)
- Host and RISC-V targets compile
- Ready for M6 (engine migration)
