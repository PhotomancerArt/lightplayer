# ADR: Binding-Graph Probe as the Bus/Binding Read Surface

- **Status:** Accepted
- **Date:** 2026-07-06
- **Deciders:** Photomancer
- **Supersedes:** the never-implemented `ExplainSlot` probe (wire stub
  deleted in the same change)
- **Superseded by:** None


> **Note (2026-07-08):** binding-ref syntax has since changed to
> `bus:<channel>` / `node:<path>#<slot>` and `time.seconds` was renamed
> `time` — see ADR `2026-07-08-binding-ref-syntax-and-channel-naming`.
> Examples below use the older syntax.

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
  *Amended 2026-08-03:* the bus pane is gone; the same rows are now the
  module card's wiring drawer, one per scope, and the probe grew scoped
  channel rows (wire proto 7–8) to key them —
  `2026-08-01-scoped-bus-engine-architecture.md`,
  `2026-08-03-panel-visibility-is-derived.md`.
- M5 replaces the origin derivation's producer (loader helpers →
  declarative policy) without touching the contract.
- Bus value **writes** (operator overrides) are out of scope; recorded as
  future work in the roadmap.

## Amended 2026-09-23: structure is revision-gated

The "Alternatives Considered" note above rejected splitting topology from
values as "no MVP benefit". The lens now polls this probe every 150 ms on
links where bytes matter (USB serial today, BLE next), and the byte ledger
(lean-wire plan, Run D) found the benefit: the graph was nearly half of a
steady PLAYFUL-choker lens read, and most of it was resent unchanged. So
the probe now sends **structure on change, values every read** — still one
probe, one request, one answer (wire proto 21):

- **Request:** `BindingGraphProbeRequest { structure: RevisionGateRead,
  include_values }`. `RevisionGateRead` (`None` | `Always` |
  `IfChanged { known }`, a list of known revisions since proto 22) is the
  same gate the geometry of control-product and output-frame probes uses.
- **Answer:** `WireBindingGraphRead { structure:
  RevisionGateResult<WireBindingGraph>, values: Option<WireBusChannelValues>
  }`. The structure — bindings, and each channel's scope, name, kind,
  providers, consumers and primary-visual role — is `Changed(graph)` or a
  few-byte `Unchanged { revision }`. The values are a positional list in
  the structure's channel order, stamped with the structure revision they
  were resolved against; each is `unresolved` (sink no-demand), `empty`,
  `value(..)` or `error(..)`. `WireBusChannel` no longer carries a value,
  and a value no longer carries its own revision.
- **The structure revision moves only when the structure does.**
  `WireBindingGraph::revision` used to be the engine revision at snapshot
  time — it moved every tick. It is now stamped by content: the engine
  builds the structure each read, hashes its wire bytes (FNV-1a 64, no
  second copy held on the device) and keeps the revision at which that
  hash last changed, always later than the one it replaces. Registering or
  removing a binding, a priority or kind change, a channel appearing or
  disappearing, a `panel = "show"` hint changing, a panel writer engaging
  or letting go — all move it, because all change the bytes; nothing a
  mutation site forgets to declare can slip past.
- **A value may not live inside the structure.** An engaged panel writer's
  provider row used to be `endpoint: literal { value: <the knob's
  position> }`. That was the whole of the churn Run D saw: `bindings` changed
  15 times in the session (16 distinct values), 2 of them genuine (a panel
  writer engaging on two channels) and 13 of them a knob position riding
  the structure. The row is now the value-free `WireBindingEndpoint::PanelWriter`;
  the position is the channel's value (the writer outranks every provider).
- **Clients never mix halves.** A value list whose structure revision is
  not the held structure's (or whose length is not its channel count) is
  dropped, never applied to the wrong channels, and the next read asks the
  structure `Always`. Studio's `BindingGraphCache` holds one consistent
  structure-and-values pair; structure-only derivations (the export lint)
  key off the structure revision and re-derive only when it moves.

Choker steady lens read: the graph went from 4,562 B to 775 B (a 29 B
`unchanged` plus 653 B of values); the whole read from 8,698 B to
4,911 B (`lpc-engine/tests/lens_read_wire_size.rs`).

## Amended 2026-09-23: `no_provider`, and the general rule this split became

A P6 ledger pass over the same lens read found six of the choker's channel
values answering `error` for a bus channel with no publisher — a
`Debug`-formatted error string, ~53 B each, for a condition that is not an
error so much as a fact about the graph's shape. `WireBusChannelValue` grew
a bare `no_provider` tag for it (no payload), cutting those six values from
653 B to 336 B. The distinction this ADR draws between a probe's structure
(gated) and its values (sent every read) turned out to be the general shape
every probe with a static/moving split needed — the buffer-geometry probes
(`sample_layout`, `display_layout`, `placements`) gate the identical way
through a shared `RevisionGateRead`/`RevisionGateResult` pair. That general
rule, the wire-tap tooling that found every cut in this family, and the
Studio-side one-copy pixel ask policy that came out of the same pass are
written up together in
`docs/adr/2026-09-23-project-reads-send-only-what-changed.md`.
