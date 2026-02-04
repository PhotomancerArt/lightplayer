# Emulator Logging and Decode-Execute Optimization - Notes

## Context

After implementing the tight instruction loop (12x speed increase), the next optimization targets are:

1. **Logging overhead** - Even with `LogLevel::None`, we're checking log level and reading register values unnecessarily
2. **Decode-execute separation** - Current decode-then-execute pattern adds overhead
3. **Code organization** - Large executor.rs file needs to be split for maintainability

## Reference: embive

The embive project (`/Users/yona/dev/photomancer/oss/embive`) uses:
- Decode-execute fusion (decode directly into execution)
- Macro-based instruction definitions
- Separate files per instruction category
- No logging overhead in hot path

## Key Insights

### Logging Overhead

Current code pattern:
```rust
let rd_old = if log_level != LogLevel::None {
    Some(read_reg(regs, rd))
} else {
    None
};
// ... execute ...
if log_level != LogLevel::None {
    Some(InstLog::Arithmetic { rd_old: rd_old.unwrap(), ... })
} else {
    None
}
```

Problems:
- Runtime check on every instruction
- Reading `rd_old` even when not needed (cache pollution)
- Option allocation overhead
- Pattern matching overhead

Solution: Dual implementations - separate fast and logging versions of each instruction function, with runtime dispatch at top level.

### Decode-Execute Fusion

Current flow:
```
inst_word -> decode_instruction() -> Inst enum -> execute_instruction() -> ExecutionResult
```

Problems:
- Intermediate `Inst` enum allocation
- Two separate pattern matches (decode + execute)
- String formatting in decode error paths

Solution: Combine decode and execute into single function that decodes directly into execution.

### File Organization

Current: Single `executor.rs` file with ~3500 lines

Benefits of splitting:
- Easier to navigate
- Can add floating point instructions in separate file
- Better code locality
- Parallel development

## Design Decisions

### 1. Runtime Logging Control with Zero Overhead

**Decision**: Dual implementation approach
- Two complete implementations: `execute_instruction_fast()` and `execute_instruction_logging()`
- Two run loops: `run_inner_fast()` and `run_inner_logging()`
- Runtime dispatch based on `log_level` at top level
- Fast and logging versions live side-by-side in same files

**Rationale**:
- **Zero overhead when disabled**: Fast path has no log_level checks, no register reads for logging, no InstLog allocations
- **Runtime control**: Full logging support when enabled, with runtime verbosity control (filetests need this)
- **Clear separation**: Fast and logging implementations are next to each other, easy to maintain
- **Compiler optimization**: Each path can be optimized independently by the compiler
- **No abstraction overhead**: Direct function calls, no trait objects or dynamic dispatch

**Trade-offs**:
- Code duplication (two implementations of each instruction)
- Maintenance burden (changes need to be made in both places)
- Larger file sizes

**Mitigation**:
- Keep fast and logging versions side-by-side for easy comparison
- Use clear comments to mark sections
- Consider helper functions for shared logic (but avoid in hot path)

### 2. File Organization

**Decision**: Split by instruction category, with both fast and logging versions in each file

**Rationale**:
- Matches RISC-V instruction grouping
- Natural extension point for floating point
- Easier to find and modify specific instructions
- Fast and logging versions together for easy comparison

### 3. Decode-Execute Fusion Strategy

**Decision**: Implement alongside existing decode/execute, migrate gradually

**Rationale**:
- Maintains backward compatibility
- Allows incremental migration
- Can benchmark each step

### 4. File Organization

**Decision**: Split by instruction category (arithmetic, immediate, load/store, etc.)

**Rationale**:
- Matches RISC-V instruction grouping
- Natural extension point for floating point
- Easier to find and modify specific instructions

## Questions

1. **Should we use dual implementations or const generics for logging?**
   - Answer: Dual implementations - simpler, clearer, better optimization potential. Const generics may not optimize as well.

2. **How to handle runtime log level changes?**
   - Answer: Runtime dispatch at top level (`run_inner()` checks `log_level` once and calls appropriate loop). Fast path has zero overhead, logging path has full runtime control.

3. **Should we keep old decode/execute API?**
   - Answer: Yes, during migration. Remove once all call sites migrated to decode-execute fusion.

4. **How to organize floating point instructions?**
   - Answer: Separate `floating_point.rs` file with fast and logging versions side-by-side, same structure as integer instructions.

5. **Performance measurement strategy?**
   - Answer: Benchmark before/after each phase, measure with `LogLevel::None` (fast path) and `LogLevel::Instructions` (logging path). Target: 15-25% improvement in fast path.

6. **How to minimize code duplication?**
   - Answer: Keep fast and logging versions next to each other for easy comparison. Use helper functions for shared logic, but avoid in hot path. Clear comments mark sections.

7. **Does `step()` and `step_inner()` need fast/logging versions?**
   - Answer: No - they call `execute_instruction()` which dispatches based on `log_level`. The dispatch happens at the `execute_instruction()` level, so `step_inner()` can remain as-is and will automatically use the appropriate path.

8. **How do we handle compressed instructions?**
   - Answer: Compressed instructions also need fast and logging versions. They'll be in `compressed.rs` with the same dual-implementation structure.

9. **How do we ensure the two implementations stay synchronized?**
   - Answer: Keep them side-by-side in the same file for easy comparison. Use clear comments marking corresponding implementations. Consider adding tests that verify both paths produce identical results (except for logging).

10. **What about the migration period - do we keep old API?**
    - Answer: Yes, during migration we'll keep the old `execute_instruction()` API that dispatches. Once decode-execute fusion is complete and all call sites migrated, we can remove the old decode-then-execute pattern.

11. **How do we handle error cases - are they the same?**
    - Answer: Yes, error handling should be identical in both paths. Errors are rare (not in hot path), so we can share error handling logic via helper functions without performance impact.

12. **Does ExecutionResult need changes?**
    - Answer: No - `ExecutionResult` already has `log: Option<InstLog>`. Fast path always sets it to `None`, logging path sets it based on log_level. The struct works for both paths.

## Implementation Order

1. Create macro system for logging
2. Refactor one instruction category (arithmetic) to use macros
3. Measure performance improvement
4. Refactor remaining instructions
5. Implement decode-execute fusion
6. Reorganize files
7. Add new instruction categories as needed
