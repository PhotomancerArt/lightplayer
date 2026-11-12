# SlotCodec Codegen From Slots Notes

## Scope Of Work

Make the mockup prove the desired authoring model:

```rust
use lpc_model::SlotRecord;

#[derive(SlotRecord)]
pub struct OutputDef {
    pub pin: ValueSlot<u32>,
    pub options: OptionSlot<OutputDriverOptionsConfig>,
}
```

From that single slot-authored source, the system should provide shape
registration, views, JSON writing, TOML reading, field validation, default
handling, and round-trip tests.

The plan is specifically about finishing the mockup and SlotCodec generator so
normal record fields are discovered from Rust slot declarations. It is not yet
about replacing production loading or removing Serde from `lpc-model`.

## Current State

### What Is Already Slot-Driven

- `lpc-slot-codegen` walks crate source files and discovers every struct with
  `#[derive(SlotRecord)]`.
- Shape bootstrap generation is source-driven:
  - `generate_slot_shapes`
  - `discover_static_registered_shapes`
  - `render_slot_shapes`
- View generation is source-driven:
  - `generate_slot_views`
  - `discover_static_slot_views`
  - `render_slot_views`
- `SlotRecord` derive emits:
  - `SlotRecordShape`
  - `SlotRecordAccess`
  - `SlotMapValueAccess`
  - `FieldSlot`
  - `SlotAccess`
  - `StaticSlotShape`
  - `StaticSlotAccess`
- Type-level `#[slot]`, `#[slot(root)]`, and `#[slot(view)]` are no longer
  needed in normal use.

### What Still Violates The Desired Model

`lpc-slot-codegen/src/lib.rs` still contains a hand-authored
`mockup_source_codec_module()` table. That table describes domain types and
fields manually:

- `ProjectDef`
- `OutputDef`
- `TextureDef`
- `FixtureDef`
- `ShaderDef`

For each type it repeats facts that already exist in the slot-authored Rust
source:

- type name
- field names
- skipped fields
- default expressions
- read expressions
- write expressions
- constructors
- map/option handling

This table was useful as scaffolding to prove the generated-code shape, but it
now defeats the point of making slots the source of truth.

### Existing Codec Runtime Pieces

`lpc-model/src/slot_codec` already provides the core runtime shape:

- syntax event sources:
  - JSON source
  - TOML source
- `SlotReader`
- object/property/value readers
- JSON writer helpers
- common syntax errors and field/discriminator diagnostics

The mockup tests already exercise:

- generated JSON/TOML codec functions
- direct JSON writing
- TOML loading through the same reader shape
- unknown-field diagnostics
- invalid discriminator diagnostics
- required-field diagnostics
- real-shaped mockup source types

### Mockup Source Shape

The mockup crate has source-domain records under `lpc-slot-mockup/src/source`:

- `ProjectDef`
- `NodeInvocationDef`
- `OutputDef`
- `OutputDriverOptionsConfig`
- `TextureDef`
- `FixtureDef`
- `ShaderDef`
- `ShaderParamDef`
- `ScalarHint`

It also has important not-yet-derived/custom areas:

- `MappingConfig`
- `PathSpec`
- `BindingDefs`
- `BindingEndpoint`
- `GlslOpts` and enum-like mode values
- semantic leaves such as `Dim2uSlot`, `Affine2dSlot`, `ColorOrderSlot`,
  `RenderOrderSlot`, `SourcePathSlot`, `PositiveF32Slot`, `RatioSlot`

These should not be described by a hidden list of records. If they need
temporary custom behavior, the behavior should be a clearly named policy/hook
for a semantic type or field kind that the discovered model references.

## Why The Static Table Happened

The table happened as M2 scaffolding: it let the experiment prove the rendered
reader/writer function shape before the discovery model was mature. That was
reasonable for one vertical slice, but it should now be removed. Keeping it
would mean adding a second domain modeling language inside the code generator.

## User Notes To Preserve

- The whole point is: author types with slots; everything else works.
- Keep it simple. This is not Serde. SlotCodec is a way to model basic data
  objects so they can be serialized, deserialized, synced, reflected, and
  edited.
- It is acceptable, and probably desirable, to force slot-modeled records to be
  simple data objects. The rest of LightPlayer already has plenty of
  complexity.
- No hidden hand-coded lists of domain records or fields.
- The plan should be driven from code, not from a parallel static table.
- Generated code should be mindful of binary size. Prefer shared helpers and
  compact metadata over large bespoke parsers.
- The mockup should become fully smooth before adopting this in production.
- If slots cannot express a needed serialization decision, add that metadata to
  slots or make the gap explicit as a temporary policy.
- Slot fields should be public for generated machinery. Avoid private slot
  fields and constructor gymnastics in the generated path.
- Complex model needs should use one of two patterns:
  - put the slot-modeled data in a field and delegate slot operations to it
  - write a fully custom impl for truly custom behavior

## Open Questions

### Q1. Should `kind` fields remain explicit skipped Rust fields?

Current mockup source records store `kind: String` and mark it `#[slot(skip)]`.
The codec treats `kind` as a discriminator, not a real field.

