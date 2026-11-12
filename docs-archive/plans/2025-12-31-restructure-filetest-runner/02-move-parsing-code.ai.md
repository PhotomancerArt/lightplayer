# Phase 2: Move Parsing Code to parse/ Module

## Tasks

1. Move parsing code to `parse/` module:
   - Move `filetest/directives.rs` → split into:
     - `parse/parse_test_type.rs` - Parse test type directives
     - `parse/parse_target.rs` - Parse target directive
     - `parse/parse_run.rs` - Parse run directives
     - `parse/parse_trap.rs` - Parse trap expectations
   - Move `filetest/extraction.rs` → `parse/parse_source.rs`
   - Move `filetest/mod.rs` types → `parse/test_type.rs`

2. Update `parse/mod.rs` to re-export all parsing functions and types

3. Update all imports throughout codebase to use new `parse::` paths

4. Remove old `filetest/` directory

## Files to Create

- `src/parse/parse_test_type.rs`
- `src/parse/parse_target.rs`
- `src/parse/parse_run.rs`
- `src/parse/parse_trap.rs`
- `src/parse/parse_source.rs`
- `src/parse/test_type.rs`

## Files to Modify

- `src/parse/mod.rs` - Add re-exports
- All files that import from `filetest::` - Update to `parse::`

## Files to Remove

- `src/filetest/` directory (after migration)

## Success Criteria

- All parsing code moved to `parse/` module
- All imports updated
- Code compiles
- No warnings
- **Tests added for each parsing function:**
  - `parse_test_type::parse_test_type()` - test all test types
  - `parse_target::parse_target_directive()` - test target parsing
  - `parse_run::parse_run_directive()` - test run directive parsing (exact and approx)
  - `parse_trap::parse_trap_expectation()` - test trap expectation parsing
  - `parse_source::extract_source_and_expectations()` - test source extraction
- **All tests pass before moving to Phase 3**

