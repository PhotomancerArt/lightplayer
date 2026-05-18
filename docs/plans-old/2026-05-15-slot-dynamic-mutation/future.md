# Future Work

## Enum Data Convenience

- **Idea:** Add `set_variant_from_slot_data` as a convenience that switches an enum variant and hydrates its payload from dynamic data.
- **Why not now:** The primitive model is two-phase: switch to default variant, then mutate active payload fields.
- **Useful context:** JSON/TOML readers should use the two primitive operations directly while streaming.

## Map Insert And Remove

- **Idea:** Add generic map key insertion/removal mutation operations.
- **Why not now:** Existing-key mutation proves the path walker without committing to construction/default policy.
- **Useful context:** `MapSlot<K, V>` already has revision-aware `insert_with_version` and `remove_with_version`.

## Option Some Construction

- **Idea:** Add `none -> some(Default::default())` mutation for option payloads.
- **Why not now:** It has the same construction-policy question as enum switching.
- **Useful context:** `OptionSlot<T>` already has `set_some_with_version` and `set_none_with_version`.

## Codec Read Body Removal

- **Idea:** Replace generated per-record `SlotCodec::read_slot` bodies with a generic object reader that mutates a default instance.
- **Why not now:** This plan is mutation-only; codec rewiring should happen after generic mutation is proven in runtime tests.
- **Useful context:** `lp-core/lpc-slot-codegen/src/render/slot_codecs.rs` is the current generated read/write body path.
