# Planning Notes: Q32 Metrics App

## Overview

Create a new app `q32-metrics` in `lp-glsl/apps/` that tracks the effects of q32 transform optimizations by generating detailed before/after reports for GLSL test files.

## Questions

### Q1: App Structure and Dependencies

**Context:**
The app needs to:
- Compile GLSL files to CLIF IR (before transform)
- Apply q32 transform
- Generate CLIF IR (after transform)
- Collect statistics (blocks, instructions, values, CLIF text size)
- Generate reports in markdown and JSON formats

**Question:**
Should this be a standalone binary app (like `esp32-glsl-jit`) or a library with a binary? Given it's a metrics/tooling app, should it be a simple binary that processes files?

**Suggested Answer:**
Standalone binary app similar to other apps in the directory. Simple structure:
- `src/main.rs` - main entry point
- `src/` - implementation modules as needed
- `tests/` directory with sample GLSL files
- `Cargo.toml` with dependencies on `lp-glsl-compiler`

**Answer:** ✅ Confirmed - standalone binary app structure

### Q2: Test GLSL Files Location

**Context:**
The app needs a directory of GLSL test files. The user mentioned starting with GLSL from `tests.rs` and basic math operator functions.

**Question:**
Where should the test GLSL files live? Options:
- `tests/` directory in the app (like `lp-glsl/apps/q32-metrics/tests/`)
- Separate directory like `lp-glsl/apps/q32-metrics/test-shaders/`
- Configurable via command-line argument

**Suggested Answer:**
Use `tests/` directory in the app root (`lp-glsl/apps/q32-metrics/tests/`). Allow override via command-line argument `--tests-dir` or `-t`. Start with GLSL files extracted from `tests.rs` and add basic math operator test cases.

**Answer:** Use `lp-glsl/apps/q32-metrics/glsl/` directory. Allow override via `--tests-dir` or `-t` command-line argument. Start with GLSL files extracted from `tests.rs` and add basic math operator test cases.

### Q3: Report Directory Structure

**Context:**
Reports should be generated in `reports/yyyy-mm-ddThh.mm.ss/` format. Each report contains:
- `report.toml` with overall stats
- Per-test directories with GLSL, pre-transform CLIF, post-transform CLIF, stats TOML

**Question:**
What should the per-test directory names be? Options:
- Use the GLSL filename (without extension)
- Use sanitized version of the filename
- Use a test ID/index

**Suggested Answer:**
Use the GLSL filename without extension, sanitized for filesystem safety (replace invalid chars with `_`). For example, `test-add.glsl` → `test-add/` directory.

**Answer:** Use GLSL filename WITH extension for directory names (e.g., `test-add.glsl/` directory). Sanitize for filesystem safety if needed. Use TOML files instead of MD/JSON - `report.toml` for overall stats, `<test-name.glsl>/stats.toml` for per-test stats.

### Q4: Statistics to Collect

**Context:**
From `tests.rs`, we collect:
- Number of blocks
- Number of instructions
- Number of values
- CLIF text size (bytes)

**Question:**
Should we collect additional statistics? Options:
- Per-function breakdown (if multiple functions in GLSL)
- Memory usage estimates
- Compilation time
- Transform time

**Suggested Answer:**
Start with the same stats as `tests.rs`:
- Blocks, instructions, values, CLIF text size (per function and total)
- Per-function breakdown is useful for comparison
- Skip timing/memory for now (can add later if needed)

**Answer:** ✅ Per-function breakdown confirmed. Separate CLIF files per function: `main.pre.clif`, `main.post.clif`, etc. instead of single module-level CLIF files. This provides better clarity when there are multiple functions.

### Q5: Report Format

**Context:**
Reports need both human-readable (MD) and machine-readable (JSON) formats.

**Question:**
What should the report structure look like? Should it include:
- Comparison with previous runs?
- Summary statistics (totals, averages)?
- Per-test detailed breakdowns?

**Suggested Answer:**
Report structure:
- **Overall report** (`report.md`/`report.json`):
  - Metadata: git hash, timestamp, number of tests
  - Summary: total blocks/instructions/values/size (before/after)
  - Per-test summary table with key metrics
