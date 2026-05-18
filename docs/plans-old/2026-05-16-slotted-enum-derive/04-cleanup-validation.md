# Phase 4: Cleanup And Validation

## Scope Of Phase

Clean up remaining manual enum-slot scaffolding, update docs, and run the final
targeted validation set.

In scope:

- remove unused imports after manual impl deletion
- remove dead helper functions and stale tests
- update slot docs for enum derive
- run final validation

Out of scope:

- serde removal outside touched enums
- new serializer features
- code size measurement pass

## Code Organization Reminders

- Do not leave commented-out manual impls.
- Do not keep parallel helper lists of variant names if derive now owns them. Prefer Rust variant names as slot discriminators.
- Keep tests at the bottom of each Rust source file.
- Prefer precise docs over broad claims.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Docs to update:

- `docs/design/slots/overview.md`
- optionally `docs/design/slots/serialization.md` if it still describes manual
  enum handling inaccurately

Documentation should state:

- `Slotted` covers records, wrappers, and structured enum payloads.
- `EnumSlot<T>` owns the active variant revision.
- raw slotted enums do not generally act as runtime root objects.
- use `SlotValue` / `LpValue::Enum` for atomic enum leaves.

Search cleanup:

```bash
rg -n "impl SlotEnumShape|impl SlottedEnum|impl SlottedEnumMut|default_variant|mapping_shape|path_spec_shape" \
  lp-core/lpc-model/src lp-core/lpc-slot-mockup/src
```

Manual impls may remain in tests or in truly dynamic/custom enum cases, but
`NodeDef`, `MappingConfig`, and `PathSpec` should no longer need them.

## Validate

```bash
cargo fmt -p lpc-slot-macros -p lpc-slot-codegen -p lpc-model -p lpc-slot-mockup -p lpc-engine
cargo check -p lpc-model
cargo test -p lpc-model
cargo test -p lpc-model --features derive --test slotted_enum_derive
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup
cargo test -p lpc-engine project_loader
git diff --check
```
