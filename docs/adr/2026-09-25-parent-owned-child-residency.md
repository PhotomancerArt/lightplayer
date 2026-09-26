# ADR: A Parent Node Owns Its Children's Residency; the Engine Applies It Before the Tick

- **Status:** Accepted, pending the ship gate of PR #827 (the plan's final
  review decides; until it merges, nothing here is in `main`)
- **Date:** 2026-09-25
- **Deciders:** Photomancer (Yona's rulings are the vision's D1–D21 and the
  plan's A1–A6; the director's are its DD log)
- **Supersedes:** the "wake on demand from binding resolution" section of the
  archived node-runtime design
  (`docs-archive/roadmaps/2026-04-28-node-runtime/design/01-tree.md:141-156`)
- **Superseded by:** None

## Context

A piece should hold about 25 patterns under one playlist on an ESP32-C6.
Loading every entry cost about 17.6 KB of heap each, which the C6 cannot
spare: at 8 entries the first shader no longer had room to compile. So only
the playing entry can be loaded. Something has to decide which entry is
loaded, and something has to load and unload it safely on abort-tier
firmware, where an allocation that fails is fatal.

The planning discovery (`lp2025/2026-09-24-2351-multi-pattern-projects`,
`notes.md`) found:

- The registry re-derives the whole inventory on every edit. Dormancy has
  to be known to derivation, or the next edit re-adds every entry.
- `Engine::tick` has no filesystem and only a `&ProjectRegistry`. It cannot
  load anything.
- The top of a tick is already a safe point: no render borrow is live and no
  node is `Executing`. The compile window opens there
  (`2026-08-03-memory-pressure-at-compile-safe-points.md`).
- The tree tombstoned removed nodes and never shrank, so any design that
  removes and re-attaches nodes on every switch would leak slot storage.
- Nothing stays `Pending` in normal operation, and a read of a node that is
  not `Alive` is a hard error, not a fall-through to slot defaults (the
  old doc comment on `NodeEntryState` said otherwise and is corrected).
- An archived design had the resolver wake `Pending` nodes when a binding
  first demanded them. It puts the decision in the dataflow graph, which
  only knows about loaded nodes.

## Decision

1. **A dormant child is tree-scoped, not a node state** (vision D1). A
   dormant entry's whole subtree is absent from the node tree: no tree node,
   no inventory rows, no bindings, no artifact-store locations, no phasors.
   Only the playlist's own def remembers it. `NodeEntryState` and the wire
   are unchanged; the reason an entry is not loaded (not playing, disabled,
   failed) lives with the owner, on the device only (D5, plan PD10).

2. **Loading children is the parent's job** (D2). A node asks for a load or
   an unload of its own entry-keyed children through a polled
   `NodeRuntime::residency_request() -> Option<ResidencyRequest>`, at most one
   load and one unload per request, cleared by taking it. The owner of a sink
   scope never demands a child that is not loaded (D4).

3. **Only the Playlist does this, for now** (D3). No other parent implements
   the request.

4. **The registry owns the residency set** (plan PD1): per playlist use
   location, the set of resident entry keys, `{idle_entry}` at load.
   Derivation stops at a non-resident entry's `ref`. Changing the set
   re-derives and reports the entry as added or removed, like an edit.

5. **The tick owner applies requests before the tick** (plan PD2).
   `Engine::apply_residency(fs, &mut registry)` runs in `lpa-server`
   `Project::tick`, the one path every edge ticks through (fw-esp32c6,
   fw-emu, fw-browser, the host server). Per request:
   - **Unload first**, so two entries are never held at once: registry
     re-derive, then remove the runtime subtree. The subtree's phasors, a
     removed clock's timebase and their scrub history leave with it, as does
     the entry's own sink-scope phasors (without that, a departed pattern's
     phasors lingered 120 ticks, and heap over a cycle depended on which ones
     had not expired yet).
   - **Then load**: registry re-derive, project the spine, attach the
     subtree, then re-wire the whole projection (asset consumers, every
     binding, the resolver). The re-wire is the same code an edit uses.
   - **Tell the owner** through one of four hooks: `entry_unloaded`,
     `entry_loaded(entry, child, output_slot)`, `entry_load_failed(entry,
     reason)`, or `residency_refused(request, reason)` when the unload would
     strand edits.

