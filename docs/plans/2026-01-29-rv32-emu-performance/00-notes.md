# RISC-V32 Emulator Performance Enhancements - Notes

## Scope of Work

Optimize the RISC-V32 emulator for performance-critical use cases, particularly `fw-emu` which will run long-running server loops. The emulator needs to execute many instructions efficiently while maintaining the ability to enable debugging features when needed.

Key goals:

1. Make debugging features (instruction logging, etc.) easily disableable with zero overhead when disabled
2. Identify and implement other easy performance improvements
3. Ensure the emulator remains usable for debugging/testing scenarios

## Current State of the Codebase

### Instruction Execution Flow

1. **`execution.rs::step()`** - Main instruction execution entry point
   - Fetches instruction from memory
   - Decodes instruction
   - Increments instruction count
   - Checks for traps (EBREAK)
   - Calls `execute_instruction()` to execute
   - Updates PC
   - Calls `log_instruction()` with InstLog
   - Handles syscalls, panics, traps

2. **`executor.rs::execute_instruction()`** - Executes decoded instruction
   - **ALWAYS creates an `InstLog` struct** for every instruction, even when logging is disabled
   - Reads register values needed for logging (rd_old, rs1_val, rs2_val) even when not needed
   - Calls `inst.encode()` to get instruction word for logging
   - Returns `ExecutionResult` with the log entry

3. **`debug.rs::log_instruction()`** - Logs instruction based on log level
   - Checks `log_level` and only stores log if `Instructions` or `Verbose`
   - But the InstLog struct has already been created!

### Performance Issues Identified

1. **InstLog Creation Overhead** (CRITICAL)
   - Every instruction creates an `InstLog` enum variant, even when `LogLevel::None`
   - This includes reading register values (rd_old, rs1_val, rs2_val) that are only needed for logging
   - Example: For `Add` instruction, we read `rd_old` even though we don't need it for execution
   - The `InstLog` struct is fairly large (contains cycle, pc, instruction, register values, etc.)

2. **Instruction Encoding Overhead**
   - `inst.encode()` is called for every instruction to create the log entry
   - This encoding happens even when logging is disabled

3. **Log Buffer Allocation**
   - `log_buffer: Vec<InstLog>` is always allocated in emulator state
   - Even when logging is disabled, the Vec is still there (though empty)

4. **Cycle Setting**
   - `set_cycle()` is called on every InstLog, even when logging is disabled
   - This involves pattern matching on the InstLog enum

5. **Error Handling**
   - Error handling captures full register state (`regs: [i32; 32]`) in error types
   - This might be expensive but is probably necessary for debugging
   - Could potentially be made optional

6. **Trap Checking**
   - Trap checking happens for every EBREAK instruction
   - Uses binary search which is efficient, probably fine to keep

### Current Logging System

- `LogLevel` enum: `None`, `Errors`, `Instructions`, `Verbose`
- `log_level` field in `Riscv32Emulator` state
- `log_buffer: Vec<InstLog>` stores log entries
- `log_instruction()` checks log level before storing
- But InstLog creation happens BEFORE the check!

## Questions

1. **Should we use conditional compilation (features) or runtime checks for disabling logging?**
   - **Answer**: Use runtime checks (LogLevel enum) for flexibility, but optimize the code so that when LogLevel::None, we skip InstLog creation entirely. This gives us the best of both worlds - flexibility and zero overhead when disabled.

2. **Should we make error handling optional (capturing register state)?**
   - **Answer**: Keep error handling as-is for now. The overhead is only on error paths, not the hot path. We can revisit if profiling shows it's a problem.

3. **Should we optimize the log buffer allocation (use Option<Vec> or similar)?**
   - **Answer**: Keep Vec always allocated. Memory is not a concern, and keeping it simple avoids Option checks.

4. **Should we make instruction encoding lazy (only encode when needed)?**
   - **Answer**: Yes, pass the instruction word from the fetch step instead of encoding from the Inst enum. The instruction word is already available from `fetch_instruction()`.

5. **Are there other performance-critical paths we should optimize?**
   - **Answer**: Focus on logging optimizations first (the biggest win). Other areas (memory bounds checking, register access, decoding, PC updates) are necessary for correctness or already optimized. Can profile later if needed.
