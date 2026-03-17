# Phase 4: Multi-Target Runner Dispatch

## Scope

Update the runner to iterate over active targets from `DEFAULT_TARGETS`,
compile and execute each test file against each target, and use the new
`Annotation`/`Disposition` system instead of the old `expect_fail` boolean
and `FiletestTarget` enum. Remove `src/test_run/target.rs`.

This is the largest phase. It touches `run.rs`, `run_summary.rs`,
`run_detail.rs`, `mod.rs`, and `lib.rs`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Update `src/test_run/mod.rs`

Update `TestCaseStats` to replace `expect_fail` terminology:

```rust
#[derive(Debug, Clone, Copy, Default)]
pub struct TestCaseStats {
    pub passed: usize,
    pub failed: usize,
    pub total: usize,
    /// Tests annotated @unimplemented/@broken that failed as expected.
    pub expected_failure: usize,
    /// Tests annotated @unimplemented/@broken that unexpectedly passed.
    pub unexpected_pass: usize,
    /// Tests annotated @ignore that were skipped.
    pub skipped: usize,
}
```

Update `record_failure` to take a `Disposition` instead of checking
`directive.expect_fail`:

```rust
pub fn record_result(
    disposition: Disposition,
    passed: bool,
    stats: &mut TestCaseStats,
    failed_lines: &mut Vec<usize>,
    unexpected_pass_lines: &mut Vec<usize>,
    line_number: usize,
) {
    match (disposition, passed) {
        (Disposition::Skip, _) => {
            stats.skipped += 1;
        }
        (Disposition::ExpectFailure, true) => {
            stats.unexpected_pass += 1;
            unexpected_pass_lines.push(line_number);
        }
        (Disposition::ExpectFailure, false) => {
            stats.expected_failure += 1;
        }
        (Disposition::ExpectSuccess, true) => {
            stats.passed += 1;
        }
        (Disposition::ExpectSuccess, false) => {
            stats.failed += 1;
            failed_lines.push(line_number);
        }
    }
}
```

### Remove `src/test_run/target.rs`

Delete this file. Its `FiletestTarget` enum and `parse_target` function are
replaced by `src/target/`.

Update `src/test_run/mod.rs` to remove `pub mod target;`.

### Create compilation helper

Create a helper function (in `run_summary.rs` or a new `compile.rs`) that
takes a `Target` and compiles GLSL source:

```rust
fn compile_for_target(
    source: &str,
    target: &Target,
    relative_path: &str,
    log_level: LogLevel,
) -> Result<Box<dyn GlslExecutable>, anyhow::Error> {
    match target.backend {
        Backend::Cranelift => {
            let run_mode = RunMode::Emulator {
                max_memory: DEFAULT_MAX_MEMORY,
                stack_size: DEFAULT_STACK_SIZE,
                max_instructions: DEFAULT_MAX_INSTRUCTIONS,
                log_level: Some(log_level),
            };
            let options = GlslOptions {
                run_mode,
                float_mode: match target.float_mode {
                    FloatMode::Q32 => lp_glsl_cranelift::FloatMode::Q32,
                    FloatMode::F32 => lp_glsl_cranelift::FloatMode::F32,
                },
                q32_opts: lp_glsl_cranelift::Q32Options::default(),
                memory_optimized: false,
                target_override: None,
                max_errors: lp_glsl_cranelift::DEFAULT_MAX_ERRORS,
            };
            let exec = glsl_emu_riscv32_with_metadata(source, options, Some(relative_path.to_string()))?;
            Ok(exec)
        }
        Backend::Wasm => {
            let options = WasmOptions {
                float_mode: match target.float_mode {
                    FloatMode::Q32 => lp_glsl_wasm::FloatMode::Q32,
                    FloatMode::F32 => lp_glsl_wasm::FloatMode::F32,
                },
                max_errors: lp_glsl_cranelift::DEFAULT_MAX_ERRORS,
            };
            let exec = wasm_runner::WasmExecutable::from_source(source, options)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(Box::new(exec))
        }
    }
}
```

