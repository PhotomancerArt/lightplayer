#### Streaming Construction Is Foundational

- **Decision:** Design around syntax streams and generated construction from
  the start.
- **Why:** Embedded RAM pressure makes mandatory `JSON -> SlotData -> object`
  too costly for large messages.
- **Rejected alternatives:** Treat direct streaming as a late optimization;
  make `SlotData` the only decode target.
- **Revisit when:** Measurements show direct construction is not needed for any
  production wire path.

#### Syntax Sources Are Shape-Agnostic

- **Decision:** JSON/TOML sources emit syntax-level objects, props, arrays,
  scalars, string chunks, and nulls.
- **Why:** Parsers should not need target slot knowledge; slot semantics belong
  in the reader/writer layer.
- **Rejected alternatives:** Shape-aware parsers; type-specific codec branches.

#### TOML Can Be Tree-Backed First

- **Decision:** TOML may parse to `toml::Value` and adapt into the shared
  reader API.
- **Why:** Authored TOML is small and TOML streaming is awkward; shared
  semantics matter more than avoiding this allocation.
- **Rejected alternatives:** Implement true streaming TOML before proving the
  model.
- **Revisit when:** Authored TOML size or parser code size becomes a measured
  problem.

#### JSON Should Prove Direct Streaming

- **Decision:** JSON should get a direct event/reader path early.
- **Why:** Wire messages may be larger and may carry resources where peak RAM
  matters.
- **Rejected alternatives:** Always parse JSON into an owned tree or `SlotData`.

#### SlotData Is Reference, Not Runtime Mandate

- **Decision:** Keep `SlotData` as a reference/test/tooling path but do not
  require it for production construction.
- **Why:** It is generic and useful, but can create extra buffering on embedded.
- **Rejected alternatives:** Remove `SlotData`; force all decode through
  `SlotData`.

#### Mockup Before Production Adoption

- **Decision:** Prove generated serialization in the mockup before broad real
  loader/message adoption.
- **Why:** The architecture is still forming and the mockup can exercise the
  same concepts with lower blast radius.
- **Rejected alternatives:** Migrate production domain first.

#### Serde Leaves Core Last

- **Decision:** Remove Serde from no-std core only after slot-native loading and
  messages are working.
- **Why:** Serde is still useful as an oracle and host convenience during
  migration.
- **Rejected alternatives:** Remove Serde before replacement behavior is
  proven.

#### SlotCodec Is Opinionated And Slot-Only

- **Decision:** Design SlotCodec for LightPlayer slot roots, records, enums,
  maps, options, values, and semantic leaves rather than as a generic Rust
  serialization framework.
- **Why:** The narrower model is the point: it should produce smaller code,
  clearer storage rules, and less duplicated domain language than Serde.
- **Rejected alternatives:** Clone Serde's full generic data model; support
  arbitrary Rust types as a primary goal.
- **Revisit when:** A non-slot use case appears that can reuse the same shape
  without broadening the embedded runtime surface.

#### Code Size Is An Acceptance Criterion

- **Decision:** Track generated source size and embedded binary size before and
  after major SlotCodec adoption steps, and plan a minimize-the-monomorphs pass
  before production rollout.
- **Why:** Reducing Serde-generated embedded code size is a leading motivation,
  so replacement code must be measured instead of assumed smaller.
- **Rejected alternatives:** Treat generated-code compactness as a final polish
  task.
- **Revisit when:** Measurements show generated SlotCodec code is not a
  meaningful contributor to firmware size.
