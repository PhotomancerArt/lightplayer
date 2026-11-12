# Phase 4: Macro And Codegen Vocabulary

## Scope Of Phase

Rename private codegen data structures and update macro behavior so root no
longer means registered shape target or codec target.

In scope:

- `lp-core/lpc-slot-codegen/src/lib.rs`
- `lp-core/lpc-slot-macros/src/attr.rs`
- `lp-core/lpc-slot-macros/src/record.rs`
- Generated code comments/docs, if any.
- Codegen and macro-adjacent unit tests.

Out of scope:

- Generated public reader/writer names.
- Runtime behavior.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
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

Suggested codegen renames:

- `StaticSlotRoot` -> `StaticRegisteredShape`
- `discover_static_slot_roots` -> `discover_static_registered_shapes`
- `has_slot_root_attr` -> `has_slot_type_attr` or similar once type-level
  `#[slot]` is supported.
- `SlotCodecRoot` -> `SlotCodecType`
- `render_slot_codec_root*` -> `render_slot_codec_type*`
- `mockup_source_codec_module().roots` -> `.types`

Macro behavior:

- Treat type-level `#[slot]` as "generate static shape and slot access."
- Treat `#[slot(root)]` as compatibility if retained.
- All slot-annotated records should implement `StaticSlotShape` unless there is
  a concrete blocker.
- Existing explicit `shape_id` overrides should continue to work.

Keep `root` in generated runtime function names only if those functions truly
represent top-level JSON/TOML object readers in the mockup. Otherwise prefer
`type` or `target` internally.

Searches:

```bash
rg -n "StaticSlotRoot|SlotCodecRoot|source_roots|slot_roots|root_types|render_slot_codec_root|discover_static_slot_roots|slot\\(root" lp-core/lpc-slot-codegen/src lp-core/lpc-slot-macros/src lp-core/lpc-slot-mockup/src lp-core/lpc-model/src
```

## Validate

```bash
cargo test -p lpc-slot-codegen
cargo test -p lpc-model slot_record
cargo test -p lpc-slot-mockup generated_shape_codec
```
