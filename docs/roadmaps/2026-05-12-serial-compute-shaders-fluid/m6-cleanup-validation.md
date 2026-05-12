# Milestone 6: Cleanup And Validation

## Title And Goal

Stabilize the compute/fluid domain slice after the end-to-end example works.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-serial-compute-shaders-fluid/m6-cleanup-validation/`

## Scope

In scope:

- Remove temporary scaffolding.
- Tighten docs and rustdocs.
- Validate host, emulator, and ESP32 builds.
- Audit memory and profile behavior.
- Ensure the shader taxonomy is named clearly.
- Update roadmap summary with what was learned.

Out of scope:

- New compute shader features.
- New fluid algorithms.
- Wgpu implementation.

## Key Decisions

- Cleanup is a separate milestone because this effort will touch many layers.
- The final state should leave clear extension points for wgpu, input nodes, and
  richer compute outputs.

## Deliverables

- Passing focused tests and CI-oriented checks.
- Updated `docs/lp-core` concepts if needed.
- Roadmap summary.
- Any future work split out into `future.md`.

## Dependencies

- Milestone 5 example has landed.

## Execution Strategy

Small plan. The shape will be clear by then, but validation and cleanup should
be tracked explicitly.

