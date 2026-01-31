# Restructure Filetest Error Formatting

## Problem

In filetests detail mode, error formatting is inconsistent across different error paths. The error details and rerun commands should always appear at the end of the output, so when you tail the command output, you see the most important information (error details and how to rerun).

Currently, different error paths format errors differently:

1. Compilation errors (`format_compilation_error`)
2. Expected trap but got value (inline formatting)
3. Unexpected trap (inline formatting)
4. Trap code mismatch (inline formatting)
5. Trap message mismatch (inline formatting)
6. Comparison failure (inline formatting)

Each path has different ordering of sections, making it hard to find the error details and rerun commands.

## Goal

All error paths in detail mode should display sections in a consistent order:

1. Emulator state (DEBUG mode only)
2. V-code (DEBUG mode only)
3. Transformed CLIF (DEBUG mode only)
4. Raw CLIF (DEBUG mode only)
5. GLSL (always shown)
6. Error details (filename:<line>, error message)
7. Rerun commands (with and without DEBUG)

The goal is that when you tail the command output, you'll see the error details and rerun commands at the end.

## Questions to Answer

1. ✅ **Should we create a single unified error formatting function that all error paths use?**

   - **Answer:** Yes, one unified function

2. ✅ **How should we handle the DEBUG mode check? Should it be passed as a parameter or checked via environment variable?**

   - **Answer:** Replace `show_full_output` boolean with an enum: `OutputMode { Summary, Detail, Debug }`
   - Pass this enum as a parameter to the unified error formatter
   - This is clearer than a boolean and explicitly represents the three modes

3. ✅ **Should the rerun commands include both `DEBUG=1` and non-DEBUG versions?**

   - **Answer:** Yes, show both commands with labels:

     ```
     Rerun just this test:
       scripts/glsl-filetests.sh matrix/mat4/op-add.glsl:42

     Rerun with debugging:
       DEBUG=1 scripts/glsl-filetests.sh matrix/mat4/op-add.glsl:42
     ```

4. ✅ **Should we extract the debug info formatting (emulator state, v-code, CLIF) into separate functions for better organization?**

   - **Answer:** Keep one function (`format_debug_info`) to ensure consistent section headers. Reorder sections to match desired order.

5. ✅ **How should we handle cases where the executable is not available (e.g., compilation errors)? Should we skip debug sections or show what we can?**

   - **Answer:** Pass `None` for the executable parameter. Consider higher-level wrapper functions (e.g., `format_compilation_error_detail`, `format_execution_error_detail`) that handle specific error cases and delegate to the main formatter, to avoid having to pass lots of `None` values.

6. ✅ **Should the GLSL section always show the bootstrap code, or should it show the original file context when available?**
   - **Answer:** Always show what was actually compiled (the isolated test code). This matches line numbers and only includes functions used in the test for clarity.
   - **Note:** Rename "bootstrap" terminology → "compiled glsl". The code no longer has a main() wrapper, so "bootstrap" is misleading.

## Plan Phases

### Phase 1: Replace `show_full_output` boolean with `OutputMode` enum

**Tasks:**

