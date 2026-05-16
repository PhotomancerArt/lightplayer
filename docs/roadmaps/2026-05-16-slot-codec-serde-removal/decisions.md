# Slot Codec Serde Removal Decisions

#### Switch Behavior Before Removing Serde

- **Decision:** Keep serde derives and helpers while switching real call paths
  to SlotCodec.
- **Why:** This lowers migration risk and makes serde removal a final proof
  instead of the first breaking change.
- **Rejected alternatives:** Delete serde derives first; rewrite all types in
  one pass.

#### EnumSlot Is The Structured Enum Field Pattern

- **Decision:** Structured slot enum fields use `EnumSlot<T>`.
- **Why:** Active variant selection needs a revision boundary, and raw Rust enum
  fields cannot carry that boundary honestly.
- **Rejected alternatives:** `#[slot(enum)]` raw fields; embedding revision
  fields inside every enum variant.

#### Start Real Migration With Messages

- **Decision:** After cleanup, switch a JSON/message path before authored defs.
- **Why:** It proves the wire side and should be a smaller first production
  slice than project/artifact TOML loading.
- **Rejected alternatives:** Start with definitions first; wait until all serde
  derives are removed.

#### Remove Serde Last

- **Decision:** Drop `serde` / `serde_json` from `lpc-model` only after message
  and definition paths use SlotCodec.
- **Why:** Remaining derives are harmless during migration, and keeping them
  avoids mixing behavior changes with dependency cleanup.
- **Rejected alternatives:** Treat dependency removal as the first milestone.
