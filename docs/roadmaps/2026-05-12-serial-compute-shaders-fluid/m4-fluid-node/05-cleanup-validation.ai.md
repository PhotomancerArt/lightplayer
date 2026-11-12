# Phase 5: Cleanup And Validation

## Scope Of Phase

Finish the plan by cleaning up docs, TODOs, tests, and validation issues.

In scope:

- Remove temporary debug code.
- Ensure rustdocs on new slot semantics explain the model.
- Ensure fluid docs explain tick/render separation.
- Update roadmap/todo notes if the implementation reveals follow-up work.
- Run final validation.

Out of scope:

- M5 visible compute-fluid example.
- UI feature work.
- GPU/wgpu design.

## Code Organization Reminders

- Keep concept files small and named after primary exported types.
- Avoid large `mod.rs` bodies.
- Tests stay at the bottom.
- Do not leave commented-out experiments.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Review:

- `lp-core/lpc-model/src/slot/`
- `lp-core/lpc-slot-macros/src/`
- `lp-core/lpc-model/src/nodes/fluid/`
- `lp-core/lpc-engine/src/nodes/fluid/`
- `lp-core/lpc-engine/src/engine/project_loader.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `docs/roadmaps/2026-05-12-serial-compute-shaders-fluid/`

Potential follow-up notes:

- General required-slot validation.
- General produced/consumed binding validation.
- More efficient bilinear/fixed-point sampler.
- M5 end-to-end compute-fluid example.
- UI display of slot semantics.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-engine
cargo check -p lpc-engine
```

