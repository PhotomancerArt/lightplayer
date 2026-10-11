# A push is one transaction: the fs batch and deflated writes

- Status: accepted
- Date: 2026-10-10
- Plan: `lp2025/2026-10-08-2339-wire-push-boundary-and-deflate` (M6 of
  `lp2025/2026-10-08-1017-tree-store-device-round`), PR #1130
- Wire: `WIRE_PROTO_VERSION` 42
- Related: `2026-07-06-sans-io-core`, `2026-07-14-wire-hello-versioning`,
  `2026-07-28-esp32c6-flash-budget` (ledger row of this date),
  `2026-10-07-project-loads-are-tried-and-recovered`,
  `docs/defects/2026-10-10-lp-fs-wrappers-silently-take-trait-defaults.md`

## Context

The tree store (`lp-base/lp-tree-store`) is a candidate filesystem for the
C6. It already had a transaction (`LpFs::begin_batch`/`commit_batch`/
`abort_batch`, no-op defaults) and a deflated-chunk entry point
(`TreeStore::put_chunk_deflated`), but the wire reached neither: a push was a
run of independent writes, and Studio's push wrote a second copy of the
project (`demo` ↔ `demo-b`) so a refused one left the old whole.

Every fs request was stateless (`lpa-server/src/file_sync.rs`, the pull-model
rule). The store's transaction lives between requests and is store-global
(chroot views share one store), so whatever the wire shape, the server holds
state while a push is open.

## Decision

1. **Batch verbs, server-held state, one owner.** `FsRequest::BeginBatch`,
   `CommitBatch`, `AbortBatch`, answered by `FsResponse::Batch { op, atomic,
   error }`. At most one batch is open per server, owned by the link that
   began it (`lpa-server/src/batch_state.rs`). This is the one recorded
   exception to the pull-model rule; a write inside a batch is the same
   request as one outside it.
2. **What ends it.** The owner's commit (the only ending that lands it) or
   abort; the owner link closing; the owner's session resetting or coming up
   again (new `ServerTransport::take_reset_links`, reported by the USB, UART
   and radio-mux transports from lp-link's `Up`/`Reset`); a second
   `BeginBatch` from the owner (a client that lost the answer starts again);
   **60 s with no request from the owner, on the frame clock, with handling
   time excluded** — the delta of the tick after any handled request is that
   request's own time (a `LoadProject` that compiles for 20 s cannot expire
   it). After the timeout the owner's mutations are refused, naming why,
   until it begins or aborts again, so a slow client never writes outside the
   batch it thinks it holds. A failed commit is dropped, never left open.
3. **The rest of the board.** While a batch is open, another link's fs
   mutations and batch verbs are refused ("batch busy"); its reads and
   everything off the fs wire go on. The server's own writes (a loaded
   project's saves, the startup choice) join the batch and land or drop with
   it. A batch ended by anything but a commit restores the tree store's
   change log too, so a pull after an aborted push hears nothing of it.
4. **`atomic` in the answer, not the hello.** `LpFs::batches_are_atomic`
   (default `false`; `true` on `LpFsTree`, forwarded by every wrapper) says
   what the filesystem under *this* request does. On `false` the server opens
   nothing and the client runs the two-slot push. No `LpFeature`, no hello
   field.
5. **Commit after load and hash.** The one-slot push is `StopAll →
   BeginBatch → DeleteDir → writes → LoadProject → HashPackage →
   CommitBatch`. Any error, refusal, mismatch or timeout aborts (best effort:
   the request that failed may never have arrived) and the old project, whole
   in the same folder, is loaded again. A board that resets mid-load comes
   back on the old committed state. No space beside the old project, even in
   the batch (the old records stay reachable until the root is written),
   removes the old project in a batch of its own first.
6. **The deflated write.** `FsRequest::WriteChunkDeflated { path, offset,
   logicalLen, data }`: raw deflate (RFC 1951), always base64, at **logical**
   offsets with `WriteChunk`'s rules, `logicalLen` ≤ 4 KiB checked before
   anything is allocated, no content id (the board hashes; `HashPackage` over
   logical content and the link CRC are the end-to-end proof), answered by the
   existing `FsResponse::WriteChunk`. It reaches every backend through one
   trait method, `LpFs::write_deflated_chunk`: the default checks the offset,
   inflates into a ≤ 4 KiB buffer (`lp-deflate`) and writes the plain bytes;
   the tree store keeps the deflated bytes (`put_chunk_deflated`). `Write`
   and `WriteChunk` are unchanged.
7. **Chunks shrink to fit a record.** The client plans with
   `lp_tree_store::plan_deflated_chunks` (hasher-free, `miniz_oxide` level 10)
   for a constant record hint, `lpc_wire::budget::FILE_SYNC_RECORD_MAX_HINT`
   (1,024 B): each chunk ≤ 4 KiB logical whose deflate fits one record, so a
   wire chunk is one stored chunk and the cut is a function of the bytes. A
   board with other records still stores correctly, only without at-rest
   compression. A file under 64 B, or one that does not shrink by a tenth,
   goes plain.
