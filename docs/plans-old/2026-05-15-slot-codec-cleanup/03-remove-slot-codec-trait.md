# Phase 3: Remove SlotCodec Trait

## Scope Of Phase

Delete the old `SlotCodec` trait and remaining impls/call sites.

Out of scope: changing slot reader/writer primitives or dynamic reader/writer.

## Code Organization Reminders

- Keep `slot_value_codec.rs`; it is the new leaf helper.
- Remove only the trait-based object serialization path.
- Do not recreate a hidden replacement trait.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report.

## Implementation Details

Remove:

- `lp-core/lpc-model/src/slot_codec/slot_codec.rs`
- `pub use slot_codec::SlotCodec`
- impls for `LpValue`, `BindingDef`, `BindingDefs`, `BindingEndpoint`,
  mockup `NodeDef`, mockup `MappingConfig`, and mockup `PathSpec`

Keep serde impls and slot shape/access impls.

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-slot-mockup dynamic_slot_codec
```
