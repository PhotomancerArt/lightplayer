### What was built

- Added `lpc-source` for authored/on-disk source model types and moved artifacts, source bindings, source shapes, source node config, value specs, presentation, schema, and TOML helpers into it.
- Added `lpc-wire` for view wire types and moved messages, project wire requests/handles, tree deltas/views, transport/json helpers, server wire shapes, and legacy partial state serialization into it.
- Slimmed `lpc-model` to shared concepts and added `WireValue` / `WireType` while removing `lps-shared` from the model crate.
- Added `lpc-engine` conversion boundaries for `LpsValueF32 -> WireValue` and `WireType -> LpsType`, plus `RuntimePropAccess`.
- Added `lp-view::WirePropAccess` and updated client/server/firmware/legacy/visual dependents to import the split crates.
- Updated crate READMEs and active M4.3/M4.4 roadmap docs to use the new crate vocabulary.

### Decisions for future reference

#### Source crate naming

- **Decision:** Use `lpc-source` for authored/on-disk source model types, with `Src*` names where roles are ambiguous.
- **Why:** `artifact` is already a domain concept, and `document` risks sounding like documentation; source matches the authored-program model.
- **Rejected alternatives:** `lpc-artifact` (overloads artifact); `lpc-document` / `lpc-doc` (documentation ambiguity); `lpc-storage` (sounds like database/IO).

#### Wire crate naming

- **Decision:** Use `lpc-wire` for view wire contract types, with `Wire*` names.
- **Why:** The crate is the wire surface of `lp-core`, and the shorter name is clearer than protocol for this layer.
- **Rejected alternatives:** `lpc-protocol` (too broad/formal); `lp-wire` (wrong crate family prefix).

#### Runtime value boundary

- **Decision:** Keep `WireValue` / `WireType` in `lpc-model`; convert to and from `LpsValueF32` / `LpsType` only in `lpc-engine`.
- **Why:** Clients and source/wire crates should not depend on shader/runtime value types, while the engine legitimately owns runtime ABI conversion.
- **Rejected alternatives:** Teach `LpsValueF32` serde and use it everywhere (leaks shader runtime types); keep `Kind::storage() -> LpsType` in `lpc-model` (keeps the dependency direction wrong).

#### Runtime vs client prop access

- **Decision:** Split produced-property iteration into `RuntimePropAccess` in `lpc-engine` and `WirePropAccess` in `lp-view`.
- **Why:** Runtime reflection sees `LpsValueF32`; client views see `WireValue` cached from wire updates.
- **Rejected alternatives:** One shared `PropAccess` in `lpc-model` (ambiguous and value-type confused); wire-facing `PropAccess` only (would force runtime to convert before sync needs it).

#### Source value spec split

- **Decision:** Promote the old private value wire mirror into `WireValue`, then split source value-spec code into focused files.
- **Why:** The existing serde shape was useful, but the large `value_spec.rs` mixed value specs, wire mirrors, texture recipes, TOML parsing, and tests.
- **Rejected alternatives:** Duplicate a second wire value enum (two representations to maintain); keep the large file (contrary to the crate-boundary cleanup goal).
