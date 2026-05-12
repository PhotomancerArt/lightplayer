# Phase 3: Fixture Mapping Defs

## Scope Of Phase

Convert fixture source defs and mapping data to the slot-domain shape, including
stable keyed maps instead of authored arrays.

In scope:

- Convert:
  - `FixtureDef`
  - `ColorOrder` or a replacement/re-export using `ColorOrderSlot`
  - `MappingConfig`
  - `PathSpec`
  - `RingOrder`
- Replace authored arrays in fixture mapping with stable keyed maps.
- Update `examples/basic/fixture.toml`.
- Add fixture-specific source slot tests with printed tree evidence.

Out of scope:

- Runtime fixture node state.
- Final real-world fixture examples beyond `examples/basic`.
- Adding `SlotShape::Array`.
- Custom serde that preserves old array syntax while pretending it is a map.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep enum shape/access code near the enum it describes.
- Put helpers lower in the file when that improves readability.
- If manual `SlotEnumAccess` is needed, keep the match arms direct and boring.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-source/src/node/fixture/fixture_def.rs`
- `lp-core/lpc-source/src/node/fixture/mapping.rs`
- `lp-core/lpc-model/src/slot/slots/affine2d.rs`
- `lp-core/lpc-model/src/slot/slots/color_order.rs`
- `lp-core/lpc-model/src/slot/value_slot.rs`
- `examples/basic/fixture.toml`

Expected source shape:

- `FixtureDef`:
  - `output_loc: RelativeNodeRefSlot`
  - `texture_loc: RelativeNodeRefSlot`
  - `mapping: MappingConfig`
  - `color_order: ColorOrderSlot`
  - `transform: Affine2dSlot` if the real 4x4 matrix can be reduced to the
    desired authored 2D affine transform for M2.
  - `brightness`: an optional or positive/range slot that matches the intended
    authored meaning.
  - `gamma_correction`: optional or direct boolean slot depending on whether
    absence still matters.
- `MappingConfig`:
  - likely enum with `PathPoints { paths: MapSlot<u32, PathSpec>,
    sample_diameter: PositiveF32Slot }`.
- `PathSpec::RingArray`:
  - use slot-aware fields.
  - `ring_lamp_counts` should become `MapSlot<u32, ValueSlot<u32>>` or a small
    record value if index/key semantics need labels.
- `RingOrder`:
  - can be a semantic string leaf or manual enum shape.

Do not silently make `mapping` an opaque `ValueSlot<MappingConfig>` unless this
phase is blocked; if that happens, stop and report because it changes the plan.

Test expectations:

- Load updated `examples/basic/fixture.toml`.
- Register source shapes.
- Snapshot/walk `fixture`.
- Assert stable map paths such as:
  - `mapping.path_points.paths[1]...`
  - ring counts through keyed map syntax.

## Validate

```bash
cargo fmt --package lpc-source
cargo test -p lpc-source --lib --tests
cargo check -p lpc-source --features schema-gen
```
