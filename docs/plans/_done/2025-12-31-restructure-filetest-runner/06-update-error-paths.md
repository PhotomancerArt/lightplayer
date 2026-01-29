# Phase 6: Update All Error Paths to Use Unified Formatter

## Tasks

1. Replace inline error formatting in `run_detail.rs`:
   - Compilation error path: use `format_compilation_error_detail()`
   - Expected trap but got value: use appropriate wrapper
   - Unexpected trap: use appropriate wrapper
   - Trap code mismatch: use appropriate wrapper
   - Trap message mismatch: use appropriate wrapper
   - Comparison failure: use `format_comparison_error_detail()`

2. Remove old `format_compilation_error` function (replaced by wrapper)

3. Ensure all error paths use the unified formatter

4. Remove duplicate formatting code

5. Update all calls to pass `OutputMode` instead of boolean

6. Verify error output format is consistent across all paths

## Files to Modify

- `src/test_run/run_detail.rs` - Replace all error formatting with unified formatter

## Success Criteria

- All error paths use unified formatter
- No duplicate formatting code
- All code compiles
- No warnings
- Error output format is consistent across all paths
- Error details and rerun commands appear at the end