8. **One push primitive.** `lpa_client::push_files::deploy_files` is the one
   conversation every project write runs (Studio's device push and library
   open, the in-browser sim, `lp-cli upload`/`dev`, the preview and docs
   hosts), over `LpClient` or `TokioLpClient`. Studio's device push is two
   named conversations, `device_push_one_slot` and `device_push_two_slot`;
   the adoption round deletes the second. `device_stamp`'s journaled
   `/hardware.json` writes stay raw `WriteChunk`s.

## Consequences

- **What the boundary buys** is atomicity of the whole push, no second
  directory, no old-slot cleanup and no destructive "remove the old project to
  make room" step on the common path. It does **not** halve the flash peak:
  on the tree store the two-slot push already deduped unchanged content, and
  inside a batch the old tree stays reachable until the commit, so a push of
  changed content peaks at old + changed either way. (A deflated chunk
  dedupes against another deflated push of the same bytes, not against a
  plain write of them: their boundaries differ.)
- **Measured** (host, simulator; `lpa-server/tests/push_boundary.rs`): on the
  repo's corpus (687 files, 587,289 B) requests 721 → 764 (+6 %), **wire bytes
  60.7 % of raw** (a 39 % saving, below the ≈45 % estimated: base64 on
  deflated payloads and the envelope), **0.41x at rest** on the tree store.
  Fixed 4 KiB chunks would save 2 points more on the wire and keep 13.5
  points more plain at rest. Deflate's main value is at-rest flash; the wire
  gain is modest. Planning the catalog's largest project (73 KB) takes
  12.6 ms in wasm (bun/JSC), 4.3 ms native.
- **Cut sweeps** of one server-driven push (the real server and client over a
  loopback, `NorFlashSim`, every named tear model, calibrated included): 198
  cuts of `projects/test/basic` and, exhaustively, 1,989 cuts of the 73 KB
  `playful-choker-tryout`: every one old or new, never a mix.
- **C6 image**: core +560 B, engine +6,588 B, gated headroom 82,202 →
  75,614 B (the flash budget's ledger). Every board's image carries the batch
  and the inflate, littlefs boards included.
- **Wire 42**: fielded boards at 41 read as older firmware until updated over
  the update channel. No never-break pin moved (lp-link's goldens, channel 3,
  the relay's frames, the access and network files, `FORMAT.md`).
- A server-held batch makes the server's behaviour depend on link lifecycle:
  a transport that cannot report resets (`fw-emu`'s `M!`, the host's
  single-link WebSocket) relies on the idle timeout. Those run on
  non-atomic backends today, so no batch opens there.
- A pull taken by another link *during* a batch sees the batch's own writes
  (the store's reads do); after an abort the change log is restored but such
  a puller is not told again.

## Alternatives Considered

- **A batch id on every write** instead of verbs: looks stateless but is not
  (the store's transaction is store-global and lives between requests), adds
  a field to every write and the board's deserializer, and invites two ids
  interleaving into one transaction.
- **A single `Batch { op }` variant**: kept as three verbs; the deserializer
  grew 1,780 B for all four new variants together, so collapsing three unit
  arms cannot save the >1 KB that would justify it.
- **A hello `LpFeature` or a `HardwareFacts.fs` field** for "this board has
  batches": reports a build or boot fact a cargo feature decides, where the
  `atomic` answer reports what this filesystem under this request does
  (including a host's `LpFsStd` and the sim's memory), and `LpFeature` ids are
  API forever.
- **Commit before load** (`… writes → Commit → Load → Hash`): simpler, but a
  project the board refuses or that resets it leaves nothing to fall back to
  (worse than the two-slot push) and defeats the "tried and recovered" rule.
- **Optional `codec`/`logicalLen` fields on `WriteChunk`**: changes the shape
  `device_stamp`, `lp-cli`'s rtt probe and the walks already use; the sibling
  costs one deserializer.
- **Fixed 4 KiB chunks**: today's request count, but half the full chunks do
  not deflate into one record, so at-rest compression mostly would not
  happen; chunk boundaries would also not be record-shaped.
- **Server-side inflate for every backend**, or a downcast to reach the tree
  store: defeats at-rest compression, or needs `Any` through `dyn LpFs`.
- **The browser's `CompressionStream`**: the push is shared Rust over bytes in
  memory, already linking a deflate encoder; a JS stream would add an async
  bridge, not run in `cargo test`, and give engine-dependent output that
  breaks stable chunk boundaries.

## Follow-ups

- Commit only after N frames have run (a project that loads but fails its
  first frames), parked by the plan.
- The engine's own inflate (~3 KB) might share the core's.
- The adoption round deletes `device_push_two_slot` and `other_slot` with
  littlefs.
- M5's batch-push walk on the `fs-tree` firmware: a drop mid-batch must leave
  the pre-push project byte for byte, and the next client must find no batch.
