# M3.1: State Wire Projection — Summary

### What was built

- **SyncProjection vocabulary:** Named the client sync boundary `SyncProjection`; the legacy `GetChanges` path is `LegacySyncProjection` (design/docs; not necessarily a new public trait).
- **`ProjectView`:** Applies real concrete configs from `NodeDetail` / serializable wire details; merges partial legacy node state; tests cover texture/output partial merge and config-after-`ConfigUpdated` behavior (`lpc-view` integration tests).
- **Legacy wire hardening:** `SerializableProjectResponse` round-trip and partial-state omission behavior tested; partial-state serde macros and merge semantics exercised across texture/output/shader and shared test patterns (`lpc-wire`).
- **Engine parity hooks:** Targeted `lpc-engine` tests (`partial_state_updates`, `scene_update`) continue to pass alongside projection changes.

### Decisions for future reference

#### SyncProjection names the client sync boundary

- **Decision:** Use **`SyncProjection`** for the conceptual boundary (project internal state into a client-usable sync payload given frame id and watch/detail interests). Use **`LegacySyncProjection`** when referring specifically to the M4 compatibility path that emits legacy `GetChanges`-style payloads.
- **Why:** Separates “derived view for consumers” from authoritative runtime storage; keeps one term for the idea and a second for the concrete legacy protocol.
- **Rejected alternatives:** `WireProjection` alone (too transport-centric); `StateSnapshot` alone (undersells deltas/versioning); tying the name only to `GetChanges` (too narrow for future projections).

#### Heavy byte fields remain compatibility snapshots for M4

- **Decision:** Texture pixels (`Versioned<Vec<u8>>` + width/height/format), fixture lamp colors and mapping cells, output channel bytes, and related frame/version metadata stay in legacy **`NodeState`** / JSON wire shape for M4 parity. They are **wire-visible snapshots**, not the long-term authoritative runtime product model.
- **Why:** M4 needs behavior and client sync parity before redesigning transport or storage; avoids colliding with concurrent client/M4 work by changing payload shape in M3.1.
- **Rejected alternatives:** Moving heavy blobs out of state streams in M3.1; new binary/throttled/compressed transport.
- **Revisit when:** M3.2 defines store-backed buffer/product identity and snapshot/reference/diff policy.

### M3.2 handoff inventory

Heavy fields and metadata to resolve in **M3.2** (buffer/product store, identity, wire policy):

| Domain | Payload / metadata today | Open for M3.2 |
|--------|---------------------------|---------------|
| Texture | Bytes + width + height + format (`Versioned`, base64 on wire) | Stable buffer id, eviction, whether snapshots vs refs vs diffs cross sync |
| Fixture | Lamp color bytes, mapping cells, texture/output handles | Same + relationship to fixture “products” if split from state JSON |
| Output | Channel bytes (`Versioned`) | Channel buffer identity vs inline snapshot |
| Sync framing | `current_frame`, `since_frame`, per-field version frames in partial serde | Snapshot vs delta semantics end-to-end; reference equality for unchanged blobs |
| Protocol split | `GetChanges` vs structural `WireTreeDelta` | Whether/how adapter merges projections (per M3.3 notes); not solved in M3.1 |

### Cleanup review (phase 4)

- **Scoped files:** `project_view.rs`, `client_view.rs`, `macros.rs` — no `todo!`, `dbg!`, `println!`, `#[ignore]` on tests, or new `#[allow(...)]` found.
- **`api.rs`:** Pre-existing doc **`TODO`** comments on `ProjectResponse` / `NodeDetail` / `Box<dyn NodeConfig>` serde limitations (explain why raw `ProjectResponse` is not serde’d); left as-is — historical context, not M3.1 scope creep.
- **Rustdoc:** `lpc-wire` `state::macros` module docs include a non-runnable code fence tagged `ignore` (rustdoc reports one ignored doctest); not `#[ignore]` on a library test.

### Validation (phase 4)

All passed:

- `cargo test -p lpc-view`
- `cargo test -p lpc-wire`
- `cargo test -p lpc-engine --test partial_state_updates --test scene_update`

### Deviations

- None. No code edits beyond adding this `summary.md`; no commits per phase instructions.