- Define `OutputMode` enum in `test_run/mod.rs`:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum OutputMode {
      Summary,  // Minimal output, used for multiple tests
      Detail,   // Full output for single test (current show_full_output=true)
      Debug,    // Full output + debug sections (when DEBUG=1)
  }
  ```
- Replace all `show_full_output: bool` parameters with `output_mode: OutputMode`
- Update logic that checks `show_full_output`:
  - `if !show_full_output` → `if output_mode == OutputMode::Summary`
  - `if show_full_output` → `if output_mode != OutputMode::Summary`
- Update call sites:
  - `lib.rs`: `let show_full_output = test_specs.len() == 1;` → determine `OutputMode` based on test count and DEBUG env var
  - `concurrent.rs`: Update `Request` struct and all usages
  - `test_run/mod.rs`: Update function signatures and all internal usages
- Check for DEBUG environment variable to determine if mode should be `Debug` vs `Detail`

**Files to modify:**

- `lightplayer/crates/lp-glsl-filetests/src/test_run/mod.rs`
- `lightplayer/crates/lp-glsl-filetests/src/lib.rs`
- `lightplayer/crates/lp-glsl-filetests/src/concurrent.rs`

**Success criteria:**

- All code compiles
- No warnings
- Tests still pass (behavior should be unchanged at this point)

---

### Phase 2: Rename "bootstrap" terminology → "compiled glsl"

**Tasks:**

- Rename `BootstrapResult` struct → `CompiledGlslResult` (or similar)
- Rename `bootstrap` module → `compiled_glsl` (or keep module name but update struct)
- Update all references:
  - `bootstrap::generate_bootstrap()` → `compiled_glsl::generate_compiled_glsl()` (or keep function name)
  - `bootstrap::BootstrapResult` → `compiled_glsl::CompiledGlslResult`
  - Variable names: `bootstrap_result` → `compiled_glsl_result`
  - Comments mentioning "bootstrap"
  - String literals: `"=== Bootstrapped GLSL Test ==="` → `"=== Compiled GLSL ==="` or similar
- Update README.md if it mentions bootstrap

**Files to modify:**

- `lightplayer/crates/lp-glsl-filetests/src/test_run/bootstrap.rs` (possibly rename file)
- `lightplayer/crates/lp-glsl-filetests/src/test_run/mod.rs`
- `lightplayer/crates/lp-glsl-filetests/README.md` (if applicable)

**Success criteria:**

- All code compiles
- No warnings
- Terminology is consistent throughout
- Tests still pass

---

### Phase 3: Refactor `format_debug_info`: reorder sections

**Tasks:**

- Update `format_debug_info` function to output sections in this order:
  1. Emulator state (if available)
  2. V-code (if available)
  3. Transformed CLIF (if available)
  4. Raw CLIF (if available)
- Current order is: CLIF (before/after), VCode, disassembly, emulator state
- New order should be: emulator state, VCode, transformed CLIF, raw CLIF
- Keep the function signature the same for now (will be updated in Phase 4)
- Ensure section headers are consistent

**Files to modify:**

- `lightplayer/crates/lp-glsl-filetests/src/test_run/mod.rs`

**Success criteria:**

- All code compiles
- No warnings
- Debug output shows sections in correct order
- Tests still pass

---

### Phase 4: Create unified error formatter

**Tasks:**

- Create new function `format_detail_mode_error` with signature:
  ```rust
  fn format_detail_mode_error(
      error_type: ErrorType,  // enum for different error types
      error_message: &str,
      filename: &str,
      line_number: usize,
      compiled_glsl: Option<&str>,  // The compiled GLSL code
      executable: Option<&dyn lp_glsl_compiler::GlslExecutable>,  // For debug sections
      output_mode: OutputMode,
  ) -> String
  ```
- Define `ErrorType` enum for different error categories:
  ```rust
  enum ErrorType {
      Compilation,
      ExecutionTrap,
      ComparisonFailure,
      TrapMismatch,
      // etc.
  }
  ```
- Implement section ordering:
  1. Emulator state (if `output_mode == OutputMode::Debug` and executable available)
  2. V-code (if `output_mode == OutputMode::Debug` and executable available)
  3. Transformed CLIF (if `output_mode == OutputMode::Debug` and executable available)
  4. Raw CLIF (if `output_mode == OutputMode::Debug` and executable available)
  5. Compiled GLSL (always, if available)
  6. Error details (`filename:line`, error message)
  7. Rerun commands (both with and without DEBUG)
- Use `format_debug_info` for debug sections
- Format rerun commands as:

  ```
  Rerun just this test:
    scripts/glsl-filetests.sh filename:line

  Rerun with debugging:
    DEBUG=1 scripts/glsl-filetests.sh filename:line
  ```

**Files to modify:**

- `lightplayer/crates/lp-glsl-filetests/src/test_run/mod.rs`

**Success criteria:**

- Function compiles and produces correctly formatted output
- All sections appear in correct order
- Rerun commands are formatted correctly
- Code compiles without warnings

---

### Phase 5: Create wrapper functions

**Tasks:**

- Create convenience wrapper functions that call `format_detail_mode_error`:
  - `format_compilation_error_detail()` - for compilation errors (no executable)
  - `format_execution_error_detail()` - for execution errors (has executable)
  - `format_comparison_error_detail()` - for comparison failures (has executable)
  - `format_trap_error_detail()` - for trap-related errors (has executable)
- Each wrapper should:
  - Extract relevant data from context
  - Call `format_detail_mode_error` with appropriate parameters
  - Return `anyhow::Error` for consistency with existing code
- Wrappers should handle `None` values gracefully (e.g., pass `None` for executable when not available)

**Files to modify:**

- `lightplayer/crates/lp-glsl-filetests/src/test_run/mod.rs`

**Success criteria:**

- All wrapper functions compile
- Wrapper functions produce same output format as unified formatter
- No warnings
- Code is clean and readable

---

### Phase 6: Update all error paths

**Tasks:**

- Replace inline error formatting in `run_test_file_detail_mode`:
  - Compilation error path (line ~379): use `format_compilation_error_detail()`
  - Expected trap but got value (line ~414): use appropriate wrapper
  - Unexpected trap (line ~448): use appropriate wrapper
  - Trap code mismatch (line ~489): use appropriate wrapper
  - Trap message mismatch (line ~524): use appropriate wrapper
  - Comparison failure (line ~599): use `format_comparison_error_detail()`
- Update `format_compilation_error` to use new wrapper (or replace entirely)
- Ensure all error paths use the unified formatter
- Remove duplicate formatting code
- Update all calls to pass `OutputMode` instead of boolean

**Files to modify:**

- `lightplayer/crates/lp-glsl-filetests/src/test_run/mod.rs`

**Success criteria:**

- All error paths use unified formatter
- No duplicate formatting code
- All code compiles
- No warnings
- Error output format is consistent across all paths

---

### Phase 7: Test and cleanup

**Tasks:**

- Run filetests to verify output format:
  - Test compilation errors show correct format
  - Test execution errors show correct format
  - Test comparison failures show correct format
  - Test trap errors show correct format
  - Verify sections appear in correct order
  - Verify error details and rerun commands appear at the end
  - Test with `DEBUG=1` to verify debug sections appear
  - Test without `DEBUG=1` to verify debug sections are hidden
- Fix any issues found
- Remove any temporary code, TODOs, or debug prints
- Fix all warnings
- Ensure all tests pass
- Verify that tailing the output shows error details and rerun commands at the end

**Success criteria:**

- All tests pass
- Output format is consistent across all error types
- Sections appear in correct order
- Error details and rerun commands appear at the end
- No warnings
- Code is clean and readable
- Move plan file to `lightplayer/plans/_done/`
- Run `cargo +nightly fmt` on `lightplayer/` directory
