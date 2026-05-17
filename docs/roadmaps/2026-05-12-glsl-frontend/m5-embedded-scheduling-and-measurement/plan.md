# M5a Plan - Compile Job Budget Semantics

## Goal

Make the resumable `lps-glsl::CompileJob` API genuinely budget-aware before wiring it into runtime
scheduling. This gives firmware and emulator callers a stable coarse-grained yielding contract:
advance at most N compiler stages, then return control.

## Scope

In scope:

- Give `CompileBudget::default()` an unbounded synchronous meaning.
- Preserve `CompileBudget::single_step()` as exactly one coarse compiler stage.
- Add `CompileBudget::steps(n)` for explicit multi-stage slices.
- Expose the current coarse `CompileStage`.
- Add tests for single-step progress, multi-step progress, and default-budget completion.

Out of scope:

- Runtime scheduler integration.
- Timing and heap watermark capture.
- Expression-level or statement-level yielding.

## Rationale

M1 introduced the resumable job shape, but `max_steps` was not yet honored. M5 needs measurement and
scheduling, and both become much easier if the job API first has clear semantics:

- `single_step`: useful for deterministic tests and frame-by-frame scheduling experiments.
- `steps(n)`: useful for firmware knobs.
- `default`: useful for existing synchronous compile wrappers.

## Validation

- `cargo test -p lps-glsl`
- `cargo check -p lps-glsl --target riscv32imac-unknown-none-elf`
