# Phase 2: Remove SlotCodec Codegen

## Scope Of Phase

Remove generated static record codec machinery from `lpc-slot-codegen` and the
mockup build.

Out of scope: deleting the `SlotCodec` trait itself.

## Code Organization Reminders

- Keep shape and view generation intact.
- Remove codec-specific config/render/tests cleanly.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report.

## Implementation Details

Remove:

- `SlotCodecCodegenConfig`
- `generate_slot_codecs`
- `render/slot_codecs.rs`
- tests asserting generated `SlotCodec` output
- mockup `build.rs` generation of `generated_slot_codec.rs`
- mockup `generated_slot_codec` module include

Update:

- `lp-core/lpc-slot-codegen/src/config.rs`
- `lp-core/lpc-slot-codegen/src/lib.rs`
- `lp-core/lpc-slot-codegen/src/render/mod.rs`
- `lp-core/lpc-slot-mockup/build.rs`
- `lp-core/lpc-slot-mockup/src/lib.rs`

## Validate

```bash
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup dynamic_slot_codec
```
