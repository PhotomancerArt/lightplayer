# Design: Q32 Metrics App

## Overview

Create a new app `q32-metrics` in `lp-glsl/apps/` that tracks the effects of q32 transform optimizations by generating detailed before/after reports for GLSL test files. The app compiles GLSL files, applies the q32 transform, collects statistics, and generates TOML reports with per-function CLIF IR files.

## File Structure

```
lp-glsl/apps/q32-metrics/
├── Cargo.toml                 # NEW: App dependencies
├── src/
│   ├── main.rs                # NEW: Entry point, CLI parsing, orchestration
│   ├── compiler.rs            # NEW: GLSL compilation and transform logic
│   ├── stats.rs               # NEW: Statistics collection
│   ├── report.rs              # NEW: Report generation (TOML)
│   └── clif.rs                # NEW: CLIF file writing utilities
├── glsl/                      # NEW: Test GLSL files directory
│   ├── test-add.glsl          # NEW: Basic addition test
│   ├── test-sub.glsl          # NEW: Basic subtraction test
│   └── ...                    # More test files
└── README.md                  # NEW: App documentation

scripts/
└── q32-metrics.sh         # NEW: Wrapper script with defaults
```

## Report Structure

```
reports/yyyy-mm-ddThh.mm.ss/
├── report.toml                # Overall report with metadata and summaries
└── test-add.glsl/             # Per-test directory (filename with extension)
    ├── test-add.glsl          # Copy of input GLSL
    ├── main.pre.clif          # Pre-transform CLIF for main function
    ├── main.post.clif         # Post-transform CLIF for main function
    ├── hash.pre.clif          # Pre-transform CLIF for hash function (if exists)
    ├── hash.post.clif         # Post-transform CLIF for hash function (if exists)
    └── stats.toml             # Per-test statistics
```

## Code Structure

### New Types

**FunctionStats:**
```rust
pub struct FunctionStats {
    pub name: String,
    pub blocks: usize,
    pub instructions: usize,
    pub values: usize,
    pub clif_size: usize,  // CLIF text size in bytes
}
```

**ModuleStats:**
```rust
pub struct ModuleStats {
    pub total_blocks: usize,
    pub total_instructions: usize,
    pub total_values: usize,
    pub total_clif_size: usize,
    pub functions: Vec<FunctionStats>,
}
```

**StatsDelta:**
```rust
pub struct StatsDelta {
    pub blocks: i32,
    pub instructions: i32,
    pub values: i32,
    pub clif_size: i32,
    pub blocks_percent: f64,
    pub instructions_percent: f64,
    pub values_percent: f64,
    pub clif_size_percent: f64,
}
```

**ReportMetadata:**
```rust
pub struct ReportMetadata {
    pub git_hash: String,  // "unknown" if not available
    pub timestamp: String, // ISO 8601 format
    pub test_count: usize,
}
```

**OverallReport:**
```rust
pub struct OverallReport {
    pub metadata: ReportMetadata,
    pub summary: ModuleStats,  // Totals across all tests
    pub tests: Vec<TestSummary>,  // Per-test summaries
}
```

**TestSummary:**
```rust
pub struct TestSummary {
    pub name: String,
    pub before: ModuleStats,
    pub after: ModuleStats,
    pub delta: StatsDelta,
}
```

**TestReport:**
```rust
pub struct TestReport {
    pub name: String,
    pub before: ModuleStats,
    pub after: ModuleStats,
    pub delta: StatsDelta,
    pub functions: Vec<FunctionReport>,  // Per-function breakdown
}
```

**FunctionReport:**
```rust
pub struct FunctionReport {
    pub name: String,
    pub before: FunctionStats,
    pub after: FunctionStats,
    pub delta: StatsDelta,
}
```

### Main Functions

**main.rs:**
- `main()` - Entry point, parse CLI args, orchestrate processing
- `parse_args()` - Parse command-line arguments (tests-dir, output-dir, format, verbose)
- `run()` - Main processing loop: iterate GLSL files, process each, generate reports

**compiler.rs:**
- `compile_glsl()` - Compile GLSL source to GlModule (before transform)
- `apply_transform()` - Apply q32 transform to GlModule
- Uses `GlslCompiler` and `Q32Transform` from `lp-glsl-compiler`

**stats.rs:**
- `collect_function_stats()` - Collect stats from a single Function
- `collect_module_stats()` - Collect stats from GlModule (all functions)
- `calculate_deltas()` - Calculate before/after deltas (absolute and percentage)

**report.rs:**
- `generate_report()` - Generate overall `report.toml`
- `generate_test_report()` - Generate per-test `stats.toml`
- `collect_git_hash()` - Get git hash via `git rev-parse HEAD` command
- Uses `toml` crate for serialization

**clif.rs:**
- `write_clif_files()` - Write per-function CLIF files (main.pre.clif, main.post.clif, etc.)
- Uses `format_function()` from `lp-glsl-compiler` to format CLIF IR

## Command-Line Interface

Required arguments:
- `--tests-dir <path>` - Directory containing GLSL test files
- `--output-dir <path>` - Directory for report output

Optional arguments:
- `--format <format>` - Fixed point format (default: `Fixed16x16`)
- `-v, --verbose` - Verbose output
- `-h, --help` - Help message

Wrapper script (`scripts/q32-metrics.sh`):
- Defaults `--tests-dir` to `lp-glsl/apps/q32-metrics/glsl`
- Defaults `--output-dir` to `docs/reports/q32`
- Defaults `--format` to `Fixed16x16`

## Error Handling

On any failure (compilation, transform, file I/O), abort immediately and exit with error details. Failures are unexpected, so fail fast.

## Dependencies

- `lp-glsl-compiler` with `std` feature
- `serde` and `toml` for TOML serialization
- `clap` for CLI argument parsing
- Standard library (no `no_std` needed)
