# M1.2 Authored Slot Serde Mockup Pressure Design

## Scope Of Work

M1.2 prepares M2 by proving that slot-aware Rust domain structs can also be
clean authored source data. The work should use `lpc-slot-mockup` as the
pressure harness and avoid converting real `lpc-source` definitions.

In scope:

- Authored serde for typed slot wrappers:
  - `ValueSlot<T>`
  - `MapSlot<K,V>`
  - `OptionSlot<T>`
- Authored serde for all current semantic slot leaf types under
  `lpc-model/src/slot/slots/`.
- A more source-like mockup fixture mapping that pressures stable-key maps,
  nested records, enums, options, and semantic leaves.
- Generated authored TOML evidence files written to a gitignored path.
- Round-trip tests that serialize mockup source defs, parse them back, and
  prove slot traversal/snapshotting still works.

Out of scope:

- Converting real `lpc-source` defs.
- Changing `examples/basic`.
- Changing the engine project loader.
- Client mutation.
- Generic UI changes.

## File Structure

```text
lp-core/lpc-model/src/slot/
  value_slot.rs
  slots/
    affine2d.rs
    artifact_path.rs
    color_order.rs
    dim2u.rs
    positive_f32.rs
    ratio.rs
    relative_node_ref.rs
    render_order.rs
    resource_ref.rs
    source_path.rs
    xy.rs

lp-core/lpc-slot-mockup/src/
  source/
    fixture_def.rs
    mapping.rs              # new source-like mockup mapping concepts
    mod.rs
    output_def.rs
    project_def.rs
    shader_def.rs
    texture_def.rs
  tests/
    authored_serde.rs       # generated TOML evidence + round-trip assertions
    fixture.rs              # shared harness helpers if needed

target/slot-mockup-evidence/
  source-basic/
    project.toml
    shader.toml
    fixture.toml
    output.toml
    texture.toml
```

`target/` is already ignored, so generated evidence files should not become a
second maintained source of truth.

## Architecture Summary

Typed slot wrappers have two representations:

- **Authored representation:** clean TOML/serde data, shaped like the value an
  engineer or UI would edit.
- **Slot/wire representation:** `SlotData` snapshots/diffs with explicit
  versions.

M1.2 makes wrapper serde use the authored representation:

- `ValueSlot<T>` serializes/deserializes as `T`.
- `MapSlot<K,V>` serializes/deserializes as a normal map/table.
- `OptionSlot<T>` serializes/deserializes as `Option<T>`.
- Semantic slots serialize/deserialize as their clean authored value.

Deserialization stamps wrapper versions with `current_state_version()`. Tests
should set the ambient version before parsing and assert parsed fields carry
that version.

The mockup source model remains the pressure harness. It should build source
defs in Rust, serialize them to generated TOML evidence, parse the evidence
back, then use the real slot registry/access/sync path to verify behavior.

## Main Components And Interactions

### Authored Wrapper Serde

`ValueSlot<T>`, `MapSlot<K,V>`, and `OptionSlot<T>` should implement serde in
terms of their inner authored data. This avoids per-source-def adapters and
lets future real `lpc-source` structs remain the source of truth.

Expected behavior:

- `ValueSlot::new(3u32)` serializes as `3`.
- `MapSlot<String, ShaderParamDef>` serializes as a TOML map/table.
- `OptionSlot<ScalarHint>::none()` serializes as absent when source structs use
  `skip_serializing_if`, and deserializes as `None` through the wrapper.

### Semantic Slot Serde

Every semantic slot in `slot/slots/` should serialize as its authored value:

- numeric semantic slots as numbers,
- path/ref slots as strings,
- `Dim2uSlot` and `Affine2dSlot` as their structured values,
- `ColorOrderSlot` as the enum/string representation already exposed by its
  semantic value,
- `ResourceRefSlot` as whatever clean resource reference serde already exists.

If a semantic slot exposes a value that is not yet serde-ready, the phase should
make that explicit with tests and a narrow implementation, not by falling back
to serializing internal `Versioned<T>`.

### Source-Like Mockup Mapping

The mockup fixture mapping should grow a source-like `PathPoints` shape that
uses stable-key maps instead of arrays.

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

The array-to-map conversion is accepted pain. Stable keys are the slot-domain
rule, and M1.2 should show what that authored TOML looks like before M2 changes
real source examples.

### Generated TOML Evidence

Tests should generate authored TOML evidence from representative Rust mockup
models. The evidence should be written under `target/slot-mockup-evidence/`,
then parsed back and verified.

This gives us inspectable artifacts without maintaining hand-authored fixtures.
The test output should print the evidence path and enough context to make
manual inspection easy.

## Validation

Expected final validation:

```bash
cargo fmt --package lpc-model --package lpc-slot-mockup
cargo test -p lpc-model --lib --tests
cargo test -p lpc-model --features derive --test slot_record_derive
cargo test -p lpc-slot-mockup -- --nocapture
cargo check -p lpc-model --features schema-gen
cargo clippy -p lpc-model --all-targets -- -D warnings
cargo clippy -p lpc-slot-mockup --all-targets -- -D warnings
```
