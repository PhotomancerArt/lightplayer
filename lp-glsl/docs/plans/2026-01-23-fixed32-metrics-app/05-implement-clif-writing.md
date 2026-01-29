# Phase 5: Implement CLIF File Writing Utilities

## Description

Implement utilities to write per-function CLIF IR files (main.pre.clif, main.post.clif, etc.) to the report directory.

## Implementation

- Create `src/clif.rs`
- Implement `write_clif_files()` function:
  - Takes test directory path, function name, before/after `Function`, and name mapping
  - Uses `format_function()` from `lp-glsl-compiler` to format CLIF IR
  - Writes `{function_name}.pre.clif` file
  - Writes `{function_name}.post.clif` file
  - Handles errors and aborts on failure
- Ensure proper file path handling and directory creation

## Success Criteria

- Per-function CLIF files are written correctly
- File naming follows the pattern: `{function_name}.pre.clif` and `{function_name}.post.clif`
- CLIF IR is formatted correctly
- Errors are handled appropriately
