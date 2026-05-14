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

