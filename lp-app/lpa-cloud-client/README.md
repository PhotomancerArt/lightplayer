# lpa-cloud-client

The client side of the cloud: the transport port, an in-process transport,
and the sync engine. Everything below the UI.

```text
                 ┌──────────────────────────────────────────┐
   sync engine   │ publish  push  pull  apply  resolve  open│
                 └───────────────────┬──────────────────────┘
                       CloudPort     │      lpfs::LpFs + lpc-history
                 ┌───────────────────┴──────┐   (SnapshotStore, EventLog,
   transports    │ InProcessCloud │ (wasm)  │    ProjectHistory, BlobStore)
                 └──────────────────────────┘
```

The engine is written over the **library primitives**, not over
`lpa-studio-core`: a local project here is a uid, a package filesystem, and a
history root. The studio wires this up in a later round; nothing in this crate
knows what a studio is, and nothing in it draws anything.

## The caller contract

This crate has no retry loop, no scheduler, and no policy. Read this section
before wiring it into anything — the rules below are *yours* to enforce, and
they are written down here so nobody has to re-derive them from the vision
doc.

### Offline is a caller state, not a mode

Every operation is **one attempt**. An unreachable service comes back as
`SyncError::Transport(TransportError::Offline)` and nothing local changes.
Queue-and-retry belongs to whoever owns the write-behind loop. `push` is safe
to retry verbatim: it re-derives what the service is missing every time, so a
retry after three more local saves sends the right thing, and a retry after a
successful push that lost its answer sends nothing.

`InProcessCloud::go_offline()` fakes exactly this, which is how the offline
scenario is tested without a network.

### Auto-apply (D5 / D18)

`pull` **applies nothing**. It fetches, banks every service head into the
local snapshot store, and classifies. Adopting is `apply_fast_forward`, and
whether to call it is the caller's decision:

- **Clean copy** — working copy equal to its head, no uncommitted editor
  state: fast-forward it. That is the Dropbox-shaped behavior the product
  wants, toast and all, and it applies to open editors too.
- **Uncommitted edits** — do not apply. Badge instead. The user saves, and
  then it is an ordinary divergence to resolve. (This is D18, the
  dirty-session carve-out: a franken-state made of half-applied inbound
  changes over local edits is worse than a badge. A future real merge may
  dissolve the hazard; a clobber join does not.)
- **Diverged** — nothing to apply. `resolve_clobber` is the operation that
  makes a choice.

If `apply_fast_forward` *is* called over uncommitted edits, it banks them
first and reports the hash in `ApplyReport::banked_uncommitted`. Nothing is
ever lost silently; the rule above is about not surprising the user, not about
data safety.

### Banking before adopting

Every path that can overwrite the working copy snapshots it first
(`LocalProject::bank_working_copy`), so the content survives in the local
store addressed by its hash whatever happens next. `resolve_clobber` goes
further and *refuses* to run over uncommitted edits
(`SyncError::UncommittedLocalEdits`), naming the hash the banked content is
recoverable at — a join whose losing side was never a version anybody can get
back to is not a resolution.

`resolve_clobber` also refuses to adopt a version whose content the client
does not hold (`SyncError::UnbankedVersion`). Pull first; pull banks every
head, including the one you are about to choose against.

### Push is never blocked

The service accepts a push that does not continue its line, recording it as a
second head (`PushOutcome::NewHead`) rather than refusing it. Two heads is a
legal, visible, temporary state. A clobber join names both as parents, which
collapses the frontier back to one.

### Tracking copies (Q10)

`open_shared` pulls a project the local library has never seen into a fresh
copy. It is a **tracking copy, not a viewer mode**: there is no read-only UI
to build. A non-member simply has their pushes refused by the service.

Two things make it the same project rather than an import:

- the **uid is preserved** (D17), so the copy's next pull is a fast-forward
  and not a stranger's history;
- the **service's event log becomes the local history verbatim** — no origin
  event is minted. The pulled log already has the project's real origin, and
  inventing an "imported" event on top would be a permanent lie in a record
  the user cannot correct.

If the copy's owner wants their own line instead, that is a fork
(`LocalProject::fork_from`): new uid, new history, no binding.

## Where things live

| Concept | File |
|---|---|
| Transport trait, `TransportError`, the `call`/`request` helpers | `cloud_port.rs` |
| The in-process service + client + offline switch | `in_process_cloud.rs` |
| Per-project binding record (D23: per project, not per folder) | `cloud_binding.rs` |
| A local project: package fs + history root + the composed moves | `local_project.rs` |
| Share address parsing/rendering | `project_link.rs` |
| The operations | `sync/*.rs`, one per file |
| Null-waker driver for immediately-ready futures | `block_on.rs` |

### One call helper, no response matching

Every request the engine makes goes through `call(port, GetProject { uid })`,
which returns that request's own response type — the pairing comes from
`lpc_cloud_api::CloudCallSpec`, so no operation carries a `match` for an
answer it cannot get. `sync/service_calls.rs` is just the named list of the
calls this engine makes; it unwraps nothing.

A reply carrying some *other* response variant is a bug on one side of the
wire, and it is reported as one:
`SyncError::Transport(TransportError::Protocol(..))`, not a `CloudError`. A
`CloudError` is the service having considered the request and said no; a
wrong-shaped answer means the conversation did not really happen.

## Two things worth knowing about the wire

**Trees are addressed by package hash.** A tree manifest is a blob like any
other, but its address is the canonical `lph1` package hash, not the hash of
the manifest JSON that carries it — that is what makes a commit's `tree` hash
directly fetchable. It stays verifiable, because a receiver recomputes
`TreeManifest::package_hash()` and rejects a mismatch. Hence
`CloudPort::put_tree`/`get_tree` rather than a raw-bytes call the caller could
address wrongly. The HTTP transport (P07) must apply the same rule at
`PUT /b/<hash>`.

**Event logs are compared by content, not by position.** The service's log is
the interleaving of every client's line, so sequence numbers do not correspond
across sides. `push` sends the multiset difference of local events against the
service's; `apply_fast_forward` adopts the difference the other way. The
binding records `last_event_seq` for the future incremental read, but the
engine currently reads the log from 0 — `ProjectHistory` replays from the
origin, and splicing an unvalidated suffix would be trusting arithmetic over
content.

## Testing

`cargo test -p lpa-cloud-client`. Unit tests live at the bottom of each file
and run against `InProcessCloud` — the real domain logic over the real
in-memory adapters, so a test that passes here is not passing against a mock.
The flagship end-to-end scenarios live in `tests/` (P06).
