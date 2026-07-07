# ADR: Binding-Graph Probe as the Bus/Binding Read Surface

- **Status:** Accepted
- **Date:** 2026-07-06
- **Deciders:** Photomancer
- **Supersedes:** the never-implemented `ExplainSlot` probe (wire stub
  deleted in the same change)
- **Superseded by:** None

## Context

Nothing bus-shaped existed on the wire: clients could not enumerate
channels, see writers/readers, read channel values, or learn about
default and implicit bindings. The runtime bus is **virtual** — `bus#X`
is demand-resolved per frame from provider bindings via the node binding
index; there is no channel state to mirror. Studio needs one truth
surface to feed binding indicators, detail popovers, binding-derived rows
for implicit consumed slots (`fixture.input`, `output.input`), the bus
pane, and the M4 channel picker.

The project-read protocol has two idioms: **queries** (revision-gated,
applied to the persistent client mirror) and **probes** (ungated,
request-scoped diagnostics collected beside the mirror; e.g. the render
and control product probes).

## Decision

**One probe — `BindingGraphProbeRequest { include_values }` — returns the
whole effective binding graph plus a bus-channel summary.**

- `WireBindingGraph { revision, bindings, channels }`:
  - `bindings: Vec<WireEffectiveBinding>` — every registered binding,
    authored **and** default, including bindings on implicit runtime
    consumed slots with no def field. Each carries: owner node, anchor
    `node` + optional `slot` path, `direction` (consumes | publishes),
    `endpoint` (bus channel | node slot | literal), `origin`
    (authored | default), priority, and semantic kind.
  - `channels: Vec<WireBusChannel>` — name, established kind, and
    `providers` / `consumers` as **indices into `bindings`** (sites are
    never duplicated), plus an optional resolved value.
- **Probe, not query.** The graph is derived runtime state; values change
  every frame while topology changes only with project edits. Probes are
  ungated, mirror-free, and request-driven — the bus pane polls while
  visible, exactly like product previews poll for focused nodes. No
  `ProjectView` schema change.
- **Values on demand.** `include_values: false` costs no resolution work.
  With values, the engine resolves each channel through a resolve session
  at read time; failures travel as `WireBusChannelValue { error }`, never
  as a failed probe.
- **Identity is `NodeId` + slot path.** Display labels resolve client-side
  from the node-tree mirror the client already holds; the id is what
  linked navigation (focus/reveal) needs (roadmap D7).
- **Origin derives from priority** for now (`authored() == 0`,
  `default_fallback() == -1000`); the wire enum is stable when M5 swaps
  hardcoded loader helpers for declarative policy.
- **Virtual bus preserved.** The snapshot derives from the binding index
  (channel kinds, `bus_targets`, plus a new symmetric `bus_sources` map).
  A future materialized bus (external writers: OSC/MIDI/radio) can serve
  the identical contract.
- **`ExplainSlot` is deleted.** Its wire types were never implemented
  (`Unsupported` stub); per-slot provenance is a client-side view over the
  binding graph.

## Consequences

- Indicator popovers, binding-derived rows, the bus pane, and the channel
  picker all consume one payload; per-node views are client-side
  projections of the graph.
- Whole-graph snapshots scale with binding count (tens for real projects,
  a few KB of JSON). If projects grow orders of magnitude, add filter
  params to the request — additive, not breaking.
- A literal published directly to a bus channel (no local slot) is
  representable (`slot: None`) but drops the literal from the anchor; the
  channel's resolved value still shows it. Accepted simplification.
- M1's client-side parse of authored `bindings` maps stays: it is
  edit-synchronous (works against unsaved overlay state), while the probe
  is runtime truth (defaults, implicit slots, priorities). The two views
  complement rather than replace each other.

## Alternatives Considered

- **Revision-gated query into the mirror:** right shape for persistent
  topology, wrong shape for per-frame values; splitting topology (gated)
  from values (ungated) doubles the surface for no MVP benefit.
- **Two probes (bus channels / per-node bindings):** per-node results
  duplicate the same sites the channel summary needs; a single graph with
  index references is smaller and one code path.
- **Materialize the bus first:** rejected in the roadmap design session
  (D3) — no engine-semantics change, no per-frame cost when nobody looks.

## Follow-ups

- M3 bus pane and binding-derived node rows consume this surface.
- M5 replaces the origin derivation's producer (loader helpers →
  declarative policy) without touching the contract.
- Bus value **writes** (operator overrides) are out of scope; recorded as
  future work in the roadmap.
