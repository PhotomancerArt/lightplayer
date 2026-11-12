# ADR: Revision-Gated Project Reads

- **Status:** Accepted
- **Date:** 2026-07-03
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

`ProjectReadRequest` carries a `since: Option<Revision>` — the client's last
known project revision — but before this work the server ignored it for every
family except the node tree. A read re-streamed the full shape registry, every
runtime-buffer summary, and a full snapshot of every slot root on each request,
regardless of what had actually changed. Over browser serial and ESP32 this is
wasteful: the client already holds most of that state and only needs the delta
since `since`.

The revision substrate to do better already existed. There is a single global
monotonic project revision (`current_revision()` / `engine.revision()`),
advanced once per frame, and per-item `changed_at` stamping (`WithRevision<T>`)
is pervasive — slot leaves, node entry state/status, runtime buffers, and shape
entries all carry one. `Begin { revision }` / `End { revision }` and the
`Runtime` status already carry the current revision `R` on every stream, so a
client can always advance even on an empty read. What was missing was the
per-family filter that compares each item's `changed_at` to `since` at
message-build time, plus a way to communicate removals so a gated stream (which
omits unchanged items) does not leave the client holding items that no longer
exist server-side.

The hard constraint (roadmap D-constraints): the server keeps **no per-client
state**. `since` arrives in the request; everything the server sends must be
derivable from project state that is identical for all clients. That rules out
any mechanism that requires remembering what a particular client has seen.

## Decision

For a `ProjectReadRequest { since: S, .. }` against a project at revision `R`,
each mirrorable item is included **iff its `changed_at > S`**, compared per item
at message-build time. No pre-aggregation: there is no per-family or
project-wide "max changed" watermark computed up front; the gate is applied to
each item as the stream is built (D3). Concretely:

- **`since` semantics.** `S = None` is treated as `0`. Inclusion is strictly
  `changed_at > S`. **Bulk-sync guard:** when `S == 0` (a fresh client), every
  family sends all live items, so items stamped at the default revision 0 are
  not lost. For `S > 0`, strict `>` applies uniformly. This matches the tree's
  long-standing `tree_deltas_since(since == 0)` behavior.

- **Always-present revision.** Every stream carries `R` in `Begin`, `End`, and
  the `Runtime` result. A read with `S == R` therefore transfers zero
  shape/slot/node/resource payload — only the revision-carrying spine and any
  probe results.

- **Membership sync for shapes and resources.** A gated stream omits unchanged
  items, so it cannot express a *removal* by omission (omission means
  "unchanged"). Rather than a removal ledger or tombstones, each family keeps an
  `ids_revision` bumped when its id set changes (shapes:
  `SlotShapeRegistry.ids_revision`, already on the wire in shapes `Begin`;
  resources: a `Revision` field added to `RuntimeBufferStore`). When
  `S < ids_revision`, the server sends the family's **current full id list** (a
  `Membership { ids }` / `Membership { refs }` event before `End`); the client
  prunes any local item not in that list. When `S >= ids_revision`, no
  membership is sent and absence means unchanged. Additions need nothing extra —
  a new item has `changed_at > S` and arrives as an ordinary entry.

- **`ByRefs` resource payloads bypass `since`.** An explicit
  `ResourcePayloadRead::ByRefs` request is a targeted fetch, not a mirror sync,
  so it delivers the requested payloads regardless of `since`. Summaries are
  still gated.

- **Slots are gated per root (G6a).** A slot root is included only when the
  root's own change revision is `> S`, keyed to the owning node/def entry
  revision (`NodeDefEntry.revision` for `.def` roots, node runtime `changed_at`
  for `.state` roots) — **not** a new `max()` over slot leaves. When included,
  the whole root snapshot is sent, as today. This is "one item = one root," the
  same granularity as the node query, consistent with D3. Sub-root patching is
  deferred to M6. Root removal rides the tree's implicit `ChildrenChanged` (a
  removed node drops its roots on the client).

- **Tree removal stays implicit.** The node tree already conveys removal via
  `ChildrenChanged` with the child absent, and the client applies it
  progressively. No explicit `Destroyed` delta was added.

- **Probes are unaffected.** Probes always execute regardless of `S`. They are
  live diagnostic work, not mirror state.

