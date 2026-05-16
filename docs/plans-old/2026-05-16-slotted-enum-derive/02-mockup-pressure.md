# Phase 2: Mockup Pressure

## Scope Of Phase

Use the new enum derive in the mockup first, where failures are cheaper and the
domain shape mirrors the real model.

In scope:

- convert mockup `MappingConfig`
- convert mockup `PathSpec`
- convert mockup `NodeDef` if it still has manual slot enum machinery
- remove replaced manual enum shape/access/default code
- keep existing mockup storage/wire tests passing

Out of scope:

- converting real model enums
- changing authored TOML/JSON syntax
- changing mockup behavior except removing boilerplate

## Code Organization Reminders

- Prefer deriving on the enum directly rather than creating helper static lists.
- Keep semantic constructors such as `path_points_vec` if they are useful.
- Remove dead shape helper functions once derive replaces them.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Primary files:

- `lp-core/lpc-slot-mockup/src/source/mapping.rs`
- `lp-core/lpc-slot-mockup/src/source/node_def.rs`

Expected shape:

```rust
#[derive(Clone, Debug, PartialEq, Slotted)]
pub enum MappingConfig {
    #[default]
    Unset,
    PathPoints {
        pub paths: MapSlot<u32, EnumSlot<PathSpec>>,
        pub sample_diameter: PositiveF32Slot,
    },
}
```

The exact visibility may need adjustment. If fields are private today, either
make them public or keep a custom impl temporarily only if a real invariant
requires privacy. The preferred model-layer rule is public slot fields.

Remove manual impls replaced by derive:

- `SlotEnumShape`
- `SlottedEnum`
- `SlottedEnumMut`
- `SlotRecordAccess`
- `SlotRecordMutAccess`
- shape helper functions used only by those impls
- manual `default_variant` if no callers need it

While converting `MappingConfig`, add a neutral nop/default variant (`Unset`
unless a better local name emerges). Existing tests that require actual mapping
data should explicitly use `PathPoints`.

Keep constructors and convenience methods that are domain-facing and still used
by tests or examples.

## Validate

```bash
cargo fmt -p lpc-slot-mockup
cargo test -p lpc-slot-mockup
```
