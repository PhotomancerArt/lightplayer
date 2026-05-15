# 07 - Cleanup And Validation

## Scope of phase

Clean up the factory milestone and run final validation.

In scope:

- Remove temporary debugging artifacts.
- Search for TODOs/stubs introduced by this plan.
- Review generated code size/shape qualitatively.
- Update docs if the final API differs from the design.
- Run final validation commands.

Out of scope:

- Binary size measurement.
- Rewriting generated codec deserialization.
- New wire operations.

## Code organization reminders

- Avoid large generic files when a concept has its own name.
- Keep public exports intentional.
- Keep generated code small and easy to inspect.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Review:

```bash
rg -n "TODO|todo|stub|unimplemented|panic!|hack" \
  lp-core/lpc-model/src/slot \
  lp-core/lpc-slot-codegen/src \
  lp-core/lpc-slot-mockup/src
```

Expected final state:

- `SlotShapeRegistry::create_default` exists and returns `Box<dyn SlotMutAccess>`.
- Static codegen installs typed factories.
- Dynamic shapes create `DynamicSlotObject`.
- Snapshot/apply remains metadata-only and restores explicit unsupported
  factory behavior.
- Existing mutation behavior remains conservative.
- Mockup proves static/dynamic default creation and explicit map insertion.

Write `summary.md` in the plan directory with:

- what was built
- important decisions
- deviations from the plan
- remaining follow-up work

## Validate

```bash
cargo fmt -p lpc-model -p lpc-slot-codegen -p lpc-slot-mockup --check
cargo test -p lpc-model
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --no-default-features
```
