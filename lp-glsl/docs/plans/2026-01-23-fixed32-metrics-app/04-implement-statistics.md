# Phase 4: Implement Statistics Collection

## Description

Implement statistics collection for functions and modules. Collect blocks, instructions, values, and CLIF text size metrics.

## Implementation

- Create `src/stats.rs`
- Define `FunctionStats` struct with: name, blocks, instructions, values, clif_size
- Define `ModuleStats` struct with: total_blocks, total_instructions, total_values, total_clif_size, functions (Vec<FunctionStats>)
- Implement `collect_function_stats()`:
  - Takes `Function` and function name
  - Collects blocks count (from `layout.blocks()`)
  - Collects instructions count (sum of `block_insts()` for each block)
  - Collects values count (from `dfg.num_values()`)
  - Collects CLIF text size (from `format_function()` result length)
  - Returns `FunctionStats`
- Implement `collect_module_stats()`:
  - Takes `GlModule` and name mapping
  - Iterates all functions
  - Collects per-function stats
  - Calculates totals
  - Returns `ModuleStats`
- Define `StatsDelta` struct with absolute and percentage deltas
- Implement `calculate_deltas()` to compute before/after differences

## Success Criteria

- Statistics are collected accurately for functions
- Module-level totals are calculated correctly
- Deltas are computed correctly (absolute and percentage)
- Code compiles without errors