- **Per-test reports** (`<test-name>/stats.md`/`stats.json`):
  - Test name, input GLSL
  - Before/after stats (blocks, instructions, values, size)
  - Deltas (absolute and percentage)
  - Per-function breakdown if multiple functions

**Answer:** ✅ Use TOML files instead of MD/JSON. Overall report: `report.toml`. Per-test reports: `<test-name.glsl>/stats.toml`. TOML is both human-readable and machine-parsable. Structure confirmed: metadata, summary stats, per-test summaries, per-function breakdowns.

### Q6: Git Hash Collection

**Context:**
Reports should include git hash for reproducibility.

**Question:**
How should we collect git hash? Options:
- Run `git rev-parse HEAD` command
- Use `git2` crate
- Make it optional if not in git repo

**Suggested Answer:**
Use `std::process::Command` to run `git rev-parse HEAD`. If it fails (not in git repo or git not available), include "unknown" or omit the field. Keep it simple.

**Answer:** ✅ Confirmed - use `std::process::Command` to run `git rev-parse HEAD`. If it fails, include "unknown" or omit the field.

### Q7: CLIF File Format

**Context:**
Need to save pre-transform and post-transform CLIF IR to files.

**Question:**
Should we save:
- Full module CLIF (all functions together)?
- Per-function CLIF files?
- Both?

**Suggested Answer:**
Save full module CLIF for each test:
- `pre-transform.clif` - before transform
- `post-transform.clif` - after transform
This matches the pattern in `tests.rs` and makes comparison easier.

**Answer:** Save per-function CLIF files: `main.pre.clif`, `main.post.clif`, `hash.pre.clif`, `hash.post.clif`, etc. This provides better clarity and makes it easier to compare individual functions.

### Q8: Error Handling

**Context:**
Some GLSL files might fail to compile or transform.

**Question:**
How should we handle errors? Options:
- Skip failed tests and continue
- Include error information in report
- Fail the entire run

**Suggested Answer:**
Continue processing other tests, but include error information in the report:
- In per-test directory: `error.txt` with error message
- In overall report: list of failed tests with error summaries
- Exit with non-zero code if any tests failed (but still generate report)

**Answer:** On failure, abort immediately and exit with error details. Failures are unexpected, so we should fail fast rather than continuing.

### Q9: Command-Line Interface

**Context:**
The app needs to be runnable and configurable.

**Question:**
What command-line arguments should we support? Options:
- `--tests-dir` - override tests directory
- `--output-dir` - override reports output directory
- `--format` - fixed point format (Fixed16x16, etc.)
- `--verbose` - more detailed output

**Suggested Answer:**
Start simple:
- `--tests-dir <path>` (default: `./tests`)
- `--output-dir <path>` (default: `./reports`)
- `--format <format>` (default: `Fixed16x16`)
- `-v, --verbose` - verbose output
- `-h, --help` - help message

**Answer:** Directories should be required options (no defaults). Create a wrapper script in `scripts/` that runs the app with good defaults:
- `--tests-dir` defaults to `lp-glsl/apps/q32-metrics/glsl`
- `--output-dir` defaults to `docs/reports/q32`
- `--format` defaults to `Fixed16x16`

### Q10: Dependencies

**Context:**
The app needs to use `lp-glsl-compiler` and related crates.

**Question:**
What features should we enable? The app needs:
- Compilation to CLIF IR
- Q32 transform
- CLIF formatting utilities
- Standard library features (file I/O, JSON serialization)

**Suggested Answer:**
Dependencies:
- `lp-glsl-compiler` with `std` feature (for file I/O)
- `serde` and `serde_json` for JSON serialization
- `clap` or similar for CLI argument parsing
- Standard library (no `no_std` needed for this tooling app)

**Answer:** ✅ Confirmed. Dependencies:
- `lp-glsl-compiler` with `std` feature
- `serde` and `toml` for TOML serialization (instead of JSON)
- `clap` for CLI argument parsing
- Standard library (no `no_std` needed)
