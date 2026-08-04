# Panel visibility is derived from authored bindings, never declared

- Status: accepted
- Date: 2026-08-03
- Context: implements docs/design/modules.md R3/R8/Q13 and panel.md P1
  (ratified 2026-07-31). Completes the pair started by
  `2026-08-01-scoped-bus-engine-architecture.md` (which supplies
  `(scope, channel)` identity) and
  `2026-08-02-panel-writers-and-state-persistence.md` (which supplies the
  writer tier this membership rule points controls at). Amends
  `2026-07-26-node-card-faces.md`, which introduced the `panel` flags this
  deletes.
- Plan: `planning/2026-08-03-1021-modules-vision-push` P2, P3, P6.

## Decision

**A control is on a panel exactly when its slot carries an AUTHORED
binding to a bus channel.** There is no authored panel flag: `SlotMeta
.panel`, `StaticSlotMeta.panel` and `ShaderSlotDef.panel` are deleted, not
kept in parallel. Publicity is a fact about the wiring, computed by the
project walk; nothing in an artifact declares it.

Three parts make that one rule work.

**1. Membership is the binding's derived `(scope, channel)`.** The project
walk decorates a bound slot's endpoint with a `UiPanelTarget`; a face
control carrying one is public, and it dispatches `PanelWriteOp` at that
target instead of editing the slot's authored default. One
`(scope, channel)` is ONE control however many cards below consume it
(panel.md P1) — the module panel dedupes by channel, and the SAME
`UiPanelControl` the leaf card renders is the one the panel shows, so the
two can never disagree.

**2. Only AUTHORED wiring counts.** A binding the loader materialized from
a slot's own `default_bind` (origin `Default`) is plumbing the author
never asked for. `bus:time` reaches nearly every shader that way, and a
time knob on the panel was noise the GV walk rejected outright: grabbing
time from a knob does not work, and offering it teaches the wrong thing.
The channel stays wired, listed, and readable in the wiring drawer — it
is simply not a control. The sanctioned way to drive time is the clock's
own transport, which is registered future work.

**3. The panel is derived by a post-pass over already-built cards, and it
reaches into sink scopes.** A module's panel is assembled by walking its
card subtree for controls whose target names its scope, stopping at child
modules (they own their own scope, and their panel rides along as a
nested group). A playlist's ACTIVE entry is the exception, and it cost
two fixes:

- Its controls live in the entry's *sink* scope, which the probe omitted
  from its channel list by construction. The control was healthy and the
  write landed; the row carrying the live value and the Panel-origin
  provider simply did not exist, so the knob read as inert. **Wire proto
  8 lists sink scopes** — with value resolution still refusing any
  channel whose winning provider is a sink-scope producer, so a probe
  pull can never render an inactive entry.
- A module's panel does not descend past a playlist (an entry's subtree
  belongs to that entry's scope, not the module's), so fyeah-sign's root
  panel — the one the end user sees in play mode — was empty while its
  knobs sat one card down. The panel now appends **one group per
  playlist whose ACTIVE entry publishes anything** (R9), targeting that
  entry's sink scope so the group reset clears exactly what the entry
  engaged. No active entry, or an entry publishing nothing, means no
  group: an empty cluster is worse than no cluster.

## Rejected alternatives

- **Keep `panel: bool` alongside the derived rule.** Two sources of truth
  for one question, and the flag would silently win or silently lose
  depending on derivation order. Deleting it is what makes "binding is
  publicity" checkable rather than aspirational.
- **Any binding is publicity, `default_bind` included.** Shipped first,
  then reversed at the GV gate. It made publicity a property of a slot's
  *shape* (every shader consuming `time` published a time knob) rather
  than of what the author wired, which is the opposite of curation.
- **Authored panel layouts** (an explicit list of promoted controls per
  module, as the closed #218 spike's `controls{}` map proposed). Deferred,
  not rejected: curation is a real need for a published module, but it is
  an additive override on top of a derived default, and a derived default
  must exist first or every module starts empty.
- **A second control family for panels**, derived independently of the
  cards. Rejected: one `(scope, channel)` is one control, and two
  derivations of the same knob is exactly the shape that lets a panel and
  the card below it disagree. The module panel reuses the very
  `UiPanelControl` the leaf card built.
- **Per-scope panel pulls.** Rejected: the panel converges on the reads
  the client already makes. Panel state and auto-save ride the runtime
  read (proto 9), so there is no second refresh cadence to keep in sync.

## Consequences

- Publishing a control is one edit — bind the slot — and unpublishing is
  unbinding it. Nothing has to be flagged twice.
- A gallery example that binds nothing opens onto an empty panel. That is
  now a content bug, pinned by
  `every_gallery_example_opens_onto_a_populated_root_panel`.
- A kind with no face contributes no panel controls even when its slots
  are bound: `ComputeShader` publishes channels that appear in the wiring
  drawer with no knob above them (meteor's `speed`/`count`). Registered as
  open work in modules.md §10.
- The `panel` flag's deletion is wire- and schema-visible; the two schema
  shapes and the agent's `upsert_param` tool lost the argument.
- Because membership is derived, the panel changes when the wiring
  changes — including live, when a playlist switches entry and the active
  entry's group is replaced.
