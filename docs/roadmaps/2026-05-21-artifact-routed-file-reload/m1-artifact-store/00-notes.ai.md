# M1 Plan Notes — `lpc-node-registry` + ArtifactStore

## Scope of work

Bootstrap **`lpc-node-registry`** and implement a **freshness-only
`ArtifactStore`**:

1. **Crate bootstrap** — new `no_std` + `alloc` crate; delete `lpc-slot-mockup`.
2. **Requester-owned artifacts** — entries exist because a caller **acquired**
   them; **`release`** when done. No filesystem-driven auto-registration.
3. **Freshness metadata** — per entry: path, **`revision`** (`lpc_model::Revision`),
   read outcome state, **no cached bytes**, **no `NodeDef` payload**.
4. **Fs integration** — `apply_fs_changes` bumps **`revision`** on **existing**
   acquired entries whose path matches; handles missing/deleted files without
   removing entries while refs are held.
5. **Transient read** — `read_bytes(id, fs)` for M2 parse path; does not retain
   payload.
6. **Unit tests** — M1 gate; `cargo test -p lpc-node-registry`.

**Out of scope:** `NodeDefRegistry` (M2), parsing, `SourceFileSlot`, ChangeSet,
`lpc-engine` edits.

## Current state

See prior analysis: production artifact logic in `lpc-engine/src/artifact/`
(NodeDef payloads, refcount). No `lpc-node-registry` yet. `lpc-slot-mockup` has
no Cargo dependents.

## Questions — resolved

### Confirmation batch (Q1–Q2, Q4, Q6–Q8)

| # | Answer |
|---|--------|
| Q1 | **Yes** — delete `lpc-slot-mockup` entirely |
| Q2 | **Yes** — new types in `lpc-node-registry` (parallel to engine until M6) |
| Q4 | **Yes** — `ArtifactLocation::File` only in M1 |
| Q6 | **Yes** — transient `read_bytes` in M1 |
| Q7 | **Yes** — stub `registry`, `source`, `change`, `view` modules |
| Q8 | **Yes** — `#![no_std]` + `default = ["std"]` |

### Q3, Q5, Q9 — revised by ownership model (user)

| # | Original suggestion | **Decision** |
|---|---------------------|--------------|
| Q3 | No refcount | **Refcount acquire/release** — artifacts owned by requester |
| Q5 | `content_frame` | **`revision`** field name (`lpc_model::Revision`) |
| Q9 | Delete removes entry | **Fs Delete bumps revision on held entries**; entry removed only on **release to refcount 0** |

### Q10 — artifact state (user)

**Decision:** Requester-owned entries + read outcome state.

- **`acquire`** (resolve locator → location) **always yields an entry** unless the
  locator is badly formed (e.g. unsupported `lib:`).
- Entry lifetime tied to **refcount**, not filesystem.
- Bootstrap chain (future): **engine → registry acquires `project.toml` → …**
- Missing/deleted files: entry persists while acquired; **`revision` bumps** on fs
  delete/modify; read state reflects failure without dropping identity.

```rust
enum ArtifactReadState {
    Unread,
    ReadOk,
    Failed(ArtifactReadFailure),
}

enum ArtifactReadFailure {
    Deleted,
    NotFound,
    Io { message: String },
    InvalidPath { message: String },
}
```

- **`revision` bump** on fs modify/create → `Unread` (clears prior failure).
- **`FsChange::Delete`** → bump + `Failed(Deleted)` immediately.
- **`release` at refcount 0** → remove entry from store.

## Resolved decisions (roadmap + plan)

- M1 does **not** touch `lpc-engine`.
- Metadata only — no byte retention on entries.
- Fs events affect **already-acquired** entries only.
- Field name **`revision`**, not `content_frame`.

## Notes

- User: artifacts are owned by the **requester**, not the filesystem. The node
  system asks the store; store returns an entry (unless locator resolution fails).
- M2 `NodeDefRegistry` will acquire artifacts when defs need file sources; M1
  tests simulate acquire/release directly.

## Implementation dispatch

Per `/plan` and `/implement` (updated model policy):

| Phase | Model |
|-------|-------|
| 01 Bootstrap | `composer-2-fast` |
| 02–04 | `composer-2.5-fast` |
| 05 Cleanup | `composer-2.5-fast` (supervised) |
