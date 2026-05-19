# Phase 3: Cleanup And Validation

## Scope Of Phase

In scope:

- Remove temporary scaffolding.
- Confirm address/args remain deferred and documented.
- Confirm the event example project is checked in and covered by example loading/render validation.
- Run final validation for M1.
- Add a short summary file for the plan.

Out of scope:

- Implementing M2 button node or playlist behavior.
- Adding OSC/MIDI/DMX bridge code.

## Code Organization Reminders

- No stray commented-out Rust fields.
- No vague TODOs in production code.
- Documentation may explicitly mention deferred `address`/`args`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- All files touched in phases 1-3.
- `docs/roadmaps/2026-05-19-events-playlists-radio-nodes/notes.md`
- `docs/roadmaps/2026-05-19-events-playlists-radio-nodes/m1-control-message-trigger-event/summary.md`

Expected changes:

- Add `summary.md` describing:
  - `ControlMessage` fields,
  - `TriggerEvent` relationship,
  - bus routing validation,
  - shared compute/visual consumed-map support,
  - `examples/events`,
  - deferred address/args.
- Search for accidental address/args implementation and remove it from M1 if present.

## Final Validate

```bash
cargo fmt --check
cargo test -p lpc-model control_message
cargo check -p lpc-model --no-default-features
cargo test -p lpc-engine control_message
cargo test -p lpc-engine runtime_spine
cargo test -p lpc-engine engine_services
cargo test -p lp-cli --test examples_valid
cargo check -p lpa-server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```
