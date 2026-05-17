# M1 Vertical Slice: Mockup Storage/Wire Codec

## Goal

Build a working mock disk-storage and wire-storage engine using the native slot
codec, with source shapes that closely mirror the current real model.

This proves the serialization rules before production source loading or project
sync depend on them.

## Target

Start with `lpc-slot-mockup`.

Why:

- It already has source defs, invocations, engine roots, dynamic shader params,
  maps, enums, options, value leaves, sync, mutation, and JSON/TOML evidence.
- It is small enough to reshape without churning production code.
- It can be made to resemble current production TOML/JSON closely enough to
  expose real rough spots.

## Scope

In scope:

- Update mockup source models to match current production concepts.
- Add reusable native codec primitives in real code.
- Mock TOML disk-storage tests.
- Mock JSON wire-storage tests.
- Owned slot-data encode/decode where useful.
- Borrowed/direct JSON writer tests where useful.
- Notes on every mismatch between mockup format and current real format.

Out of scope:

- Replacing all TOML loading.
- Replacing real JSON protocol parsing.
- Choosing a new TOML parser.
- Removing Serde derives from host tooling.
- Reworking project sync.
- Measuring firmware size before one real adoption path exists.

## Phase 1: Refresh Mockup Shape

Update `lpc-slot-mockup` to match current real model pressure:

- `ProjectDef`
  - skipped loader `kind`
  - optional `name`
  - `nodes: MapSlot<String, NodeInvocationDef>`
- `NodeInvocationDef`
  - artifact-only
- `ShaderDef`
  - `glsl_path`
  - `render_order`
  - `bindings` or a mock equivalent if production `BindingDefs` is too much
  - `glsl_opts`
  - `param_defs`
  - remove old mock-only `texture_loc`
- `TextureDef`
  - `size`
  - bindings/mock bindings
- `OutputDef`
  - `pin`
  - bindings/mock bindings
  - optional `options`
- `FixtureDef`
  - `render_size`
  - bindings/mock bindings
  - skipped/default `sampling` if needed for TOML parity
  - `mapping`
  - `color_order`
  - `transform`
  - optional `brightness`
  - optional `gamma_correction`
- `MappingConfig`
  - `PathPoints { paths, sample_diameter }`
  - `PathSpec::RingArray`
  - `ring_lamp_counts: MapSlot<u32, ValueSlot<u32>>`
  - `RingOrder`

The goal is not exact production type reuse. The goal is matching the same
serialization concepts.

## Persistence Invariant

Every mockup file/object that is persisted as its own disk artifact should be a
slot root.

That means these mockup source files should each map to a concrete
`#[slot(root)]` type:

- `project.toml` -> `ProjectDef`
- `shader.toml` -> `ShaderDef`
- `texture.toml` -> `TextureDef`
- `output.toml` -> `OutputDef`
- `fixture.toml` -> `FixtureDef`

Nested persisted structures are fine, but only as data inside one of those
roots:

- node invocations inside `ProjectDef.nodes`
- shader params inside `ShaderDef.param_defs`
- output options inside `OutputDef.options`
- fixture paths inside `FixtureDef.mapping`
- bindings/mock bindings inside each node root

Loader metadata such as `kind`, `schema_version`, or future format markers may
exist in TOML/JSON without becoming slots, but that policy must be explicit.
The codec should not silently persist domain data outside a slot root.

## Phase 2: Codec Placement

Implement reusable codec code in production, but validate it through mockup
tests first.

Likely placement:

```text
lp-core/lpc-wire/src/slot/
  authored_toml.rs        # or slot_toml.rs
  slot_data_json.rs       # extend existing direct writer if needed
  slot_data_decode.rs     # shared decode core if not TOML-specific
```

or, if the module wants to be independent of wire protocol naming:

```text
lp-core/lpc-slot-codec/src/
  lib.rs
  authored_value.rs
  slot_decode.rs
  toml.rs
  json.rs
```

Do not over-extract before the mockup has a second caller. A small production
module plus mockup tests is acceptable.