Suggested answer: Keep this for the mockup only while codegen proves the
pipeline, but make discriminator policy explicit in codec metadata derived from
either an inherent `KIND` constant convention or a future slot attribute. The
field itself should not be part of the persisted slot record long-term.

### Q2. How should constructors be discovered?

Current mockup types use private fields and constructor helpers such as
`from_codec(...)`, `Default`, and `new()`.

Answer: Do not make constructor discovery clever. Generated SlotCodec targets
basic slot data records. Slot fields should be public so generated code can
construct, read, write, get, and set them directly. If a model needs private
state or complicated invariants, it should either delegate to a public
slot-data field or provide a fully custom slot/codec implementation.

### Q3. How should defaults be encoded?

Current generated readers seed local variables from hand-written expressions
such as `OutputDef::default()` and then replace fields that appear.

Suggested answer: Use `Default` or a generated default instance where available.
For fields that are required because no default exists, generated code should
track presence and produce `missing required field` errors. The discovered
model should know whether a type has a default through an explicit policy in
the mockup phase, and later through an attribute/convention.

### Q4. What custom policies are acceptable?

The user does not want hidden hand-coded domain field lists. Some semantic
values still need custom syntax.

Answer: Custom serialization code is acceptable when it is primitive-focused,
explicit, and easy to discover. The bad thing is not custom code by itself; the
bad thing is a hidden record/field table that becomes a second schema language.

Acceptable custom handlers should be organized in an obvious place, likely one
of these depending on whether the slot system remains inside `lpc-model` or is
split out:

- `lpc_model/src/codec` for the main serialization framework
- `lpc_model/src/codecs` for individual primitive/semantic handlers
- or, if extracted, `lpc_slot/src/codec` and `lpc_slot/src/codecs`

Permit custom policies only for primitive/semantic type families or slot field
kinds, for example:

- primitive `ValueSlot<T>` scalars
- `OptionSlot<T>`
- `MapSlot<K, V>`
- semantic leaves such as `Dim2uSlot` and `Affine2dSlot`
- explicit slot enums such as `MappingConfig`
- compact single-value enum syntax for `BindingEndpoint`, if enabled

Do not permit a table that says "`FixtureDef` has fields A, B, C."

### Q8. Should `#[slot(skip)]` remain?

Answer: Erase `#[slot(skip)]` for now. It makes the model unclear: if a field is
in a slot record, it should participate in the slot shape. If a value should
not be persisted, that is a different explicit concept such as future
`#[slot(transient)]`. If a value is only a serialization discriminator, it
should not be modeled as a normal field.

### Q7. Should the slot system be extracted to `lpc-slot` now?

The user raised this as a likely cleanup direction: the slot system has grown
into its own product vocabulary and may deserve its own crate.

Current dependency context:

- `lpc-model/src/slot`, `lpc-model/src/slot_codec`, and many files under
  `lpc-model/src/slots` are mostly slot infrastructure.
- `lpc-model/src/slot/value_ref.rs` references `crate::value::value_path`, so
  some code is still coupled to the broader model crate.
- Some semantic slot leaves under `lpc-model/src/slots` are domain-coupled
  (`ResourceRef`, products, relative node refs, artifact paths, source paths).
- `lpc-slot-macros` currently generates paths through `::lpc_model::...`.
- `lpc-slot-codegen` currently renders generated code through `::lpc_model::...`.

Suggested answer: Include crate extraction in the plan, but keep it staged and
mechanical. First extract only generic slot infrastructure and codec runtime
into `lpc-slot`, then keep domain-specific semantic leaves and handlers in
`lpc-model` until they are clearly generic. `lpc-model` should re-export
`lpc-slot` during migration so call sites do not churn all at once.

Final answer for this plan: Do not extract `lpc-slot` or split `lpc-domain`
now. The model/domain/slot/value/product concepts are interlinked enough that
the split would become its own project. For this plan, keep the current crate
layout and focus on getting the mockup fully smooth. It is acceptable to drag
the current domain objects along while proving the SlotCodec generator. Do not
spend this plan's complexity budget on workspace-wide import churn.

### Q9. How broad should validation be?

Answer: Keep validation focused on the mockup/codegen/model crates needed for
the experiment. Do not worry about tests everywhere else while the generator
contract is still moving.

Expected validation for this plan:

- `cargo fmt`
- `cargo test -p lpc-slot-codegen`
- `cargo test -p lpc-slot-mockup`
- `cargo check -p lpc-model --no-default-features`
- `cargo check -p lpc-wire --no-default-features`

Broader engine/firmware validation can happen later when production adoption
begins.

### Q5. Should the plan include moving codegen into smaller files?

`lpc-slot-codegen/src/lib.rs` is very large and now mixes shape discovery,
view rendering, codec scaffolding, helpers, and tests.

Suggested answer: Yes. This work should first introduce a shared discovered
record model in separate files, then move codec generation toward that model.
That gives the future codegen work a cleaner place to live.

### Q6. Should manual mockup tests remain?

`manual_shape_codec.rs` still has fully hand-written manual domain structs and
reader/writer functions.

Suggested answer: Keep it only as a reference while generated coverage is being
completed. The final cleanup phase should delete or quarantine manual tests
that no longer teach anything the generated mockup tests do not cover.
