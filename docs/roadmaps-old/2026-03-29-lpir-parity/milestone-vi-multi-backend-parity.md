# Milestone VI: Multi-backend parity (WASM / RV32 sweep + comparison tooling)

## Goal

All three backends (`jit.q32`, `wasm.q32`, `rv32.q32`) have 0 unexpected failures. Build tooling
to make cross-target comparison easy and repeatable.

## Suggested plan name

`lpir-parity-milestone-vi`

## Scope

**In scope:**

### Comparison tooling

- **Multi-target report mode** for `lps-filetests-app`: a `--report` (or similar) flag that
  runs all targets and produces a structured report with:
  - Per-file, per-target status (pass / fail / unimplemented / broken / unsupported).
  - **Cross-target discrepancies**: files that pass on one target but fail on another.
  - Summary stats per target.
  - Output as markdown (human-readable) and optionally machine-readable (JSON/CSV) for
    diffing between runs.
- Integrate with `scripts/glsl-filetests.sh` or as a separate script.

### WASM / RV32 sweep

- Run the full corpus on `wasm.q32` and `rv32.q32`.
- Triage failures:
  - **Shared LPIR bug** → fix in `lps-naga` / `lpir`.
  - **Backend emit bug** → fix in `lps-wasm` or `lpir-cranelift`.
  - **Intentional platform limit** → annotate `@unsupported(backend=…, reason="…")`.
  - **Unimplemented on that backend** → annotate `@unimplemented(backend=…)`.
- Do **not** remove existing `@unimplemented(backend=wasm)` wholesale without verifying the test
  passes; triage incrementally.

**Out of scope:**

- New GLSL features not covered by Milestones I–V.
- Struct support.

## Key decisions

- **Report format:** Markdown is primary (can be committed to `docs/reports/`). JSON is optional
  for scripted diffing.
- **When to annotate vs fix:** If a WASM/RV32 failure is a real emit bug and small, fix it. If
  it's a larger structural gap in the backend, annotate `@unimplemented` and document in the
  report.

## Deliverables

- Multi-target report command in `lps-filetests-app`.
- Generated parity report in `docs/reports/`.
- WASM / RV32 failures triaged: fixed, annotated, or documented.
- All three targets at 0 unexpected failures.

## Dependencies

Milestones I–V (jit.q32 must be clean before sweeping other targets makes sense).

## Estimated scope

Medium. Tooling is ~200-400 lines (report generation, CLI integration). WASM/RV32 triage depends
on how many failures remain after shared LPIR fixes carry over — likely small if the LPIR layer
is correct.