## Decoder Shape

The core decode API should stay small:

```rust
pub fn decode_slot_data(
    shape: &SlotShape,
    input: AuthoredValueRef<'_>,
    registry: &SlotShapeRegistry,
) -> Result<SlotData, SlotDecodeError>;
```

`AuthoredValueRef` can initially wrap `toml::Value`/`toml::Table`. Avoid owning
a second tree until the need is proven.

The walker should support:

- `SlotShape::Ref`
- `SlotShape::Record`
- `SlotShape::Map`
- `SlotShape::Option`
- `SlotShape::Enum`
- `SlotShape::Value`
- `SlotShape::Unit`

Use explicit `SlotDecodeError` with a path stack.

JSON decode can use the same core through a JSON-backed `AuthoredValueRef`, or
can wait until TOML has settled if that keeps the slice smaller.

## Authored Conventions

Proposed first conventions:

- Records decode from TOML tables by field name.
- Missing non-optional fields use domain defaults only if the slot metadata or
  target hydration code defines one.
- Maps decode from TOML tables when keys are strings/u32/i32 and from arrays
  only if the shape explicitly chooses an array compatibility mode later.
- Enums decode from a table with `kind = "<variant>"`; the remaining fields are
  the variant payload table.
- Options decode as `None` when the field is absent and `Some` when present.
- Semantic string leaves such as paths and refs decode through their existing
  `FromLpValue`/slot leaf conversion rules, not ad hoc per-field parsing.
- `#[slot(skip)]` fields are loader metadata or runtime-only data and need an
  explicit storage policy: accept-and-ignore, validate, or reject.
- Defaults must be explicit in slot metadata, codec policy, or mock hydration
  code. Do not rely on invisible Serde behavior in the native path.

## Writer Strategy

The mockup should prove three writer modes:

- Owned native JSON/TOML from `SlotData`.
- Borrowed native JSON from `SlotDataAccess` plus `SlotShape`, especially for
  firmware-like paths.
- Serde compatibility tests that deserialize native JSON into existing
  serde-backed mirror structs where that is still useful.

Manual streaming writers should not become a separate schema. They are an
implementation strategy for the same native slot codec.

## Hydration Strategy

For M1, the mockup can hydrate typed mock defs explicitly from decoded slot
data. This can be repetitive. That is fine for the experiment.

Do not introduce a broad generic "deserialize any Rust type from SlotData"
derive until the size result justifies it. The whole point is to avoid building
another Serde-sized abstraction by accident.

If hydration starts to look promising, the later derive should generate direct
field extraction from `SlotDataAccess`/`SlotData`, not Serde visitor machinery.

## Validation

Run focused tests:

```bash
cargo test -p lpc-slot-mockup
cargo test -p lpc-wire --test source_slot_sync
```

Firmware size checks wait until a real adoption slice exists:

```bash
cd lp-fw/fw-esp32
cargo bloat \
  --target riscv32imac-unknown-none-elf \
  --profile release-esp32 \
  --features esp32c6,server \
  --crates -n 80
```

If the change touches shader/source loading used by firmware, also run the
normal firmware checks from `AGENTS.md` before considering it complete.

## Success Criteria

- Mockup TOML resembles current real authored TOML for project, shader, texture,
  output, and fixture defs.
- Mockup JSON resembles current real wire/storage JSON where applicable.
- Native codec tests cover records, maps, options, enums, skipped fields,
  defaults, semantic leaves, and direct writers.
- Tests cover missing fields, unknown fields, bad scalar types, and bad enum
  variants.
- Rough points and real-model deviations are written down.
- The codec does not add `std` requirements to source/model/wire crates.

## Stop Criteria

Stop and reassess if:

- The mockup must diverge heavily from real TOML/JSON to make the codec work.
- Hydration requires a large per-type framework before the mock model works.
- The slot metadata lacks too much authored semantics to decode cleanly.
- Manual/direct writers drift into a second schema instead of sharing native
  slot semantics.
