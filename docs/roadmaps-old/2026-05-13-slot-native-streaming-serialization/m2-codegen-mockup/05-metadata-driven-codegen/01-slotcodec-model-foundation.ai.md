# Phase 1: SlotCodec Model Foundation

## Scope Of Phase

Introduce the internal metadata model that will drive mockup codec generation.

In scope:

- Add small SlotCodec model types in `lpc-slot-codegen`.
- Add a temporary metadata table for the five mockup source roots.
- Keep existing generated output behavior unchanged.
- Add focused `lpc-slot-codegen` tests for the SlotCodec model/table shape.

Out of scope:

- Replacing root readers/writers with SlotCodec renderers.
- Production adoption.
- Derive-macro/module-local codegen.

## Code Organization Reminders

- Prefer granular files with one main concept per file if `lib.rs` becomes
  unwieldy.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-codegen/src/lib.rs`

Expected changes:

- Add SlotCodec model structs, initially private, such as:
  - `SlotCodecModule`
  - `SlotCodecRoot`
  - `SlotCodecField`
  - `SlotCodecConstructor`
- Add a function such as `mockup_source_codec_module() -> SlotCodecModule`.
- Represent at least the five source roots:
  - `ProjectDef`
  - `OutputDef`
  - `TextureDef`
  - `FixtureDef`
  - `ShaderDef`
- Include enough metadata for later phases:
  - root Rust type
  - function stem
  - discriminator/kind expression
  - default expression, if needed
  - fields
  - read expression per field
  - write expression per field
  - skip/discard policy for fields such as `bindings` and `sampling`
  - constructor expression or constructor parts

Tests:

- Add `lpc-slot-codegen` unit tests that assert the metadata table contains all
  five source roots.
- Assert representative fields exist, for example:
  - `ProjectDef` has `name` and `nodes`.
  - `OutputDef` has `pin` and `options`.
  - `FixtureDef` has `mapping`.
  - `ShaderDef` has `param_defs`.

Constraints:

- Treat this as a Serde-derive-like model for slots, not as a runtime data
  tree.
- Keep it build/codegen-time only.
- Copy Serde concepts where useful: generated per-type adapters, default
  policy, discriminator/tag handling, field lists, skip/transient policy.
- Keep the supported shape narrower than Serde: slot roots, records, enums,
  option slots, maps, value slots, `LpValue`, and known semantic leaves.
- This phase should not change generated output yet.
- Keep the current `generated_shape_codec` behavior untouched.

## Validate

```bash
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup generated_shape_codec
```
