# Design: Metadata-Driven Mockup Codec Codegen

## Scope Of Work

Turn the current working no-Serde mockup codec from hardcoded generated text
into generation driven by a compact `SlotCodec` metadata model.

Design principle:

`SlotCodec` is a LightPlayer-shaped projection of Serde. Prefer Serde's proven
architecture where it fits, especially the split between format readers/writers,
generated per-type impls, defaults, tags, skips, and derive-time field models.
Deviate where LightPlayer needs a narrower slot-native model, streaming reads,
slot reflection, no runtime value tree, and embedded code-size discipline.

In scope:

- Add a small SlotCodec model in `lpc-slot-codegen`.
- Build a mockup source-root codec module from model plus temporary explicit hook
  metadata.
- Generate root reader/writer adapters for:
  - `ProjectDef`
  - `OutputDef`
  - `TextureDef`
  - `FixtureDef`
  - `ShaderDef`
- Keep specialized leaf/enum helpers explicit until common patterns stabilize.
- Keep all current generated-code tests passing.
- Keep `lpc-slot-mockup` free of direct Serde derives, attributes, and
  dependencies.

Out of scope:

- Production adoption.
- Removing Serde from `lpc-model` or other core crates.
- Derive-macro or module-local codec generation.
- Full metadata generation for every specialized leaf/enum helper.

## File Structure

```text
lp-core/lpc-slot-codegen/src/
  lib.rs
    generate_mockup_slot_codec()
    SlotCodec model types, initially local to this file
    source-root SlotCodec metadata table
    root reader/writer renderers
    existing slot shape/view generation

lp-core/lpc-slot-mockup/
  build.rs
    invokes generate_mockup_slot_codec()
  src/
    lib.rs
      generated_slot_codec include
    source/
      project_def.rs
      output_def.rs
      texture_def.rs
      fixture_def.rs
      mapping.rs
      shader_def.rs
    tests/
      generated_shape_codec.rs

lp-core/lpc-model/src/slot_codec/
  slot_reader.rs
  slot_json_writer.rs
  mod.rs
```

If `lpc-slot-codegen/src/lib.rs` becomes too large during implementation, split
the codec-specific pieces into search-friendly files such as:

```text
lp-core/lpc-slot-codegen/src/
  slot_codec_model.rs
  mockup_slot_codec.rs
```

Do this only when it reduces real friction; avoid churn just for tidiness.

## Architecture Summary

Current generated output already works. This plan changes the generator shape,
not the runtime codec semantics.

```text
source scan + temporary hook metadata
        │
        ▼
SlotCodecModule / SlotCodecRoot / SlotCodecField model
        │
        ▼
root reader/writer renderers
        │
        ▼
generated_slot_codec.rs
        │
        ▼
SlotReader / SlotJsonWriter shared helpers
```

The generated root adapters should keep the pattern proven by the mockup:

- Create or reference a default instance where omitted fields should use
  default leaves.
- Require `kind` as the first discriminator for source roots.
- Scan properties with `ObjectReader`.
- Use shared map/array helpers from `slot_codec`.
- Call explicit constructor/access hooks for private fields.
- Delegate specialized leaves/enums to explicit helpers.

## Main Components

### SlotCodec Model

Initial model should be intentionally small.

Suggested shape:

```rust
struct SlotCodecModule {
    roots: Vec<SlotCodecRoot>,
}

struct SlotCodecRoot {
    rust_type: &'static str,
    fn_stem: &'static str,
    kind_expr: &'static str,
    default_expr: Option<&'static str>,
    constructor: SlotCodecConstructor,
    fields: Vec<SlotCodecField>,
}

struct SlotCodecField {
    wire_name: &'static str,
    local_name: &'static str,
    read_expr: &'static str,
    write_expr: &'static str,
    default_expr: Option<&'static str>,
    skip_read: bool,
    omit_if: Option<&'static str>,
}
```

Exact type names can change during implementation, but the important idea is
that root read/write code is rendered from data, not handwritten per root.

### Temporary Hook Metadata

Use an explicit table for the mockup source roots.

This table can name:

- constructor calls like `OutputDef::from_codec(pin, options)`
- default expressions like `OutputDef::default()`
- accessor expressions like `output.pin()`
- specialized helper calls like `read_dim2u(prop.value())?`
- fields to skip/read-discard such as `bindings` or `sampling`

This is a bridge. The long-term direction is derive-emitted or module-local
codec impls where private fields can be accessed without exposing broad public
APIs.

### Root Readers

Render root readers from `SlotCodecRoot`.

Expected generated shape:

```rust
pub fn read_output_def<S>(reader: &mut SlotReader<'_, S>) -> Result<OutputDef, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "pin", "bindings", "options"];
    let defaults = OutputDef::default();
    let mut pin = defaults.pin();
    let mut options = defaults.options().cloned();
    let mut object = reader.object()?;
    let _kind = object.expect_discriminator("kind", &[OutputDef::KIND])?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "pin" => pin = prop.value().u32()?,
            "bindings" => prop.value().skip_value()?,
            "options" => options = Some(read_output_driver_options(prop.value())?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(OutputDef::from_codec(pin, options))
}
```

### Root Writers

Render root JSON writers from `SlotCodecRoot`.

Expected generated shape:

```rust
pub fn write_output_def_json(output: &OutputDef) -> Vec<u8> {
    let mut out = Vec::new();
    let mut writer = SlotJsonWriter::new(&mut out);
    let mut object = writer.object().unwrap();
    object.prop("kind").unwrap().string(OutputDef::KIND).unwrap();
    object.prop("pin").unwrap().u32(output.pin()).unwrap();
    if let Some(options) = output.options() {
        write_output_driver_options(object.prop("options").unwrap(), options);
    }
    object.finish().unwrap();
    out
}
```

### Specialized Helpers

Keep these explicit during this plan:

- `read_dim2u` / `write_dim2u`
- `read_affine2d` / `write_affine2d`
- `read_scalar_hint` / `write_scalar_hint`
- `read_mapping_config` / `write_mapping_config`
- `read_path_spec` / `write_path_spec`
- `read_glsl_opts` / `write_glsl_opts`
- `read_shader_param_def` / `write_shader_param_def`

The plan may move these into clearer named sections, but should not attempt to
fully infer them from slot metadata yet.

## Validation Strategy

The primary acceptance contract is:

```bash
cargo test -p lpc-slot-mockup generated_shape_codec
cargo test -p lpc-slot-mockup
```

Supporting validation:

```bash
cargo test -p lpc-model slot_codec
cargo test -p lpc-slot-codegen
cargo check -p lpc-model --no-default-features
cargo check -p lpc-wire --no-default-features
cargo check -p lpc-slot-mockup
```

Also verify:

```bash
rg -n "serde|Serialize|Deserialize|serde_json" lp-core/lpc-slot-mockup
```

This search should return no direct mockup Serde references.
