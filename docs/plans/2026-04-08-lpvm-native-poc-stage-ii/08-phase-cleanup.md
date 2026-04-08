# Phase 8: Cleanup and Validation

## Scope

Final cleanup: remove TODOs, fix warnings, ensure formatting, and verify the implementation compiles and passes all tests on both host and RISC-V targets.

## Cleanup Checklist

### 1. Remove Temporary Code

Search for and address:
```bash
grep -r "TODO\|FIXME\|XXX\|unimplemented!" lp-shader/lpvm-native/src/ | grep -v "^.*//.*TODO.*M3"
```

Allowed TODOs (M3 scope):
- Control flow (branches, loops)
- 64-bit operations
- RVC compressed instructions
- JIT buffer output

Remove or convert:
```rust
// Before:
unimplemented!("VInst variant not yet supported: {:?}", vinst);

// After (for M2 scope):
return Err(NativeError::UnsupportedVInst(format!("{:?}", vinst)));
```

### 2. Fix Warnings

```bash
cargo check -p lpvm-native 2>&1 | grep -i warning
```

Common issues to fix:
- Unused imports
- Dead code (mark with `#[allow(dead_code)]` if intentional)
- Unused mut
- Missing docs on public items

### 3. Format

```bash
cargo +nightly fmt -p lpvm-native
```

### 4. No-Std Check

```bash
# Critical: Must pass for ESP32 firmware
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf --features no_std
```

### 5. Test Final Validation

```bash
# All unit tests
cargo test -p lpvm-native --lib

# All tests
cargo test -p lpvm-native

# Clippy strict
cargo clippy -p lpvm-native -- -D warnings
```

## Code Review Checklist

Per-file review:

- [ ] `inst.rs` — Clean encoders, documented bit layouts
- [ ] `emit.rs` — No `println!` debug, proper error handling
- [ ] `greedy.rs` — Clean allocation logic, limit check present
- [ ] `engine.rs` — Proper pipeline, error propagation
- [ ] `module.rs` — Clean struct, no dead code
- [ ] `error.rs` — All error variants have Display impl
- [ ] `lib.rs` — Clean re-exports

## Final Architecture Check

Verify the implemented architecture matches the design:

```
IrFunction
    │
    ▼
lower::lower_ops() → Vec<VInst>
    │
    ▼
regalloc::GreedyAlloc::allocate() → Allocation
    │
    ▼
emit::EmitContext::emit_vinst() + emit_prologue/epilogue
    │
    ▼
finish_elf() → Vec<u8> (ELF)
    │
    ▼
NativeModule { elf, name, code_size }
```

## Summary Document

After cleanup, update `summary.md`:

```markdown
# M2 RV32 Emission Summary

## Completed
- R/I/S/B/U/J instruction encoding (inst.rs)
- Prologue/epilogue emission with fixed frame
- VInst → machine code mapping
- R_RISCV_CALL_PLT relocations via object crate
- ELF object file generation
- Greedy allocator live value limit
- Engine integration pipeline
- Comprehensive tests

## Test Results
- Unit tests: N passed
- Integration tests: N passed
- No-std check: PASS
- Clippy: 0 warnings

## Deferred to M3
- Control flow (branches, loops)
- 64-bit operations
- Spill/reload infrastructure
- Linear scan allocator
- JIT buffer output
```

## Commit Message

```
feat(lpvm-native): M2 RV32 instruction emission and ELF generation

- Implement R/I/S/B/U/J instruction encoding in isa/rv32/inst.rs
- Add EmitContext with prologue/epilogue and frame layout
- Map VInst variants to machine code with register allocation
- Generate R_RISCV_CALL_PLT relocations for builtin calls
- Produce ELF object files using object crate (no_std compatible)
- Add live value limit to greedy allocator (max 24)
- Integrate full compile pipeline in NativeEngine
- Comprehensive unit and integration tests

Refs: docs/roadmaps/2026-04-07-lpvm-native-poc/m2-rv32-emission.md
```

## Validation Commands

Final validation must pass before commit:

```bash
cargo test -p lpvm-native
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
cargo clippy -p lpvm-native -- -D warnings
cargo +nightly fmt -p lpvm-native -- --check
```
