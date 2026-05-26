#### Promoted from artifact-routed M6

- **Decision:** Engine switchover + prerequisites live in
  `docs/roadmaps/2026-05-21-engine-registry-cutover/`.
- **Why:** Large enough for its own roadmap.
- **Revisit when:** Unlikely.

#### M1 is API hardening, not a blind type move

- **Decision:** M1 resolves shape, UI parity, mutation inventory, and M3/M4
  sequencing **before** committing to implementation.
- **Why:** Avoid wire/registry churn; user-requested gate.
- **Revisit when:** M1 exit criteria met.

#### Edit vocabulary in lpc-model (intent)

- **Decision:** Shared serde edit types **should** live in **`lpc-model::edit`**.
- **Why:** Wire + registry need one vocabulary; wire cannot depend on registry.
- **Status:** **Pending M1 sign-off** — module layout and type list in
  `m1-api-hardening/00-design.md`.

#### Edit types are still not SlotData

- **Decision:** `lpc-model::edit` is dedicated — not slot shapes / codec.
- **Why:** Preserves ChangeSet intent.
- **Supersedes:** ChangeSet “ops in registry only” for **location**; not for semantics.

#### Registry keeps runtime logic

- **Decision:** Overlay, apply, commit, `NodeDefRegistry`, `SyncResult`,
  `NodeDefView` stay in **`lpc-node-registry`**.

#### Wire addressing

- **Decision:** TBD in M1 — **lean** artifact path + `SlotPath` for edits.
- **Rejected for now:** Keeping `node.<id>.def` as the edit wire root.
- **Revisit when:** M1 UI parity doc — may require read metadata, not second root.

#### Legacy mutation cleanup

- **Decision:** Inventory in M1; **delete** `WireSlotMutation*` path, engine
  `slot_mutation`, client pending queue in **M8** (after cutover works on new path).
- **Why:** No dual mutation APIs in production long-term.

#### M3 / M4 sequencing

- **Decision:** **Open** — options documented in M1; not preset to server-first.
- **Why:** User preference to think through; cutover not feared.

#### Session log deferred

- **Decision:** No server session log v1; path-keyed overlay (ChangeSet M8).

#### M10 provenance out of scope

- **Decision:** ExplainSlot probes stay artifact-routed M10.
