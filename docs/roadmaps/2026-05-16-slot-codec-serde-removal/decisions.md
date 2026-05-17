# SlotCodec Domain Serialization Decisions

#### Switch Behavior Before Removing Serde-Derived Domain Paths

- **Decision:** Keep serde derives and helpers while switching real call paths
  to SlotCodec.
- **Why:** This lowers migration risk and makes removal of expensive
  serde-derived domain behavior a final proof instead of the first breaking
  change.
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

#### Keep Serde Unless Measurement Says Otherwise

- **Decision:** Do not drop `serde` / `serde_json` from `lpc-model` as a goal
  by itself. Keep Serde available where it is convenient and cheap, while
  ensuring firmware-facing slot-authored domain data uses SlotCodec.
- **Why:** Firmware measurement after merging `main` showed the SlotCodec path
  reduced `lpc_model` size while `serde_core` stayed a modest flat cost.
  Removing Serde wholesale would force us to reimplement useful non-slot
  protocol/tooling behavior without a measured payoff.
- **Rejected alternatives:** Treat dependency removal as the first milestone.

#### Measure Bloat At Firmware Boundaries

- **Decision:** Use `fw-esp32` release builds and `cargo bloat` as the primary
  acceptance check for serialization-size decisions.
- **Why:** Source-level codegen size is only a proxy. The firmware link result
  is what matters for embedded flash pressure.
- **Commands:** See
  `docs/reports/2026-05-17-slotcodec-bloat-check.md`.

#### Retire Source Crate, Keep Wire Crate

- **Decision:** Retire `lpc-source`; keep `lpc-wire` for protocol envelopes and
  slot sync/mutation payloads.
- **Why:** Authored source definitions now live in `lpc-model` as slot-native
  domain data. `lpc-wire` still owns client/server protocol vocabulary and can
  use Serde for small shells while slot payloads remain slot-shaped.
- **Rejected alternatives:** Keep `lpc-source` as a compatibility layer; delete
  `lpc-wire` before replacing active client/server protocol paths.
