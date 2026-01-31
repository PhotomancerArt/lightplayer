# Phase 6: Verify Code Size Reduction

## Goal

Run the lp-glsl-q32-metrics-app script to compare code sizes before and after the changes, verifying
that we've achieved the expected code size reduction.

## Tasks

### 6.1 Run Q32 Metrics Script

Execute `scripts/lp-glsl-q32-metrics-app.sh`:

- This will generate a new report in `docs/reports/q32/`
- Report will include pre and post transform CLIF files
- Statistics will show instruction counts and code sizes

### 6.2 Compare with Baseline

Compare new report with baseline:

- Baseline: `docs/reports/q32/2026-01-24T01.26.02-pre-ops-builtins`
- Compare instruction counts for:
    - `test-add.glsl` functions (should see reduction in add operations)
    - `test-sub.glsl` functions (should see reduction in sub operations)
    - `test-div.glsl` functions (should see reduction in div operations)
    - `test-perlin.glsl` functions (should see overall reduction)

### 6.3 Verify Expected Reduction

Expected reductions:

- Each `fadd` operation: ~20 instructions → 1 call
- Each `fsub` operation: ~20 instructions → 1 call
- Each `fdiv` operation: ~30 instructions → 1 call
- Overall: Estimated 50-70% reduction in arithmetic-heavy code

## Success Criteria

- lp-glsl-q32-metrics-app script runs successfully
- New report generated with post-builtin code
- Code size reduction verified in comparison
- Instruction counts reduced as expected
- Report shows improvement over baseline

## Code Organization

- Place helper utility functions at the bottom of files
- Place more abstract things, entry points, and tests first
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete
  solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented
