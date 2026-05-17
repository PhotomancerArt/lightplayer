# Phase 3: Remove Domain Serde

> Superseded by the measured M4 policy. The active rule is to avoid
> serde-derived firmware parsing for full slot-authored domain trees, not to
> delete every Serde derive from model types.

## Scope Of Phase

Remove Serde derives, imports, attributes, and manual impls from domain model
types whose read/write behavior is now owned by SlotCodec or `SlotValue`.

In scope:

- Authored node definitions and runtime state records.
- Semantic leaves.
- Binding/reference/product/resource model types.
- Slot containers where serde impls are only old authored convenience behavior.

Out of scope:

- Slot metadata/snapshot types covered by Phase 2.
- Serde tests covered by Phase 4 unless they must change with the code.
- Removing dependencies from `Cargo.toml`; that happens after all references are
  gone.

## Code Organization Reminders

- Prefer mechanical deletion where serde was only derive/attribute metadata.
- Keep parser/display methods that are domain behavior, such as reference string
  parsing.
- Do not add new compatibility shims for old serde casing.
- Keep semantic leaves centered on `SlotValue`, `ToLpValue`, and `FromLpValue`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Primary directories:

- `lp-core/lpc-model/src/nodes/`
- `lp-core/lpc-model/src/slots/`
- `lp-core/lpc-model/src/binding/`
- `lp-core/lpc-model/src/product/`
- `lp-core/lpc-model/src/products/`
- `lp-core/lpc-model/src/resource/`
- `lp-core/lpc-model/src/resources/`
- `lp-core/lpc-model/src/node/`
- `lp-core/lpc-model/src/value/`
- `lp-core/lpc-model/src/project/`
- `lp-core/lpc-model/src/server/`

Search command:

```bash
rg -n "serde|Serialize|Deserialize" lp-core/lpc-model/src
```

Expected changes:

- Remove `use serde::{...}` from domain files.
- Remove `Serialize`, `Deserialize` derives.
- Remove `#[serde(...)]` attributes.
- Remove hand-written serde impls for references/endpoints where equivalent
  parsing/display and SlotValue behavior remain.
- Remove serde impls from `ValueSlot`, `MapSlot`, `OptionSlot`, and `EnumSlot`
  if no non-metadata phase still needs them.

Important edge cases:

- Some enums such as `RingOrder`, shader option enums, color order, texture
  format, products, and resources should keep or grow `SlotValue`/LpValue
  codecs where needed.
- `NodeInvocation` and other small records should remain `Slotted` if they are
  authored/synced.
- If a type is not slotted and still crosses a boundary, stop and identify the
  boundary instead of inventing an ad hoc serde replacement.

## Validate

```bash
cargo fmt -p lpc-model
cargo check -p lpc-model
cargo test -p lpc-model nodes::node_def
cargo test -p lpc-model slot_codec
```
