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

#### Edit vocabulary in lpc-model

- **Decision:** Shared serde edit types live in **`lpc-model::edit`**.
- **Why:** Wire + registry need one vocabulary; wire cannot depend on registry.
- **Status:** Implemented with canonical overlay vocabulary: `ProjectOverlay`,
  `ArtifactOverlay`, `SlotOverlay`, `SlotEdit`, `SlotEditOp`,
  `ArtifactBodyEdit`, `OverlayMutation`, mutation results, commit summaries,
  and portable definition locations.

#### Edit types are still not SlotData

- **Decision:** `lpc-model::edit` is dedicated — not slot shapes / codec.
- **Why:** Preserves ChangeSet intent.
- **Supersedes:** ChangeSet “ops in registry only” for **location**; not for semantics.

#### Registry keeps runtime logic

- **Decision:** Overlay, apply, commit, `NodeDefRegistry`, `SyncResult`,
  `NodeDefView` stay in **`lpc-node-registry`**.

#### Wire addressing

- **Decision:** Authored edits use artifact path + `SlotPath`; read/UI metadata
  will bridge runtime nodes to those edit addresses during cutover.
- **Rejected for now:** Keeping `node.<id>.def` as the edit wire root.
- **Revisit when:** M1 UI parity doc — may require read metadata, not second root.

#### Artifact body edit naming

- **Decision:** New shared/wire-facing byte-level operations use
  `ArtifactBodyEdit`, not `AssetEdit`.
- **Why:** The operation can replace or delete any artifact body, including
  `.toml` definitions. "Asset" remains useful for non-def referenced files such
  as GLSL/SVG bodies.
- **Status:** Implemented. Registry-local `AssetEdit` compatibility state was
  removed; registry stores `ProjectOverlay` from `lpc-model`.

#### Overlay mutations, not registry SyncOp on wire

- **Decision:** Client-authored overlay changes use ordered
  `OverlayMutationBatch`; registry `SyncOp::Fs` stays server-local.
- **Why:** `SyncOp` mixes client edit intent with filesystem watcher events.
  Exposing it would leak registry mechanics into the wire contract.
- **Status:** Implemented for the POC with `WireOverlayRead*`,
  `WireOverlayMutation*`, and `WireOverlayCommit*` wrappers.

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
