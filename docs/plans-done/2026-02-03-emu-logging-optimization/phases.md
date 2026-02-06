# Implementation Phases

## Phase 1: Create Dual Implementation Structure

1. Create `executor/` directory structure
2. Create `executor/mod.rs` with dispatch functions:
   - `execute_instruction_fast()` - dispatches to fast implementations
   - `execute_instruction_logging()` - dispatches to logging implementations
   - `execute_instruction()` - public API that dispatches based on log_level
3. Create `executor/arithmetic.rs` with fast and logging versions side-by-side
4. Implement one instruction (e.g., ADD) in both fast and logging versions
5. Update `run_loops.rs` to have `run_inner_fast()` and `run_inner_logging()`
6. Test that dispatch works correctly

## Phase 2: Refactor Arithmetic Instructions

1. Implement all arithmetic instructions (ADD, SUB, MUL, etc.) in fast path
2. Implement all arithmetic instructions in logging path
3. Update dispatch in `executor/mod.rs`
4. Measure performance improvement
5. Verify logging still works when enabled
6. Update tests if needed

## Phase 3: Refactor Remaining Instruction Categories

1. Create `immediate.rs` with fast and logging versions
   - Implement ADDI, SLLI, SRLI, etc. in both paths
2. Create `load_store.rs` with fast and logging versions
   - Implement LW, SW, LB, etc. in both paths
3. Create `branch.rs` with fast and logging versions
   - Implement BEQ, BNE, BLT, etc. in both paths
4. Create `jump.rs` with fast and logging versions
   - Implement JAL, JALR in both paths
5. Create `system.rs` with fast and logging versions
   - Implement ECALL, EBREAK, CSR in both paths
6. Create `compressed.rs` with fast and logging versions
   - Implement compressed instructions in both paths
7. Measure cumulative performance improvement

## Phase 4: Decode-Execute Fusion (Hot Path)

1. Create `decode_execute_fast()` that combines decode and execute for fast path
2. Create `decode_execute_logging()` that combines decode and execute for logging path
3. Implement for arithmetic instructions first (most common)
4. Use lookup tables for opcode dispatch
5. Gradually migrate other instruction categories
6. Update `run_inner_fast()` and `run_inner_logging()` to use decode-execute fusion
7. Benchmark performance improvement

## Phase 5: Optimize Run Loops

1. Ensure `run_inner_fast()` has zero logging overhead
2. Ensure `run_inner_logging()` properly handles all log levels
3. Optimize dispatch in `run_inner()` (single check at start)
4. Add `#[inline(always)]` hints where appropriate
5. Benchmark final performance

## Phase 6: Cleanup and Validation

1. Remove old `decode()` + `execute()` pattern if fully migrated
2. Update documentation
3. Run full test suite
4. Benchmark final performance (with and without logging)
5. Validate logging still works correctly at all levels
6. Verify filetests work correctly with both log levels

## Future Phases

- Add floating point instruction support in `floating_point.rs` (with fast + logging versions)
- Add vector extension support
- Further optimizations based on profiling
