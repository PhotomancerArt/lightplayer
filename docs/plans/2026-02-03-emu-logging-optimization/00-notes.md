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

Solution: Compile-time conditional compilation using macros or const generics.

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

### 1. Compile-Time vs Runtime Logging Control

**Decision**: Hybrid approach
- Compile-time feature flag: `logging` feature controls whether logging code is compiled
- Runtime log level: When logging IS compiled, use runtime `LogLevel` to control verbosity

**Rationale**:
- Zero overhead when disabled (compile-time removal)
- Flexible control when enabled (runtime selection)
- Best of both worlds

### 2. Macro vs Const Generics

**Decision**: Start with macros, consider const generics if needed

**Rationale**:
- Macros are simpler to implement
- Can migrate to const generics later if needed
- Easier to understand and maintain

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

1. **Should we use feature flags or const generics for logging?**
   - Answer: Start with feature flags (simpler), can upgrade to const generics if needed

2. **How to handle runtime log level changes?**
   - Answer: When logging feature is enabled, runtime LogLevel controls verbosity. When disabled, no logging possible.

3. **Should we keep old decode/execute API?**
   - Answer: Yes, during migration. Remove once all call sites migrated.

4. **How to organize floating point instructions?**
   - Answer: Separate `floating_point.rs` file, similar structure to integer instructions.

5. **Performance measurement strategy?**
   - Answer: Benchmark before/after each phase, measure with logging disabled and enabled.

## Implementation Order

1. Create macro system for logging
2. Refactor one instruction category (arithmetic) to use macros
3. Measure performance improvement
4. Refactor remaining instructions
5. Implement decode-execute fusion
6. Reorganize files
7. Add new instruction categories as needed
