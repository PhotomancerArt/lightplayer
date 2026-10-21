---
status: fixed
found: 2026-10-20      # how: the drawing plan's emulated C6 measurement (P4): fluid-draw at 13 fps while a finger moved, 30 when it rested
fixed: 2026-10-20
area: lpc-engine `Engine::panel_write_value`, the resolver's panel-writer route
class: budget-exhaustion
related:
  - docs/adr/2026-07-31-resolver-persistent-resolution.md
  - docs/adr/2026-10-20-touch-sets-are-momentary-map-panel-writes.md
---
# Every panel write rebuilt the whole binding graph

**Symptom** — on the emulated C6 (`lp-emu:esp32c6:t2`), `fluid-draw` ran at
32 fps idle and **13 fps** while a scripted stroke wrote touches: every frame
that answered a write took ~75 ms of tick against ~35 ms without one. A
knob turn paid the same, but a knob is turned rarely enough that nobody saw
it; a finger writes ten or more times a second.

**Cause** — a panel writer's provider carried its held value as a
`BindingSource::Literal`, and literals are structural: the resolver caches
them across frames (ADR 2026-07-31). So every write had to call
`invalidate_structure()`, which drops every route, the intern table and the
static path memos, and the next frame re-derived the whole graph — about
40 ms on the C6.

**Fix** — the provider names the writer instead
(`BindingSource::PanelWriter { scope, channel }`), and its route reads the
held value from the store each time it resolves
(`ResolveHost::panel_writer_production`, stamped with the writer's own write
revision, so an untouched value still reads as unchanged). Only engaging a
writer invalidates; re-writing an engaged one changes no structure.
Clearing and despawning still invalidate. Pinned by
`re_writing_an_engaged_panel_writer_costs_no_structural_work`
(`lpc-engine/src/engine/scoped_resolution_tests.rs`): zero structural work
on a re-write frame, the new value read.