6. **A load failure never fails the tick.** A failure anywhere — the
   registry, the attach, a node that attaches `Failed`, the binding re-wire —
   is rolled back, nothing is left half-attached, and the owner is told.

7. **Pending edits never wedge a switch.** Transient edits (Debug-role
   overrides and produced paths, which a commit never writes and a reboot
   never keeps) are dropped by the unload. Edits a commit would write refuse
   the whole request through `residency_refused`, and the owner keeps playing
   what it has.

8. **Knob memory survives dormancy** (D12). Writers in an entry's sink scope
   are keyed by the playlist's id, which a switch does not change. Writers in
   scopes owned by nodes inside the entry (a pattern module's own knobs) are
   keyed by ids a reload replaces, so they are parked by persist path on
   unload and re-engaged on load. Panel restore accepts every authored
   entry's scope. `/.lp/panel.json` is unchanged; it was already keyed by
   persist path (`docs/design/panel.md` P11).

9. **The resolver never wakes nodes** (D6). The archived design is rejected:
   the dataflow graph only knows loaded nodes, and hiding a load (a parse, a
   compile) inside a resolve puts real time on the hot path with nobody
   deciding it.

10. **Keep-last-good is untouched, because dormancy removes the whole node**
    (D11). A shader keeps its last good compiled program across a bad edit
    for as long as the node lives. A dormant entry has no node, so there is
    nothing to keep: its compiled code is freed with it, which is what stops
    code piling up over a cycle. And you have to load to edit: opening or
    editing an entry in Studio loads and plays it, so an edit always lands on
    a live node and keep-last-good applies exactly as before.

11. **The tree frees removed slots** (plan PD3, A4). `RuntimeNodeTree` stores
    live entries only (`NodeEntrySlots`: sorted by id, with an O(1)
    id-position guess), so a removed node's slot is dropped. `NodeId`s are
    still never reused. 100 three-node reloads now grow nothing (the
    tombstoning tree grew 145,152 B on the host;
    `docs/defects/2026-09-25-node-tree-tombstones-grow-per-reload.md`).

### What the Playlist does with it

- **The switch** (D8–D10, plan PD4/PD5): the playlist holds its whole def
  entry list with an optional child. A switch holds the frame it last
  showed (one lamp-sized RGBA16 buffer, alive for the switch only), unloads,
  loads, shows the held frame until the new entry renders for real, then
  fades from it. It is never black.
- **Both output paths hold** (the texture-path decision). A fixture with
  `"sampling": "texture_area"` — the default when a fixture names none —
  drives lamps through the texture path, so that path holds too: the held
  frame is a texture at the request's size, copied GPU-side, and a render
  of another size (a canvas preview) cuts to the live product rather than
  being held. When both paths are in use, each holds its own frame.
- **A failure marks the entry and moves on** (plan PD9). A load or compile
  failure marks that entry failed, the playlist moves to the next enabled
  entry while still holding the frame, and the cycle skips a failed entry
  until the project reloads or the entry is edited. Activating a failed
  entry retries it, so fix-then-open works.
- **`active_entry` names the loaded entry**: it moves to a new entry when
  that entry's child is live, not when the switch is decided.
- **Cycling** (D13, plan PD6, A1–A3): `cycle` (`Hold` or `Cycle { step,
  fade }`, step ≤ 0 frozen) and `skip` (a list of keys) are optional def
  fields, consumed from `bus:playlist.cycle` / `bus:playlist.skip`: the
  authored value is the default, a Play-mode panel write overrides it and
  persists. The cycle position is a pure function of the playlist's consumed
  clock plus an anchor a pick, trigger or next/prev sets, so it follows the
  clock's speed and pause. While cycling, the idle entry is an ordinary stop;
  with the cycle off, idle and its triggers behave exactly as before (D17).
  Next/prev are trigger ids on the playlist (D20).
- **Absent is typed.** "Nothing authored and nothing written" for `cycle` and
  `skip` is an allocation-free `ResolveError::is_absent_option`, remembered
  by the resolver until the graph changes shape, not a matched error string
  (the fixture's `power` and the shader's float-mode pin read the same way).

### Around it

- **Studio checks every entry, the device loads one** (D19). `lp-cli upload`
  loads every entry host-side first and refuses to deploy a project with a
  broken one, naming it; a Studio save warns about a broken entry but saves,
  because saving work in progress must never be blocked. The device's own
  load failure is the backstop.
- **Removing a node sweeps dormant entries' files**: `remove_node` widens
  residency to every entry before it diffs, then narrows back
  (`docs/defects/2026-09-25-remove-node-orphans-a-dormant-entrys-files.md`).
- **fw-browser behaves as the device does** (D14): the same residency path.

## Consequences

- **Measured** (every number emulated, never hardware; the report is
  `docs/reports/2026-09-25-dormant-playlist-entries-proof.md`):
  - a dormant entry costs **343 B** of heap at load, against ~17.6 KB loaded
    (fw-emu `lp-cli profile --mode startup`, 5 vs 25 entries);
  - retained heap after a second full 25-entry cycle equals the first to the
    byte (**0 B**; fw-emu, after the phasor fix);
  - the 25-entry tryout uploads and cycles on `lp-emu:esp32c6:t1` with
    105,456 B free after the first compile and 91,284 B at its lowest;
  - a switch shows a pause of about **190 ms** (emulated t1: ~150 ms unload
    and load, one held frame, ~40 ms compile). It reads as a pause, not a
    blackout, because the held frame stays up.
- **The flash cost** of the whole mechanism on the C6 image is about 32 KB
  (residency +14.6 KB, the switch +6.9 KB, cycling +7.2 KB, the typed absent
  read +3.1 KB); headroom stays above 240 KB against the 64 KB floor. The
  residency step was not audited for duplicate monomorphizations.
- **A triggered entry pays a load and a compile on every trigger** (A6).
  fyeah-sign's blast now starts about 120–200 ms after the press is decided
  (emulated), where before it started on the next frame. No keep-resident
  policy exists; one is a product decision, not an engine one.
- The switch is a structural edit, one entry at a time: one registry
  re-derive per half plus a whole-project binding rebuild. Cheap at a
  switch, and allocation-free when no node asks.
- A newly loaded child is alive and bound before the tick that follows, but
  a shader's first render only asks for a compile window, so its first
  visual output arrives a tick later. This is why the owner holds a frame.
- A parent whose def names children the tree does not hold is now normal.
  Anything that asks "does the project have X" from the live tree alone is
  suspect; the panel-restore defect
  (`docs/defects/2026-09-25-panel-restore-drops-dormant-entry-knobs.md`)
  was the first case found.
- `LoadedProjectRuntime::tick` has no filesystem, so it does not apply
  residency; direct embedders call `apply_residency` or
  `tick_with_residency`. The product's edges never use it.

## Alternatives Considered

- **Resolver-driven wake-up** (the archived design): rejected (D6).
- **A residency level on `NodeEntryState`** (stub, built, compiled,
  playing): rejected in the vision. "Compiled" and "playing" are
  shader-specific and demand, not residency; and a node state keeps a node
  in the tree, which is exactly the per-entry cost being removed.
- **Engine-side dormancy only**, with the registry deriving every entry:
  rejected. Every edit would re-add them.
- **`make_only_resident` for a switch:** one registry re-derive instead of
  two. Rejected for now: the engine must remove before it attaches, and two
  calls make that order explicit.
- **Keep writers keyed by `NodeId` and re-key persistence:** the file
  already keys by persist path; only the in-memory store needed a stable key
  for the dormant window, which parking gives it.
- **Recognising an absent option by its error text** (as first landed):
  replaced. It matched a message, and it formatted that message every frame
  per playlist on the device.

## Follow-ups

- Preloading, thumbnails for dormant entries, fw-browser loading every
  entry, parents other than Playlist, and sending the dormancy reason over
  the wire are the vision's future work.
- The per-node overhead trim (D18) is its own plan.
- Two playlists in one module share one `playlist.cycle` / `playlist.skip`
  pair.
- An absent option read through a compiled def-view reader still formats a
  `NodeError` per read; only the by-path reads are allocation-free.
