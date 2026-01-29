# Restructure Filetest Runner

## Problem

The filetest runner has become messy with inconsistent error formatting, mixed concerns, and unclear organization. The codebase has:

1. **Inconsistent error formatting**: Different error paths format errors differently, making it hard to find error details and rerun commands
2. **Mixed concerns**: Summary and detail modes are mixed in the same file, making it hard to understand and maintain
3. **Unclear organization**: Code is spread across files without clear semantic boundaries
4. **Unused code**: Test types like `test compile` and `test transform` exist but aren't used (only `test run` is used)

## Goal

Restructure the filetest runner into a clean, semantic file organization that:

1. **Consistent error formatting**: All error paths in detail mode display sections in a consistent order:
   - Emulator state (DEBUG mode only)
   - V-code (DEBUG mode only)
   - Transformed CLIF (DEBUG mode only)
   - Raw CLIF (DEBUG mode only)
   - Test GLSL (always shown)
   - Error details (filename:<line>, error message)
   - Rerun commands (with and without DEBUG)

2. **Clear separation**: Summary and detail modes are in separate files with common code abstracted out

3. **Semantic organization**: One concept per file, making code easier to test and maintain

4. **Extensibility**: Structure allows for adding other test types in the future while keeping runtest code separate

5. **Better naming**: Replace "bootstrap" terminology with "test glsl", replace `show_full_output` boolean with `OutputMode` enum

## Scope

This is a **total rewrite** of the filetest runner. We will:
- Remove old code and replace with new structure
- Maintain all existing functionality
- Improve error formatting consistency
- Clean up terminology
- Make the codebase more maintainable

## New File Structure

```
src/
├── lib.rs                          # Main entry point, test discovery, orchestration
├── colors.rs                       # ANSI color codes and colorization utilities
├── discovery.rs                    # Test file discovery (walking directories, filtering)
├── runner.rs                       # Main test runner orchestration (parallel/sequential)
│   └── concurrent.rs              # Concurrent test execution
├── output_mode.rs                  # OutputMode enum (Summary, Detail, Debug)
│
├── parse/                          # Test file parsing
│   ├── mod.rs                      # Re-exports only
│   ├── parse_test_type.rs          # Parse "// test run", "// test compile", etc.
│   ├── parse_target.rs             # Parse "// target riscv32.q32" directive
│   ├── parse_run.rs                # Parse "// run:" directives
│   ├── parse_trap.rs               # Parse trap expectations
│   ├── parse_source.rs             # Extract GLSL source and CLIF expectations
│   └── test_type.rs                # TestType enum and related types
│
├── test_run/                       # Run test implementation
│   ├── mod.rs                      # Re-exports only
│   ├── run.rs                      # Main delegator (chooses summary vs detail)
│   ├── run_summary.rs              # Summary mode (compile once, reuse emulator)
│   ├── run_detail.rs               # Detail mode (compile per test case, includes error formatting)
│   ├── test_glsl.rs                # Generating isolated test GLSL code (includes function filtering)
│   ├── execution.rs                # Executing functions in emulator
│   ├── target.rs                   # Target value parsing (riscv32.q32 -> RunMode/DecimalFormat)
│   └── parse_assert.rs             # Parse assertions, compare values
│
├── test_compile/                   # Compile test implementation (unused but kept for future)
│   ├── mod.rs                      # Re-exports only
│   └── compile.rs                  # Compile test logic
│
├── test_transform/                 # Transform test implementation (unused but kept for future)
│   ├── mod.rs                      # Re-exports only
│   └── transform.rs                # Transform test logic
│
└── util/                           # Shared utilities
    ├── mod.rs                      # Re-exports only
    ├── file_update.rs               # BLESS mode file updating
    ├── validation.rs                # CLIF validation utilities
    └── path.rs                     # Path utilities (relative_path, etc.)
```

## Acceptance Criteria

1. All existing tests pass
2. Error formatting is consistent across all error paths
3. Error details and rerun commands appear at the end of output
4. Code compiles without warnings
5. File structure matches the proposed structure above
6. All `mod.rs` files contain only re-exports
7. Terminology is consistent ("test glsl" instead of "bootstrap", `OutputMode` instead of `show_full_output`)
8. Filetests run correctly using the scripts:
   - `scripts/glsl-filetests.sh matrix/mat4` (summary mode - multiple tests)
   - `scripts/glsl-filetests.sh matrix/mat4/op-add.glsl` (detail mode - single test)
   - Both should show appropriate output format (errors at end with rerun commands)

