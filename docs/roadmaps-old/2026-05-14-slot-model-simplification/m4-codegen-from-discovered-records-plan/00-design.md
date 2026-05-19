# Design

## Scope

Replace the mockup slot codec's hand-authored record schema with code generated
from discovered slot records, then remove codec-only constructors from the
mockup domain.

The plan proves the pattern in the mockup. Production loader adoption is later.

## File Structure

```text
lp-core/lpc-slot-codegen/src/lib.rs
  discovery model
  shape generation
  view generation
  mockup codec generation

lp-core/lpc-slot-mockup/build.rs
  calls slot shape generation
  calls generated mockup codec generation

lp-core/lpc-slot-mockup/src/source/
  project_def.rs
  output_def.rs
  texture_def.rs
  fixture_def.rs
  shader_def.rs
  mapping.rs

lp-core/lpc-slot-mockup/src/generated_slot_codec.rs
  include!(OUT_DIR generated file)

lp-core/lpc-model/src/slot_codec/
  syntax readers/writers and shared helper surface
```

## Architecture Summary

The generator should have one discovered model of slot records and fields. That
model feeds shape generation, view generation, and codec generation. Codec
generation should not maintain a second list of fields.

Generated record readers should construct actual slot field types directly.
For example, a decoded `render_size` field should become a `Dim2uSlot`, not a
plain `Dim2u` that later passes through `FixtureDef::from_codec`.

Generated record writers should use field types or existing domain accessors in
a predictable way, with shared helper functions for common field families. The
generated code may still have a few explicit mockup/domain policies, but those
policies must be visible in the generator and not hidden in domain constructors.

Enum/discriminator handling remains explicit in this milestone. The record
field list should be discovered; enum variant body readers can remain custom
or metadata-driven until there is a clean derive story for slot enums.

## Main Components

### Discovered Slot Model

Add or evolve codegen structs such as:

```rust
struct DiscoveredSlotRecord {
    type_path: String,
    type_name: String,
    fn_stem: String,
    fields: Vec<DiscoveredSlotField>,
}

struct DiscoveredSlotField {
    name: String,
    rust_type: SlotFieldType,
    codec_policy: CodecFieldPolicy,
}
```

The exact names can differ, but the important part is that shape/view/codec
codegen use the same discovered data where possible.

### Field Type Lowering

The generator needs enough type recognition to lower common slot field shapes:

- `ValueSlot<T>`
- semantic aliases such as `Dim2uSlot`, `Affine2dSlot`, `ColorOrderSlot`
- `OptionSlot<T>`
- `MapSlot<K, V>`
- nested `SlotRecord` fields
- explicit enum fields such as `MappingConfig`
- fields intentionally omitted from a given codec surface

Keep the first pass narrow and test-driven against the mockup source records.

### Generated Readers

Generated readers should:

- create defaults when a record has `Default`
- initialize mutable field variables as actual field types
- parse known fields with shared helper functions
- assign slot field values directly
- reject unknown fields with the existing friendly errors
- construct the record literal directly

Example target shape:

```rust
let defaults = FixtureDef::default();
let mut render_size = defaults.render_size.clone();
...
while let Some(mut prop) = object.next_prop()? {
    match prop.name() {
        "render_size" => render_size = Dim2uSlot::new(read_dim2u(prop.value())?),
        ...
    }
}
Ok(FixtureDef {
    render_size,
    bindings,
    sampling,
    mapping,
    color_order,
    transform,
    brightness,
    gamma_correction,
})
```

### Generated Writers

Generated writers should:

- write `kind` for top-level discriminated records
- write fields using common helpers for `ValueSlot<T>` and wrapper aliases
- preserve current mockup JSON/TOML shapes unless intentionally changed
- keep custom enum writers explicit for now

### Escape Hatches

Complex cases should be local and visible:

- custom semantic value conversion lives on `T: SlotValue`
- custom enum/discriminator bodies live near enum helper generation
- full custom handling remains possible, but should be named as custom

## Non-Goals

- No generic Serde replacement surface.
- No private field constructor inference.
- No workspace-wide production adoption.
- No binary size optimization pass beyond avoiding obviously verbose generated
  code.
