# Notes: Metadata-Driven Mockup Codec Codegen

## Scope Of Work

Replace the current hardcoded mockup slot-codec template with generated code
driven by a compact `SlotCodec` metadata model.

In scope:

- Keep the mockup fully off direct Serde derives and dependencies.
- Preserve the current generated slot-native behavior for all mockup source
  roots:
  - `ProjectDef`
  - `OutputDef`
  - `TextureDef`
  - `FixtureDef`
  - `ShaderDef`
- Introduce a `SlotCodec` or equivalent internal model in `lpc-slot-codegen`.
- Render source-root readers/writers from that model instead of copying one
  large handwritten generated-code blob.
- Keep generated code compact and helper-driven.
- Retain existing `generated_shape_codec` tests as the acceptance contract.
- Treat Serde as the conceptual template: copy proven architecture where it
  fits, then project it onto slots, streaming readers, and embedded code-size
  constraints.

Out of scope:

- Production adoption in `lpc-wire`, `lpc-source`, or loaders.
- Removing Serde from `lpc-model` or other crates.
- Solving derive-macro/module-local generation for privacy in this milestone.
- Generating dynamic runtime/node message codecs.

## Current State

The mockup is now slot-native at the source-model layer:

- `lpc-slot-mockup` no longer has direct `serde` or `serde_json`
  dependencies.
- Mockup source types no longer derive `serde::Serialize` or
  `serde::Deserialize`.
- Generated slot-native codec coverage exists for every mockup source root.
- `cargo test -p lpc-slot-mockup` passes.

The custom codec foundation lives in `lpc-model::slot_codec`:

- `JsonSyntaxSource`
- `TomlSyntaxSource`
- `SlotReader`
- `ValueReader`
- `SlotJsonWriter`
- shared helper methods including:
  - `ValueReader::string_key_map`
  - `ValueReader::u32_key_map`
  - `ValueReader::f32_array`
  - `ValueReader::i32`
  - `SlotJsonValue::string_key_map`
  - `SlotJsonValue::u32_key_map`
  - `SlotJsonValue::f32_array`
  - `SlotJsonValue::i32`

The problem is that `lpc-slot-codegen` still emits most codec code from
hardcoded raw-string sections:

- `MOCKUP_SLOT_CODEC_IMPORTS_AND_TYPES`
- `MOCKUP_SLOT_CODEC_BUNDLE_READERS`
- `MOCKUP_SLOT_CODEC_REAL_PROJECT_DEF`
- `MOCKUP_SLOT_CODEC_WRITERS`

Those sections are easier to replace than the previous single blob, but they
are still not metadata-driven.

Relevant files:

- `lp-core/lpc-slot-codegen/src/lib.rs`
- `lp-core/lpc-slot-mockup/build.rs`
- `lp-core/lpc-slot-mockup/src/lib.rs`
- `lp-core/lpc-slot-mockup/src/source/*.rs`
- `lp-core/lpc-slot-mockup/src/tests/generated_shape_codec.rs`
- `lp-core/lpc-model/src/slot_codec/*.rs`

## User Notes

- `SlotCodec` is the preferred name: short, catchy, and easy to type.
- This is a LightPlayer-shaped projection of Serde. We are building it because
  embedded code size and slot-native modeling require it, not because custom
  serialization is desirable for its own sake.
- Prefer battle-tested Rust crate concepts where they fit, as with Naga and
  Cranelift elsewhere in LightPlayer.
- The goal is to get the mockup fully working on the new slot-native model.
- Direct Serde should be gone from the mockup.
- Continue unless there is a serious/blocking design issue.
- Embedded code size remains a leading motivation, so generated code should
  lean on shared helpers rather than per-type bespoke loops where possible.
- The system can be highly opinionated. It only needs to serialize the slot
  model well; general-purpose Rust serialization is not a requirement.
- Before production adoption, take size metrics and do a
  minimize-the-monomorphs pass over slot-related generated code and helpers.
- The mockup should keep exercising the same basic shape as the real domain:
  node defs, invocations, values, maps, enums/discriminators, TOML disk shape,
  and JSON wire shape.

## Open Questions

### Q0. How closely should SlotCodec follow Serde?

Suggested answer: follow Serde's architecture and naming instincts where they
reduce invention, but keep the supported model much narrower.

Context:

- Serde's useful split is:
  - format parser/writer
  - serializer/deserializer facade
  - generated per-type impl
  - derive-time model of fields, defaults, tags, and skips
- SlotCodec should project that into:
  - `JsonSyntaxSource` / `TomlSyntaxSource`
  - `SlotReader`
  - `SlotJsonWriter`
  - generated slot-root readers/writers
  - `SlotRecord` / `SlotEnum` metadata as the schema language
- Do not clone Serde's full generic Rust data model. Support the LightPlayer
  domain shape: slot roots, slot records, slot enums, value slots, option
  slots, maps, `LpValue`, and known semantic leaves.

### Q1. Should metadata generation start from the existing slot shape registry or source AST?

Suggested answer: start from source AST plus a small explicit codec hook table.

Context:

- `lpc-slot-codegen` already parses source files with `syn` to discover
  `SlotRecord` roots and slot views.
- The runtime `SlotShapeRegistry` knows field names, records, maps, options,
  and enum variants, but it does not know Rust construction hooks such as
  `OutputDef::from_codec(...)` or private-field accessors.
- Source AST can recover field identifiers and attributes without requiring
  runtime shape construction in the build script.

### Q2. How should private fields be handled for this plan?

Suggested answer: keep using explicit codec hooks/accessors in the mockup and
represent those hooks in SlotCodec metadata.

Context:

- Current generated code needs methods like `OutputDef::from_codec`,
  `TextureDef::size`, `FixtureDef::mapping`, and `ShaderDef::from_codec`.
- Making every field public would make serialization leak into the domain
  model.
- The longer-term answer is probably derive-emitted or module-local codec impls,
  but this plan should avoid making that architecture decision yet.

### Q3. Should this plan attempt to generate every helper function from slot metadata?

Suggested answer: no. Generate root record adapters first, and keep specialized
  leaf/enum helpers explicit until the common patterns settle.

Context:

- `Dim2u`, `Affine2d`, `ColorOrderValue`, `RingOrder`, `GlslOpts`,
  `MappingConfig`, and `PathSpec` need semantics beyond basic record field
  loops.
- Replacing everything at once would make the plan too broad and risk obscuring
  the useful part: SlotCodec-driven root read/write generation.

### Q4. What counts as success?

Suggested answer:

- The current mockup tests still pass.
- Direct Serde remains absent from `lpc-slot-mockup`.
- The hardcoded root-specific reader/writer code is replaced by renderers over
  SlotCodec metadata for the source roots.
- Any remaining explicit generated text is limited to test fixture types,
  specialized leaf/enum helpers, and a small temporary metadata table.
