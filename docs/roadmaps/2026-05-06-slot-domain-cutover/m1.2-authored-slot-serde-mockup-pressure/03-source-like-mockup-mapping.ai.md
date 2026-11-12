# Phase 3: Source-Like Mockup Mapping

## Scope Of Phase

Make the mockup fixture mapping pressure the source-domain shapes that M2 will
need, without changing real `lpc-source`.

In scope:

- Add a source-like fixture mapping shape to `lpc-slot-mockup`.
- Use stable-key maps instead of arrays.
- Pressure nested enums, records, maps, and semantic slots.
- Keep existing mockup sync/diff/mutation tests working.

Out of scope:

- Real `lpc-source::fixture::MappingConfig` changes.
- Changing `examples/basic`.
- Supporting every future fixture mapping concept.

## Code Organization Reminders

- Prefer a separate `source/mapping.rs` if fixture mapping grows beyond a small
  enum.
- Keep one clear concept per file where it improves filesystem navigation.
- Keep manual enum access impls close to their enum definitions.
- Put helpers lower in files and tests at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-mockup/src/source/fixture_def.rs`
- `lp-core/lpc-slot-mockup/src/source/mod.rs`
- Possible new file: `lp-core/lpc-slot-mockup/src/source/mapping.rs`
- `lp-core/lpc-slot-mockup/src/model/mod.rs`
- `lp-core/lpc-slot-mockup/src/tests/*`

Target shape:

```rust
FixtureMapping::PathPoints {
    paths: MapSlot<u32, PathSpec>,
    sample_diameter: PositiveF32Slot,
}

PathSpec::RingArray {
    center: XySlot,
    diameter: PositiveF32Slot,
    start_ring: ValueSlot<u32>,
    end_ring: ValueSlot<u32>,
    ring_lamp_counts: MapSlot<u32, ValueSlot<u32>>,
    offset_angle: ValueSlot<f32>,
    order: RingOrderSlot,
}
```

Notes:

- `RingOrderSlot` may be a mockup-local field type or a semantic slot if it
  clearly belongs in `lpc-model`. Prefer mockup-local unless the type is
  obviously reusable.
- The array-to-map conversion is intentional. Stable keys are the slot-domain
  rule.
- Use derived records where possible and manual enum access only where needed.
- Existing mockup tests may need path updates because `mapping` becomes more
  structured.

Tests:

- Generic server tree walk includes nested mapping paths.
- Incremental change can modify a deep nested map value.
- Map key removal prunes client data.

## Validate

```bash
cargo fmt --package lpc-slot-mockup
cargo test -p lpc-slot-mockup
```
