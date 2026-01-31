# RISC-V32 Emulator Performance Enhancements - Design

## Scope of Work

Optimize the RISC-V32 emulator for performance-critical use cases by making debugging features (
especially instruction logging) have zero overhead when disabled. The main optimization is to avoid
creating `InstLog` structs when logging is disabled.

## File Structure

```
lp-riscv/lp-riscv-tools/src/emu/
├── executor.rs                    # UPDATE: Make InstLog creation conditional
├── emulator/
│   ├── execution.rs               # UPDATE: Pass instruction_word and log_level
│   ├── debug.rs                    # UPDATE: Handle Option<InstLog>
│   └── state.rs                    # No changes needed
└── logging.rs                      # No changes needed
```

## Conceptual Architecture

### Current Flow (with overhead)

```
step()
  ├─ fetch_instruction() → instruction_word
  ├─ decode_instruction() → Inst
  ├─ execute_instruction(Inst, pc, regs, memory)
  │   ├─ inst.encode() → instruction_word (REDUNDANT!)
  │   ├─ Read registers for logging (rd_old, rs1_val, etc.)
  │   └─ Create InstLog (ALWAYS, even if logging disabled)
  │   └─ Return ExecutionResult { log: InstLog }
  ├─ exec_result.log.set_cycle() (ALWAYS)
  └─ log_instruction() → checks log_level, discards if None
```

### Optimized Flow (zero overhead when disabled)

```
step()
  ├─ fetch_instruction() → instruction_word
  ├─ decode_instruction() → Inst
  ├─ execute_instruction(Inst, instruction_word, pc, regs, memory, log_level)
  │   ├─ If log_level == None: skip all logging work
  │   ├─ If log_level != None: create InstLog with instruction_word
  │   └─ Return ExecutionResult { log: Option<InstLog> }
  ├─ If log.is_some(): set_cycle() and log_instruction()
  └─ Otherwise: skip logging entirely
```

## Main Components and Interactions

### 1. ExecutionResult Changes

- Change `log: InstLog` to `log: Option<InstLog>`
- When `log_level == LogLevel::None`, `log` will be `None`
- When logging is enabled, `log` will be `Some(InstLog)`

### 2. execute_instruction() Signature Changes

- Add `instruction_word: u32` parameter (from fetch_instruction)
- Add `log_level: LogLevel` parameter
- Only create `InstLog` when `log_level != LogLevel::None`
- Only read register values needed for logging when logging is enabled
- Use `instruction_word` directly instead of calling `inst.encode()`

### 3. step() Changes

- Pass `instruction_word` and `self.log_level` to `execute_instruction()`
- Only call `set_cycle()` and `log_instruction()` if `exec_result.log.is_some()`

### 4. log_instruction() Changes

- Accept `Option<InstLog>` instead of `InstLog`
- If `None`, return early
- Otherwise, proceed with existing logic

## Performance Improvements

1. **Zero InstLog Creation Overhead**: When `LogLevel::None`, no InstLog structs are created
2. **No Register Reads for Logging**: Register values like `rd_old`, `rs1_val`, `rs2_val` are only
   read when needed for logging
3. **No Instruction Encoding**: Use the already-fetched `instruction_word` instead of encoding from
   `Inst` enum
4. **No Cycle Setting**: `set_cycle()` is only called when logging is enabled
5. **Early Returns**: `log_instruction()` can return early if log is `None`

## Backward Compatibility

- Public API remains the same (users still use `with_log_level()`)
- Debugging features still work exactly as before when enabled
- Only the internal implementation changes to be more efficient
