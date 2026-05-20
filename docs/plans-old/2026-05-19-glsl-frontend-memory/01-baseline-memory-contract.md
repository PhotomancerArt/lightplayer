# Phase 1: Baseline Memory Contract

## Scope Of Phase

Establish the memory baseline and acceptance criteria before changing code.

In scope:

- Add or update plan-local measurement notes.
- Capture current type sizes and stack hotspots.
- Capture current device/firmware memory behavior using the existing `fwcheck` path when hardware is available.
- Identify the exact commands future phases should rerun after the refactor.

Out of scope:

- Changing compiler behavior.
- Refactoring HIR, typechecking, or lowering.
- Changing firmware heap size.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Create:

- `docs/plans/2026-05-19-glsl-frontend-memory/measurements.md`

Record:

- Current branch and commit.
- Current `lp-fw/fw-esp32/src/board/esp32c6/init.rs` heap reservation.
- Current known sizes of:
  - `HirExpr`
  - `HirExprKind`
  - `HirPlace`
  - `HirAssignTarget`
  - `LpsType`
  - `ParsedExpr`
- Current top RV32 stack frames for `lps-glsl`.
- Current `just demo-esp32c6-check basic` result if hardware is attached.
- Current memory lines from the device trace, especially project load before/after and shader compile context.

If there is no existing size-print helper, add a temporary local-only measurement snippet while working, then remove it before finishing the phase. Do not leave test-only size assertions in production code unless they are broad enough to be stable and useful.

Suggested measurement method for stack frames:

1. Build the ESP32 firmware with stack size emission enabled if the local toolchain supports it.
2. Inspect generated stack-size metadata for `lps_glsl::hir::*` symbols.
3. Record only the top relevant frames in `measurements.md`.

If stack-size emission is too brittle locally, use the measured values from `00-notes.md` as the baseline and clearly mark them as inherited from the previous pass.

Acceptance criteria for later phases:

- No regression in shader behavior.
- No increase to firmware heap reservation as the primary fix.
- `HirExpr` should stop being passed/returned recursively by value.
- The largest `lps-glsl` typechecker stack frames should fall materially after the arena phases.
- `just demo-esp32c6-check basic` should complete without panic/OOM on the ESP32-C6.

## Validate

```bash
git status --short
cargo check -p lps-glsl
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
just demo-esp32c6-check basic
```

If the device is unavailable, skip the final command and record that in `measurements.md`.
