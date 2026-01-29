# Phase 9: Integration and Cleanup

## Description

Integrate all components in `main.rs`, implement the main processing loop, and perform final cleanup.

## Implementation

- Complete `main.rs` implementation:
  - Implement `run()` function with main processing loop
  - Iterate GLSL files in tests directory
  - For each file:
    - Read GLSL source
    - Compile to module (before transform)
    - Apply transform (after transform)
    - Collect statistics (before and after)
    - Create test directory in report output
    - Copy GLSL file to test directory
    - Write per-function CLIF files
    - Generate test report (stats.toml)
  - After all tests:
    - Generate overall report (report.toml)
  - Handle errors and abort on failure
- Ensure all modules are properly integrated
- Add proper error messages
- Test end-to-end workflow
- Run `cargo +nightly fmt` on all files
- Fix all warnings
- Update README.md with usage instructions

## Success Criteria

- End-to-end workflow works correctly
- All reports are generated properly
- All CLIF files are written correctly
- Code compiles without errors
- All warnings are fixed
- Code is properly formatted
- README includes usage instructions
