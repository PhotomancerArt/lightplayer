# EditorMeta and the unified editor

- Status: accepted
- Date: 2026-08-13
- Plan: `2026-08-11-2325-unified-editor-shell` (PR #415)
- Design records: `spikes/unified-editor-shell/index.html` (rounds 1–5),
  `spikes/studio-chrome/index.html` (the workbench redirect),
  `docs/design/walk-up-patching.md` (pass 2, ratified)

## Context

Mapping needed a project-scope home: the fixture-face embedded editor
edited one document in a card, the standalone `/mapping` page edited
files outside any project, and the interim `/patch` page held the #409
patching machinery on its own route. Each was a partial answer; together
they were three places doing one job, none of them showing the project's
fixtures as one space. The unified-editor plan was redirected mid-flight
into the workbench chrome (see the PanelDock ADR) and then re-shaped
twice at its review gates by Yona — this ADR records where the decisions
landed.

## Decision

**`editor.json` — a project-level editor document.** Arrange-canvas
placement (per node, per *surface* key: `"mapping"` now, future views get
their own) lives in ONE `editor.json` beside `project.json`, owned by
`lpc-mapping` (`EditorMetaDoc`): format 1, version-and-refuse, unknown
surface keys preserved on rewrite, canonical one-node-per-line pretty
form, floats quantized to 4 decimals (serde_json's fast-path parse is
one-ulp lossy on long mantissas; canonical bytes must round-trip).
Chosen over a slot in node defs because the device parses defs strictly
and `lpc_model`+serde is the flash lever — editor presentation data must
never cost device schema, flash, or a format bump. **It is never a
sampling input**: the engine and device do not read it. Keys are authored
address paths (runtime `NodeId`s never persist); a renamed node losing
its placement is accepted editor-cache staleness. An absent file is a
normal state (settled by a directory listing, never a read error) and
reads as the empty document.

**One editor, in place.** The workbench Mapping view's center IS the
editor: an Arrange canvas of every fixture's own resolved geometry placed
by its `editor.json` transform (translate + rotate + uniform scale, no
shear), with unarranged fixtures auto-packed in a sticky bottom row,
unloaded fixtures as footprint blocks, and map2d-less fixtures as range
strips. Double-clicking a fixture DIVES in place — the mapping editor's
session mounts with every other fixture rendered dimmed inside the same
canvas at its true arranged position (a render-only `ContextFixture`
layer; `lpa-mapping-editor` stays project-unaware). The fixture-face
embedded editor and the standalone `/mapping` page were **deleted**
("this path is clearly the superior direction, and I don't want the
needs of those 2nd-class uses to restrict our work"). One wheel grammar
everywhere, by shared code: scroll pans, ⌘/Ctrl-scroll zooms.

**Unified panels, Figma-shaped.** The editor's object-list rail and
floating properties popover were deleted; the workbench's Fixtures panel
grows the dive's object tree, and a new Props panel (right dock, Mapping
view only — dock strips are per-view data) carries the re-housed
`ObjectPropertiesPane`. One `MapEditorSession`, workbench-owned, drives
canvas, tree, and props; props commits ride a bump counter into the
session host's echo-suppressed apply pipeline so undo history survives
them exactly like canvas edits.

**Undo is mode-scoped on a correlation substrate.** ⌘Z routes to the
focused session while diving and to the arrange byte-stack otherwise
(patch verbs keep their own stack). Every undo step on every controller
stack carries `(edit_seq, node, mode)` from one session-global monotonic
sequence, and node/mode switches journal into the same bounded ring —
correlation substrate only, deliberately not global replay.

**Patching gets its own view later.** The tab-is-mode idea died twice:
first to the workbench (views are routes, 1:1 with URLs), then at the
gates to the panel argument — mapping wants fixture-tree + object-props,
patching wants fixtures + outputs; different furniture means a different
view, while the canvas stays the same code. The interim `/patch` page
survives untouched until that view lands (with the parked #409 verbs,
pulse, and show-visual).

## Consequences

- Arrange edits are `EditorMetaOp` writes through the normal asset
  overlay (two-sided byte snapshots; drag overrides held until the
  snapshot echoes, so a slow round-trip never snaps a fixture back).
- The patch kernel enforces one-object-one-contiguous-window at resolve:
  `resolve_patch` returns a `PatchResolution` whose `refusals` carry
  per-entry `DuplicatePath` degrades (see the object-ids ADR's amendment
  note); the verb layer blocks documents that would degrade.
- Two canvas implementations still exist under the one experience
  (arrange SVG + editor SVG sharing grammar, math, and session); merging
  them into one project-space canvas — diving with no component swap and
  no camera jump — is the ruled follow-up, planned separately, with the
  patching view built on the merged result.
- Viewport rotation / "snap viewport to fixture" / isolation mode are
  registered future affordances (editing through the arrange transform is
  accepted for now).

## Amendment (2026-08-28): node keys are PROJECT-RELATIVE addresses

The decision above says "Keys are authored address paths (runtime
`NodeId`s never persist)". True as far as it goes, and the
implementation took it literally: it stored the whole runtime address,
root segment included. That segment names **the host's mount, not the
project** — the same project is `/preview.show/…` while previewed from
the gallery, `/<uid>.show/…` once saved to the library, and something
else again under a test harness. So a placement silently stopped
applying the moment a project was copied, saved, or opened from a
different surface, and an arrangement could never be *authored into* an
example: nothing a generator writes can guess the reader's mount.

Found by shipping one. `examples/small-dome` needs its door fixture
overlaid on the dome's plan rather than tiled beside it, which means the
example ships an `editor.json` — and the first one matched nothing at
runtime (the map read "0 arranged" against a document naming both
fixtures).

**Keys are the address with its root segment stripped**
(`/dome.module/dome.fixture`), which is exactly the part that identifies
a node *within* the project and survives every copy. `editor_meta_node_key`
normalizes on write, `editor_meta_surface` reads it and falls back to a
legacy ROOTED key with the same tail, so documents written before this
rule keep working and true up on their next write. The rest of the
decision is unchanged: still per-node, per-surface, still never a
sampling input, still refuse-don't-rewrite on an unreadable document.

Consequence worth naming: a generated example may now ship authored
placements, which is what makes structure-faithful multi-fixture
examples (a dome and its door in one plan) possible at all.