- **No per-client server state.** `S` arrives in the request; the server retains
  nothing about the client between requests. `ids_revision` is project state
  keyed by revision, identical for all clients.

### Membership sync vs. a removal ledger — rationale

The alternative to membership sync is a **removal ledger**: record each
`(id, removed_at_revision)` and, on a read, replay every removal with
`removed_at > S`. The problem is unbounded history. A ledger must answer
"what was removed since `S`" for arbitrarily old `S`, so it can never be
pruned without a fallback — and the only correct fallback when the ledger no
longer reaches back to `S` is "send the current full id list and let the client
reconcile." Membership sync is exactly that fallback, used directly and always:
it is stateless, self-correcting, and needs no history. The cost is that
`ids_revision` currently bumps on additions too (and, for shapes, on any entry
replace — see the note below), so a membership list occasionally rides along
with a pure addition or content change; those are a few redundant bytes that
double as a self-check, and the client's prune is a no-op when nothing was
removed. Tombstones have the same unbounded-retention problem as ledgers and
additionally pollute the live id space.

## Consequences

- A read at `since == R` transfers no mirrorable payload for any family; a fresh
  read (`since == None`/`0`) transfers everything. Both are proven by the
  cross-family contract suite in `lpc-engine`
  (`read_at_since_r_sends_no_payload_items`, `fresh_client_receives_everything`,
  `probes_run_regardless_of_since`) plus per-family gating/membership tests.
- The client mirror apply paths became additive/merge for the gated families
  (upsert shapes and resources rather than replace-and-evict; upsert slot roots
  rather than clear-and-rebuild) with membership-driven pruning, so a partial
  stream is applied correctly. This is the minimum needed for a gated read to be
  correct; the full per-event progressive apply is M6.
- The engine gating code is `no_std` and runs on-device; firmware builds and the
  emulator scene-render test stay green.
- The default Studio request is deliberately **still full-snapshot** in M5
  (`since == None`), so no live behavior changes yet. The win is proven at the
  contract level; the live flip is M6.
- The aggregate `ProjectReadResponse` / `ProjectReadCollector` still work
  because the default read is a full snapshot and the contract tests exercise
  the event stream directly. Deleting them is M6's job.

## Alternatives Considered

- **Removal ledger (per-family `(id, removed_at)` log):** correct but requires
  unbounded history or a fallback that is itself membership sync. Rejected in
  favor of using that fallback directly.
- **Tombstones (retain removed items flagged deleted):** same unbounded
  retention problem, and they pollute the live id space. Rejected.
- **G6b sub-root slot patches:** recurse `SlotData` comparing each node's
  revision to `S` and emit `SlotRootDelta` patches for changed subtrees only.
  This is the `WireSlotPatch` progressive-apply machinery M6 is chartered to
  build; pulling it into M5 would swallow M6. Deferred; M5 uses per-root gating
  (G6a).
- **Per-family pre-aggregated watermarks:** compute a single "max changed
  revision" per family and gate the whole family on it. Rejected per D3 — it is
  the pre-aggregation the contract explicitly forbids as the primary mechanism;
  the gate must be per item at the request query's granularity.
- **Explicit tree `Destroyed` delta:** unify removal representation across
  families. Not needed — `ChildrenChanged` already conveys tree removal and the
  client already applies it. Deferred (revisit only if unifying removal is
  judged worth it).

## Follow-ups

- **M6:** flip Studio's default read to gated (`since = last known revision`),
  wire the full per-event progressive apply, and delete the aggregate
  `ProjectReadResponse` / `ProjectReadCollector` collector and the
  `event_stream_matches_full_debug_response` identity test (a delta stream no
  longer equals a full snapshot).
- **Removal-only `ids_revision` bump.** The shape registry currently bumps
  `ids_revision` on *any* entry replace, not only on add/remove (an
  `lpc-model`-level quirk left untouched here — see the M5 discovery notes), so
  the shape membership list is emitted whenever any shape's content changes, not
  only when the id set changes. Resources bump `ids_revision` on
  insert/remove only. A later lean-out is to bump `ids_revision` strictly on
  membership changes (adds/removes) across both families, so the membership list
  rides only when an id actually left the set. This is a correctness-neutral
  chattiness reduction (the client prune is a harmless no-op today).
