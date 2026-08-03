# Panel writers: an unauthored runtime tier, persisted device-first in `/.lp/panel.json`

- Status: accepted
- Date: 2026-08-02
- Context: implements docs/design/panel.md P1–P4, P8–P11, P14 (ratified
  alongside modules.md 2026-07-31). Builds on the scoped-bus engine
  (`2026-08-01-scoped-bus-engine-architecture.md`), which supplies the
  `(scope, channel)` identity this whole tier keys on, and follows the
  runtime command-channel precedent from
  `2026-07-27-runtime-node-command-channel.md`. What decides WHICH
  controls reach a panel to engage these writers is
  `2026-08-03-panel-visibility-is-derived.md`.
- Plan: `planning/2026-08-01-1003-modules-impl-roadmap` P8–P10; the P11
  auto-save toggle reached the wire at proto 9 in
  `planning/2026-08-03-1021-modules-vision-push`.

## Decision

**A panel control is not an editor.** Turning a knob does not change the
project. It engages a *panel writer*: an unauthored, lazily materialized
runtime value source living on the Engine, keyed by `(scope, channel)`,
that outranks authored writers for that channel in that scope until
explicitly cleared.

Four consequences follow, and each one is load-bearing:

**The store is a side store on the Engine, not a binding.**
`apply_project_changes` rebuilds bindings from defs, so a panel writer
registered as a `BindingDraft` would be silently destroyed by the first
authoring edit — the user's dim would vanish because someone renamed a
node. `PanelWriterStore` therefore sits beside the binding set and
survives every edit that does not rebuild the Engine itself. A test pins
exactly that.

**Writers are lazy.** A writer materializes on first touch, never at load.
Eager materialization would make every public parameter self-shadow at
boot, and an outer scope could then never drive an inner channel —
modules.md R10 would be dead on arrival.

**Engagement REPLACES the scope's provider set, it does not join it.**
At the host seam, an engaged writer is returned alone. This is what makes
"panel wins" hold on `ByKey` merge channels too, where a merge would
otherwise blend the panel value with the authored ones and produce
something the user never asked for.

**The wire ops are runtime pokes, never authored ops.**
`WireProjectCommand::PanelWrite` / `PanelClear` are project-level arms —
they address a scope, not a node — and touch no overlay, no `PendingEdit`,
and no dirty flag. That is not merely tidy: it is what sidesteps
`PendingEdit` value-shadowing, which would otherwise fight multi-client
convergence. Clients learn state through ordinary probe pulls, so two
phones on one device agree, and a knob fight resolves last-writer-wins at
the engine with no locks and no ownership.

### Persistence is device-first, in the framework tier

Panel state persists to `/.lp/panel.json` *inside the project's own
filesystem* — the framework-owned tier, never an authored artifact. Both
sim tiers run on `LpFsMemory`, so **sims stay ephemeral by construction**
(settled D-B); unit tests are the correctness story there, and the device
walk confirms it. Persistent sims are recorded future work.

**Restore happens at Engine construction — before the first tick, and so
before the first render.** The requirement is verbatim from the design:
*4 a.m., Burning Man, LED scarf dimmed from a phone; unplug, replug — it
must come back dim, with not one bright frame.* A boot that renders even
one frame at authored brightness is non-conforming, which is why the seam
is `Project::new` (and `Project::reload`, which rebuilds the Engine) and
not a later ready-event. On device, `auto_load_project` runs before the
main loop, so this seam is the boot path.

**Keys are `scope-path / channel`** from `ScopeRef::persist_path` — tree
paths and authored entry keys, never runtime ids or indices — so state
survives reload, reattach, and sibling reorder. A sink scope keys by the
ENTRY, not the entry's child: swapping which node a playlist entry plays
keeps the entry's panel state. **State follows the slot, not the
content.** An entry naming a scope this project no longer has is dropped
on load: vendoring and renames degrade gracefully rather than failing a
boot.

**Version is bump-and-refuse.** An unknown version means the file is
ignored wholesale — no migration, matching the alpha posture everywhere
else in the format story (settles P-Q3). Losing panel state costs one
re-dim; a half-applied migration costs trust.

**Writes are throttled to ~10 s and gated on a mutation counter**, with a
flush on clean shutdown. The counter matters more than it looks: a clear
followed by a re-write inside one window leaves an identically-shaped
map, so comparing `(len, newest revision)` would miss it and silently
lose the newer value. An idle project writes nothing at all, however long
it runs.

**Momentary writers never persist** (P14). A gesture has no held value,
and a deadline that outlived a power cycle would be meaningless.

### The prerequisite that makes persistence safe at all

Writing inside the project filesystem fires an `FsEvent` back into
`Project::refresh_artifacts` → `apply_project_changes`. Without a filter,
**every ~10 s save would clear and re-register the whole binding graph,
and the rebuild would schedule the next save** — a permanent churn loop
costing the device its flash and its frame budget, triggered by nothing
more than leaving a knob engaged.

So `refresh_artifacts` drops `/.lp/**` events *first*, before anything
reads the batch, and returns early when nothing authored remains. This is
the same boundary `lpc_history::is_hashed_path` draws for the canonical
package hash and `SnapshotStore` draws for device copies; panel state
inherits both, so a dimmed scarf can never read as a modified project or
show up as a device diff. `Project::applied_refresh_count` exists so this
is *observable* rather than asserted — a test proves an authored write
moves it and a `/.lp/` write does not.

## Alternatives rejected

- **Panel state as overlay slot edits** (the transient/`SlotRole::Debug`
  tier used by clock rate/scrub). Rejected on identity: panel writers are
  `(scope, channel)` command-channel state, not per-node slot edits. This
  is the boundary the debug-slots-taxonomy ADR itself draws — events go to
  the command channel, Debug goes to the overlay, panel state goes to
  `.lp/panel.json`. Riding the overlay would also re-introduce the
  dirty-flag and `PendingEdit` coupling the whole design exists to avoid.
- **Panel writers as authored bindings.** Destroyed by
  `apply_project_changes`; see above.
- **Client-side panel state.** Breaks P9 outright: two phones would show
  different values for one control, and a device rebooting alone (no
  client attached) could not restore anything.
- **Writers that accumulate or generate** (`phase += speed·dt`). Rejected
  by P3: that behavior belongs to a node. The supported idiom is
  `speed` → phasor node → `phase` → consumer.
- **Slew in this phase.** Deferred deliberately (P-Q1 stays open):
  emission is immediate, which is correct on its own rather than a
  placeholder, and the seam when it arrives is writer-side shaping — the
  writer holds the raw value and shapes only what it emits, so nothing
  downstream changes.

## Consequences

- Any future input source — MIDI, OSC, hardware encoders, play mode,
  phones — enters through these same two ops with the same identity,
  latch semantics, and persistence rules. No second control path.
- A control whose backing slot has no bus channel behind it still edits
  its authored default through the slot path; the two coexist, and the
  control's `panel_target` is what selects between them.
- Turning auto-save off records itself in the file, so the choice
  survives a reboot rather than quietly re-enabling overnight.
