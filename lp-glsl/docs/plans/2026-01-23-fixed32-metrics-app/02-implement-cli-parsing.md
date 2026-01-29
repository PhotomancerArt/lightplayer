# Phase 2: Implement CLI Argument Parsing

## Description

Implement command-line argument parsing using `clap`. Support required arguments for tests directory and output directory, plus optional format and verbose flags.

## Implementation

- Create `src/main.rs` with basic structure
- Implement `parse_args()` function using `clap`
- Required arguments:
  - `--tests-dir <path>` - Directory containing GLSL test files
  - `--output-dir <path>` - Directory for report output
- Optional arguments:
  - `--format <format>` - Fixed point format (default: `Fixed16x16`)
  - `-v, --verbose` - Verbose output
  - `-h, --help` - Help message
- Parse fixed point format string to `FixedPointFormat` enum

## Success Criteria

- CLI parsing works correctly
- Required arguments are enforced
- Default values are applied for optional arguments
- Help message displays correctly
