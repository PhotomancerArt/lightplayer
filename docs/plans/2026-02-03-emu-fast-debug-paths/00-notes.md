# Emulator Fast/Debug Paths Optimization - Notes

## Scope of Work

Refactor the emulator core to have:
1. **Fast path**: Combined decode/execute with zero logging overhead
2. **Debug path**: Full logging support with runtime control
3. **Monomorphic approach**: Use generics with const trait to eliminate logging overhead at compile time
4. **Code organization**: Split instructions into category files, one instruction per inline function
5. **Decode-execute fusion**: Combine decode and execute steps for better performance
6. **Simplified log levels**: Remove `LogLevel::Verbose`, keep only `None`, `Errors`, `Instructions`

**Expected Impact**: 
- 15-25% improvement from removing logging overhead
- 10-15% improvement from decode-execute fusion
- Better code maintainability through organization

## Current State

### Logging System
- `LogLevel` enum: `None`, `Errors`, `Instructions`, `Verbose`
- `LogLevel::Verbose` only used in one place (`debug.rs`) and treated same as `Instructions`
- Every instruction checks `log_level != LogLevel::None` and reads `rd_old` even when disabled
- `InstLog` structs created even when logging disabled
- ~3581 lines in `executor.rs` with ~120 instruction variants

### Execution Flow
- `run_inner()` → `step_inner()` → `execute_instruction()`
- `execute_instruction()` takes `log_level` parameter and checks it for every instruction
- Decode and execute are separate: `decode_instruction()` → `Inst` enum → `execute_instruction()`

### File Structure
```
lp-riscv/lp-riscv-emu/src/emu/
├── executor.rs              # ~3581 lines, all instructions
├── decoder.rs               # Re-exports lp_riscv_inst::decode_instruction
├── emulator/
│   ├── execution.rs         # step_inner() calls execute_instruction()
│   ├── run_loops.rs         # run_inner() calls step_inner()
│   └── state.rs             # Riscv32Emulator with log_level field
└── logging.rs                # LogLevel enum, InstLog enum
```

## Questions

1. **Should we implement decode-execute fusion in this plan, or defer it?**
   - **Answer**: Yes, include decode-execute fusion in this plan. We're already refactoring execution paths, so doing both together makes sense.

2. **How should we organize category files?**
   - **Answer**: Create `executor/` directory with category files. Each category has a single generic function (e.g., `decode_execute_rtype<M>()`) that's generic over logging mode, not separate fast/logging versions. Categories:
     - `arithmetic.rs` - R-type arithmetic (ADD, SUB, MUL, etc.)
     - `immediate.rs` - I-type immediate (ADDI, SLLI, etc.)
     - `load_store.rs` - Load/store (LW, SW, LB, etc.)
     - `branch.rs` - Branch instructions (BEQ, BNE, etc.)
     - `jump.rs` - Jump instructions (JAL, JALR)
     - `system.rs` - System instructions (ECALL, EBREAK, CSR)
     - `compressed.rs` - Compressed instructions (C.ADD, C.LW, etc.)
     - `bitmanip.rs` - Bit manipulation (Zbb, Zbs, Zba extensions)
     - `mod.rs` - Main dispatch function `decode_execute<M>()`

3. **Should each instruction be a separate inline function, or grouped by category?**
   - **Answer**: Each instruction gets its own `#[inline]` function (e.g., `execute_add<M>()`), generic over logging mode. Category functions (e.g., `decode_execute_rtype<M>()`) decode the instruction word, extract fields, and call the appropriate instruction function.

4. **How should we handle the generic `log_level` parameter in fast path?**
   - **Answer**: `log_level` is checked once in `run_inner()` to decide which path to take. Instruction functions don't take `log_level` parameter - they only check `M::ENABLED`. If `M::ENABLED` is true, create log; if false, don't. Logging is binary at the instruction level - no verbosity checks inside instructions.

5. **Should we keep the `Inst` enum for now and add decode-execute fusion later, or do both at once?**
   - **Answer**: Do both at once. Since we're doing decode-execute fusion, we'll eliminate the `Inst` enum in the hot path. We can keep it for backward compatibility or single-step debugging if needed.

6. **How should we handle compressed instructions?**
   - **Answer**: Include compressed instructions in the refactor. Create `compressed.rs` category file with `decode_execute_compressed<M>()` function. Decode the 16-bit instruction and route to appropriate instruction execution functions, reusing the same instruction execution functions where possible (e.g., `execute_add<M>()`).

7. **What about error handling?**
   - **Answer**: Keep error handling as-is. We're only optimizing the hot path, and error handling overhead is on error paths, not the hot path.

## Notes

- User confirmed: log_level check happens once in `run_inner()`, not per instruction
- User wants: simplified log levels (None, Errors, Instructions only)
- User wants: decode-execute improvement considered at the same time
- User wants: instructions in category files with one inst per inline function
- User wants: logging is binary at instruction level - no `log_level` parameter passed to instructions, just check `M::ENABLED`
