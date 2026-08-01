# ADR: The dataflow resolver keeps its resolution across frames

- **Status:** Accepted
- **Date:** 2026-07-31
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Frame cost on the desk ESP32-S3 was measured as a flat ~8.4 ms per
fixture+output chain — the same whether a fixture rendered 30×1 or 90×90, and
the same for 10 mapped LEDs or 120. Authors could not buy the cost down, which
made multi-fixture projects degrade linearly for no reason they could see (4
fixtures = 20 fps; ~10 would be single digits).

`lp-cli profile` attributed it. The shader — the thing the product is named
for, JIT-compiled to native code on the device — was **1.1%** of self cycles.
The frame was dominated by the dataflow resolver re-resolving the binding
graph from cold every tick: `Resolver::clear_frame_cache` ran each frame, so
each frame re-walked `resolve → resolve_binding_source → resolve` with
`SlotPath::parse` per read, `QueryKey` allocation, equality and drop, and the
allocator/`memcpy` pair at ~44% of self cycles combined.

The resolver already had a cache that outlived nothing. It was cleared per
tick because one method, `clear_frame_cache()`, served two unrelated callers:
"a new frame started" and "the graph changed shape". While both said the same
thing, no caller could persist anything.

## Decision

**Resolution persists across frames and is invalidated by structural change,
not by the passage of a frame.**

The resolver distinguishes two operations:

- `begin_frame()` — values resolved for the previous frame are stale, because
  producers tick and time moves. The graph is unchanged.
- `invalidate_structure()` — bindings, tree topology, node definitions, or
  project state changed. Everything cached is void.

Three things persist until `invalidate_structure()`:

1. **Routes** (`ResolvedRoute`) — the decision about *how* a query is
   answered: which binding wins (owner depth for consumed slots, priority for
   bus providers), the merge policy, and the fully expanded provider list for
   mergeable receivers. Computing this reads the binding index and introspects
   authored definitions; none of it can change without a structural change.
2. **Def-sourced values** — `ProductionSource::Literal` (binding literals) and
   `::Default` (authored-def reads, including their deep `snapshot_slot_shape`
   copies). Both are functions of the project, not of the frame.
3. **Interned queries** — `QueryKey → QueryId`, plus a memo from
   `(node, constant path)` to the query it names.

Everything else stays per-frame: producer values, merges, and anything reached
*through* a binding. A cached route still runs the producer behind it, once
per frame.

Failures are never cached. An ambiguous bus channel or a cyclic binding graph
keeps reporting itself rather than being remembered as a decision.

## The invalidation contract

**Any engine mutation that can change what a query resolves *through* — as
opposed to what it resolves *to* this frame — must call
`Resolver::invalidate_structure()`.**

This is the load-bearing rule, and it does not fail loudly. A missed call
serves a stale but entirely plausible answer. The current sites:

| Site | Why |
|---|---|
| `Engine::apply_project_changes` | Every registry-driven change: deploy, reload, overlay mutation, node add/remove/replace, def changes, asset refresh. Also rebuilds all bindings. |
| `Engine::remove_runtime_subtree` | Tree topology |
| `Engine::reattach_runtime_node` | A node's runtime is replaced under its consumers |
| `Engine::add_binding` | A new binding can win a slot that already resolved |
| `Engine::clear_bindings` | The binding graph is emptied |

Two of those were found while writing this down. `add_binding` never
invalidated anything — harmless while every frame cleared regardless, a
stale-value bug the moment it did not. `clear_bindings` ran on the tree, so
emptying the binding graph left the resolver's knowledge of it intact and
depended on whatever ran next to invalidate; it now goes through the engine.

A playlist entry switch is deliberately **not** structural. It changes which
child a node demands, not what any query resolves through, and the resolver
handles it by simply being asked a different question.

## Consequences

### Measured

1-fixture workload (`projects/test/quad-strips-1fix`), steady render, esp32c6
cycle model:

| | Baseline | After |
|---|---|---|
| Total attributed cycles | 2,468,427 | **1,146,110** (−54%) |
| allocator + memcpy | 44.0% | 34.4% |
| `[jit] render` | 1.1% | 2.4% (same cycles, less than half the frame) |

`QueryKey::eq`, `merge_policy_for_consumed_slot`,
`bindings_for_consumed_slot`, `slot_lookup` and `Vec<SlotPathSegment>::clone`
no longer appear in the top twenty.

The 4-fixture workload moved only −1.9%, because
`HwRegistry::endpoint_status_for` is 46.7% of it and is a separate per-frame
recomputation with its own owner. See
`docs/debt/s3-frame-cost-scales-per-fixture.md`.

### Revision stamps age

A cached literal or def read keeps the revision it was first stamped with,
rather than being restamped each frame. This is the more truthful reading —
the value genuinely has not changed since then — and an audit of every
`changed_at` consumer on the resolve path found none that compares a
production's revision against the current frame. Playlist triggers compare
message sequence numbers; wire delta-sync treats an aged stamp as "unchanged,
do not resend", which is correct for a value cached *because* it is unchanged.

### Ids are epoch-scoped

`QueryId`s are indices into a table that is cleared on every structural
change. They must never be stored anywhere that outlives the resolver's
epoch — which is why routes, themselves epoch-scoped, are the one place that
holds them.

## Alternatives considered

**A compiled resolution plan** — build an explicit schedule of producer ticks
and routes at load time and execute it per frame. Higher ceiling: no
per-query overhead at all. Rejected because nodes issue queries that are not
knowable ahead of the tick (a fixture reads `power.some` only if the def has
it; a shader reads one query per declared input; a playlist demands whichever
child is current), so the plan would need a discovery pass per frame anyway —
which is lazy memoization with extra steps. This decision does not foreclose
it: routes are already the per-query half of such a plan.

**Leaving the cache per-frame and optimizing the constants** — cheaper key
comparison, fewer clones. Rejected on the measurement: the work itself was
redundant, not merely expensive.

## How this is kept

Two mechanisms, because the failure mode is silence:

- **Counters** (`ResolveFrameCounters`) make "did this frame re-derive the
  graph?" a testable question. `steady_state_frame_does_zero_structural_work`
  asserts a steady frame performs no binding lookups, no merge-policy reads
  and no def reads.
- **A differential test** runs the same scene twice — once caching, once with
  `set_force_invalidate_per_frame(true)`, which reproduces the old
  clear-everything-per-tick behaviour — through binding replacement, a
  producer switch and a node reattach, and demands the two agree frame for
  frame. This is the check that does not require someone to have first
  imagined the particular way a cache could go stale, which matters because a
  wrong cached answer is a plausible answer and survives assertions written by
  whoever wrote the cache.
