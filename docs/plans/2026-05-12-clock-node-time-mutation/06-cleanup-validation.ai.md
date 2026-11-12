# Phase 6: Cleanup And Validation

## Scope Of Phase

Clean up the clock/time/mutation slice and validate it across host and firmware-relevant targets.

In scope:

- Remove temporary code and stale TODOs introduced during the plan.
- Ensure docs and rustdocs describe clock controls, transient persistence, inline invocations, and mutation scope.
- Tune fluid example to be visually readable.
- Verify examples still load.
- Run final validation commands.

Out of scope:

- Large refactors unrelated to this plan.
- Full CI if local time is constrained, unless specifically requested before push.

## Code Organization Reminders

- Keep one concept per file.
- Do not leave important concepts hidden in `mod.rs`.
- Tests stay at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Review:

- `NodeInvocation` rustdocs should not talk about a temporary plan.
- `ClockDef` and `ClockControls` docs should explain persisted vs transient fields.
- `SlotPolicy` docs should explain mutability/persistence as tooling/writeback hints.
- `ProjectReadRequest` docs should state reads are stateless and mutations are explicit client requests.
- Debug UI should not expose raw debug strings in normal clock controls.

Example validation:

- `examples/basic` should include a clock and bind shader time.
- `examples/fluid` should include a clock, bind compute time, and use direct `time` in GLSL.
- Fluid emitters should move visibly and not saturate immediately.

## Validate

```bash
cargo fmt
cargo test -p lpc-model
cargo test -p lpc-wire
cargo test -p lpc-view
cargo test -p lpc-engine
cargo check -p lpa-server
cargo check -p lp-cli
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If preparing to push, run the repo gate:

```bash
just check
just build-ci
just test
```
