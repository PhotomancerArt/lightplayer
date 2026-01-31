# Q32 Metrics App

Tool for tracking q32 transform bloat metrics by generating detailed before/after reports for GLSL
test files.

## Usage

Run with the wrapper script (recommended):

```bash
./scripts/lp-glsl-q32-metrics-app.sh
```

Or run directly:

```bash
cargo run --bin lp-glsl-q32-metrics-app -- \
    --tests-dir <path-to-glsl-files> \
    --output-dir <path-to-reports> \
    --format Fixed16x16
```

## Arguments

- `--tests-dir <path>` - Directory containing GLSL test files (required)
- `--output-dir <path>` - Directory for report output (required)
- `--format <format>` - Fixed point format: `Fixed16x16` or `Q32x32` (default: `Fixed16x16`)
- `-v, --verbose` - Verbose output

## Output

Reports are generated in `reports/yyyy-mm-ddThh.mm.ss/` format:

- `report.toml` - Overall report with metadata and summaries
- `<test-name.glsl>/` - Per-test directory containing:
    - `<test-name.glsl>` - Copy of input GLSL
    - `<function-name>.pre.clif` - Pre-transform CLIF for each function
    - `<function-name>.post.clif` - Post-transform CLIF for each function
    - `stats.toml` - Per-test statistics

## Test Files

Test GLSL files should be placed in the `glsl/` directory. Each file should contain a `main()`
function.