Note: The exact FloatMode type mapping depends on whether the
DecimalFormat→FloatMode rename (Q3) has been done. If not yet, use the
existing `DecimalFormat` type from `lp_glsl_cranelift` and `lp_glsl_wasm`.
Adapt accordingly.

### Update `src/test_run/run.rs`

The main dispatcher now accepts a list of targets:

```rust
pub fn run_test_file_with_line_filter(
    test_file: &TestFile,
    path: &Path,
    line_filter: Option<usize>,
    output_mode: OutputMode,
    targets: &[&Target],
) -> Result<(Result<()>, TestCaseStats, Vec<usize>, Vec<usize>)>
```

For each target, call `run_summary::run` or `run_detail::run` (based on
output mode), accumulating stats across targets.

If `targets` is empty (no matching targets after filtering), return success
with zero stats.

### Update `src/test_run/run_summary.rs`

Update signature to accept `&Target` instead of reading from `test_file.target`:

```rust
pub fn run(
    test_file: &TestFile,
    path: &Path,
    line_filter: Option<usize>,
    target: &Target,
) -> Result<(Result<()>, TestCaseStats, Vec<usize>, Vec<usize>)>
```

Key changes:
1. Replace `target::parse_target(target_str)` with the passed-in `Target`
2. Replace the `match &filetest_target` compilation block with the
   `compile_for_target` helper
3. Replace `directive.expect_fail` checks with
   `directive_disposition(&test_file.annotations, &directive.annotations, target)`
4. Use `record_result(disposition, ...)` instead of the inline
   `if directive.expect_fail` blocks

### Update `src/test_run/run_detail.rs`

Same pattern as run_summary.rs:
1. Accept `&Target` parameter
2. Use `compile_for_target` helper
3. Use `directive_disposition` + `record_result`

### Update `src/lib.rs`

Update `run_filetest_with_line_filter` to:
1. Determine active targets (for now, use `DEFAULT_TARGETS`; CLI filtering
   comes in phase 5)
2. For `test run`: loop over targets, call the runner for each
3. For `test error`: call once, ignore targets (as decided in Q4)
4. Accumulate stats across targets

Update all display strings: replace "expect-fail" with "expected-failure",
"unexpected-pass" etc. in the summary output.

Update `format_file_counts` and `format_results_summary` to handle the new
`skipped` stat and the renamed `expected_failure` stat.

### Update `src/test_error/mod.rs`

Remove the target dispatch code. Error tests now just compile with the
frontend (which is backend-independent). Remove the `target::parse_target`
call and the match on `FiletestTarget`.

The simplest approach: compile using the cranelift path (since error tests
only exercise the frontend, any backend works). Or better: call
`CompilationPipeline::parse_and_analyze` directly without going through a
backend at all.

### Tests

Existing filetest integration tests should still pass (they'll run against
`DEFAULT_TARGETS` which includes cranelift.q32 — same as before).

Add unit tests in run_summary or a test helper:

- `test_summary_mode_skips_ignored_directive` — directive with
  `@ignore(backend=wasm)` is skipped when running wasm.q32
- `test_summary_mode_xfail_directive` — directive with
  `@unimplemented(backend=wasm)` that fails on wasm.q32 counts as
  expected_failure

## Validate

```
cargo build -p lp-glsl-filetests
cargo test -p lp-glsl-filetests
cargo +nightly fmt -- --check
```

At this point the runner works with the new annotation system. Existing test
files still have `// target riscv32.q32` and `[expect-fail]` — they'll be
parsed with the legacy `// target` line ignored and `[expect-fail]` no longer
recognized. This means: **phase 3 and phase 4 must be done together, or the
test file migration (phase 6) must happen between them**. The recommended
approach is to do phases 3+4 together, keeping the code compiling throughout,
then migrate test files in phase 6.

Alternatively, keep backward compatibility during the transition by having
the parser recognize both old and new formats temporarily.
