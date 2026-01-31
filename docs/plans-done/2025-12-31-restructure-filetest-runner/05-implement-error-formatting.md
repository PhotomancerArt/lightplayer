# Phase 5: Implement Unified Error Formatting in run_detail.rs

## Tasks

1. Refactor `format_debug_info` function to output sections in correct order:
   - Emulator state (if available)
   - V-code (if available)
   - Transformed CLIF (if available)
   - Raw CLIF (if available)

2. Create unified error formatting function in `run_detail.rs`:
   ```rust
   fn format_detail_mode_error(
       error_type: ErrorType,
       error_message: &str,
       filename: &str,
       line_number: usize,
       test_glsl: Option<&str>,
       executable: Option<&dyn lp_glsl_compiler::GlslExecutable>,
       output_mode: OutputMode,
   ) -> String
   ```

3. Define `ErrorType` enum for different error categories:
   ```rust
   enum ErrorType {
       Compilation,
       ExecutionTrap,
       ComparisonFailure,
       TrapMismatch,
       UnexpectedTrap,
       ExpectedTrapGotValue,
   }
   ```

4. Implement section ordering:
   - Emulator state (if `output_mode == OutputMode::Debug` and executable available)
   - V-code (if `output_mode == OutputMode::Debug` and executable available)
   - Transformed CLIF (if `output_mode == OutputMode::Debug` and executable available)
   - Raw CLIF (if `output_mode == OutputMode::Debug` and executable available)
   - Test GLSL (always, if available)
   - Error details (`filename:line`, error message)
   - Rerun commands (both with and without DEBUG)

5. Format rerun commands as:
   ```
   Rerun just this test:
     scripts/glsl-filetests.sh filename:line
   
   Rerun with debugging:
     DEBUG=1 scripts/glsl-filetests.sh filename:line
   ```

6. Create wrapper functions:
   - `format_compilation_error_detail()` - for compilation errors
   - `format_execution_error_detail()` - for execution errors
   - `format_comparison_error_detail()` - for comparison failures
   - `format_trap_error_detail()` - for trap-related errors

## Files to Modify

- `src/test_run/run_detail.rs` - Add error formatting functions
- `src/test_run/run_detail.rs` - Refactor `format_debug_info` (or create new function)

## Success Criteria

- Unified error formatter implemented
- All sections appear in correct order
- Rerun commands formatted correctly
- Wrapper functions created
- Code compiles
- No warnings

