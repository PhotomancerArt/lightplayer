# Phase 4: Update Statistics with VCode and Assembly Sizes

## Description

Add `vcode_size` and `assembly_size` fields to statistics structs, update collection functions to accept and use these sizes, and add delta calculations for the new fields.

## Implementation

- Update `FunctionStats` in `src/stats.rs`:
  - Add `pub vcode_size: usize`
  - Add `pub assembly_size: usize`
- Update `ModuleStats` in `src/stats.rs`:
  - Add `pub total_vcode_size: usize`
  - Add `pub total_assembly_size: usize`
- Update `StatsDelta` in `src/stats.rs`:
  - Add `pub vcode_size: i32`
  - Add `pub assembly_size: i32`
  - Add `pub vcode_size_percent: f64`
  - Add `pub assembly_size_percent: f64`
- Update `collect_function_stats()`:
  - Add `vcode_size: usize` and `assembly_size: usize` parameters
  - Include these in returned `FunctionStats`
- Update `collect_module_stats()`:
  - Add `vcode_assembly_sizes: &HashMap<String, (usize, usize)>` parameter
  - Look up sizes for each function
  - Sum total vcode and assembly sizes
  - Pass sizes to `collect_function_stats()`
- Update `calculate_deltas()`:
  - Calculate vcode_size delta and percentage
  - Calculate assembly_size delta and percentage
  - Include in returned `StatsDelta`
- Update `calculate_function_delta()`:
  - Calculate vcode_size delta and percentage
  - Calculate assembly_size delta and percentage
  - Include in returned `StatsDelta`
- Update `collect_function_reports()`:
  - Pass vcode/assembly sizes when collecting function stats
- Update `process_test()` in `main.rs`:
  - Pass vcode_assembly_sizes to `collect_module_stats()` calls

## Success Criteria

- All statistics structs include vcode_size and assembly_size fields
- Statistics collection functions accept and use vcode/assembly sizes
- Delta calculations include vcode and assembly deltas with percentages
- Code compiles without errors
- No warnings (except unused code that will be used in later phases)

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented
