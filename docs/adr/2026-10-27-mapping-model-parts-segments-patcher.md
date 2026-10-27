# ADR: The mapping model: parts, segments, the patcher, and map2d format 6

- **Status:** Accepted (format gate G1 answered by Yona, 2026-10-27;
  planning `lp2025/2026-10-27-0114-mapping-rebuild`, M1)
- **Date:** 2026-10-27
- **Deciders:** Yona (G1), Photomancer
- **Supersedes:** None. The map2d ≤ 5 document, the patch document (≤ 3)
  and their resolvers stay until M3 (project format 12) moves every project
  onto format 6 and the engine onto the patcher.
- **Superseded by:** None
- **Related:** `2026-08-04-project-format-migration-architecture.md` (the
  upgrade chain M3 extends), `2026-07-28-esp32c6-flash-budget.md`,
  `2026-07-06-sans-io-core.md`. Planning inputs: the theory
  (`lp2025/2026-10-20-0853-wire-picker-spike/theory.md`), the settled words
  (`lp2025/2026-10-26-1001-mapping-domain-names/names.html`, draft 3), the
  lab spike's findings F1–F18
  (`lp2025/2026-10-25-1857-component-object-patching-spike/findings.md`),
  and the roadmap (`lp2025/2026-10-27-0114-mapping-rebuild/plan.md`).

## Context

A fixture's mapping was two documents and two ideas. The layout
(`*.map2d.json`, format ≤ 5) listed objects whose order was the wiring
order; the patch (`*.patch.json`, format ≤ 3) placed lamp ranges or object
paths (`/id/k`) on an output's wire, and every lamp no row claimed
auto-flowed after the highest placed one, across fixtures on the same
output. That could not say what installations needed: a ring of a circular
display on another port and running the other way; one copy of a dome's
sector moved to a spare port while the others keep their jacks; a run fed
from its far end. The lab spike (PR #1310) built a walker that could, and
named what it proved (F1–F18). Studio's Map, Patch and Graph tabs each
held a different model.

M1 builds one model and one patcher in `lpc-mapping` (no_std + alloc) for
the engine, the firmware and Studio alike, and freezes its file, map2d
format 6. Its shapes were put to Yona at G1 with examples; this ADR records
what was decided.

## Decision

### 1. Mapping = layout + patching, in one model

`lpc_mapping::part::Mapping` is one fixture's mapping: its **parts** (the
layout) and its **patching** (the fixture's start and flow, each part's own
start and flow, and overrides by node id), plus the layout facts
`sample_diameter` and `canvas`. `part::render` turns the parts into the
rendered **tree**; `patching::patch_mapping` walks it and gives every
segment its **address** (output, port, channel). Nothing outside the crate
calls either yet: the engine moves onto them in M2, Studio's Graph in M4.

### 2. Parts render a tree whose leaves are segments

- **Parts** are shapes (path, polygon, circle, grid, filled polygon,
  points), groups (parts held together; never part of an id) and repeats
  (copies of one part turned about a centre). A repeat holds a part with
  its own id (F6). Copies are generated parts, never stored.
- **Segments** are the leaves: one run of lamps on one port, the only thing
  that holds lamps. One segment never has two addresses (D3). A part decides
  how it splits: a path is one segment, or one per subpath
  (`"split": "subpaths"`, keys `s1`, `s2`… by the subpath's place in the
  drawing); a circle is one segment per ring, or one for all its rings
  (`"split": "one"`); the other shapes are one each.
- **Geometry is today's arithmetic.** A segment's lamps come from today's
  resolver itself (each shape as a one-object document through
  `resolve_into`), so every layout renders exactly the lamps `resolve()`
  gives today, bit for bit: proven over the corpus, every shape kind, and
  every layout in `catalog/`, `projects/` and `lab/mapping/projects/`
  (121 documents), through format 6 and back.

### 3. Ids: flat, never reused

A part's id is `[a-z][a-z0-9_-]*` (today's object-id rule), unique in its
fixture. New ones are minted from the kind's word and a number (`path3`,
`circle2`); the counters are saved (`"next": {"path": 4}`) and only go up,
so a deleted id is never minted again and an override kept for it can never
quietly point at a new part (D6, D12). Any node's id is flat: the part,
the key the part generated (`circle1/r3`, `path1/s2`), and the repeat
copies it sits in, innermost first, trailing zeros dropped (`panel@2`,
`x@0@3`; F5). Grouping is never in an id.

### 4. One circle part; the centre is a ring of radius 0

G1: one circle part, not a circle beside a ring set. Today's `ring` shape
(concentric rings on one run) is a circle split `"one"`, its rings at
today's radii and counts (`Circle::from_ring_set`), every lamp where it was.

Each ring's lamps are today's single-ring arithmetic: lamp 0 at the start
angle on the ring's radius. **A ring of radius 0 is the centre** and holds
one lamp, at the circle's centre. This departs from the roadmap's D7 / the
spike's F7 ("a ring of one lamp sits at the centre, whatever its radius"):
under that rule a ring set whose derived inner ring has one lamp (an outer
count of 4 over 4 rings gives 4, 3, 2, 1) would move a lamp when it becomes
a circle, and the director's instruction was to keep every document's lamps
where they are. With radius 0 as the centre, the single centre lamp still
works (Yona: "the rings should still handle" it), the rule is the same in
either split, and a one-lamp ring on a radius sits where every ring's lamp
0 sits. The lab already saves its centre lamp as a ring of radius 0 (F7's
amendment).

### 5. The patcher's rules

The patcher walks one fixture's tree depth first with a **cursor**
(output, port, channel); it never sees another fixture, so nothing flows
across fixtures.

- **One step per field** (F8): entering a node, each of output, port and
  channel takes exactly one step: the node's override for that field; else
  its part's own start (in every copy of the part); else what its parent's
  flow renders for it; else Keep. A step is Keep, Set, or Add; amounts keep
  the unit they were written in (lamps or channels) and are converted at
  the segment they apply to.
- **Rules are generators** (D4): a flow (`end_to_end`, or `one_per_port`
  from a port, absolute) renders starts for the children; the patcher sees
  starts only.
- **The first segment after a port or channel Set is held to it** (F9): if
  it doesn't fit what is left of its port it stays, is reported, and lands
  as far as the port reaches; any other segment that doesn't fit wraps to
  the next port at channel 0.
- **Past the last port** (F10) a segment and everything after it that only
  follows lands nowhere, reported once, until a port is Set again. Outputs
  have no order: nothing ever runs into another output.
- **Off** (F11) lays nothing and doesn't move the cursor; what follows takes
  its place.
- **Direction** (F14) on a part is the default for the segments under it;
  a segment's own is absolute. **Offset** shifts a run's lamps within it,
  wrapping.
- **Leaving a node never moves the cursor.** Only a step does.
- **Overrides are by id and never retargeted** (D12): one whose node is
  gone is kept and reported.
- **Problems are reported, never guessed:** no start, a gone node, output
  or port, a segment that doesn't fit or falls past the last port, a start
  off a lamp boundary, two segments sharing channels, an order that doesn't
  fit its colors. Each names what it is about.

### 6. Map2d format 6: one file

As G1 decided (Yona's answers, 2026-10-27, in the plan's `notes.md`):

| G1 question | Answer |
|---|---|
| One file or two | **One file.** `*.map2d.json` format 6 holds the parts and the patching; patch files retire at M3 |
| Paths | **SVG path data** in `d` ("leverage existing formats"). This build reads `M`, `L` and (as lines) `H`, `V`, absolute or relative, and writes absolute; `Z`, curves and arcs are refused by name until built, so adding them is not a format change |
| Starts | **Per-field ops with units**: `"start": {"output": {"set": "main"}, "port": {"set": 0}, "channel": {"set": {"lamps": 0}}}`; `{"add": 1}`; amounts `{"lamps": n}` or `{"channels": n}`; an absent field is Keep. Stored as ops, not named rules: "At" and "Next port" are presets that write ops |
| Outputs | **Named in each op, no outputs list** ("good thought"): no output follows another, so a list's order would mean nothing |
| Overrides | **An object keyed by node id**, written sorted: `"overrides": {"circle1/r3": {"port": {"set": 1}, "direction": "reverse"}}` |
| Explicit keep | **Yes**: in an override, `"keep"` holds the cursor whatever the flow says, where an absent field takes what the node would get |
| Part-wide direction | **Left out for now**: direction lives in overrides; on a part's node it covers that copy only |
| Shapes | **One circle** (§4); **`points`** (lamps at listed positions, one segment); **`split: "subpaths"`**; **`off`** |
| Lamp facts | **None in format 6** ("don't freeze what we don't know yet"; director's ruling DD12). The in-memory lamp reference stays (RGB, the order from the fixture); the lamp-model plan adds saved fields with its own bump |
| Starts and flows on parts | On the part (G1 lean, unchallenged): a part's `start` applies to every copy of it, an override to one node |
| Counters | `next` at the top, `next_ring` on each circle (G1 lean) |

The circular display as soldered:

```json
{
  "format": 6,
  "next": { "circle": 2 },
  "start": { "output": { "set": "main" }, "port": { "set": 0 },
             "channel": { "set": { "lamps": 0 } } },
  "parts": [ { "id": "circle1", "name": "display", "kind": { "circle": {
    "center": [50.0, 50.0], "start_angle_deg": -90.0, "dir": "cw",
    "drawn_from": "outside", "next_ring": 6, "rings": [
      { "key": "r1", "radius": 42.0, "count": 24 },
      { "key": "r2", "radius": 32.0, "count": 16 },
      { "key": "r3", "radius": 22.0, "count": 12 },
      { "key": "r4", "radius": 12.0, "count": 8 },
      { "key": "r5", "radius": 0.0, "count": 1 } ] } } } ],
  "overrides": { "circle1/r3": { "port": { "set": 1 },
    "channel": { "set": { "lamps": 0 } }, "direction": "reverse" } }
}
```

Reading and writing (`lpc_mapping::map2d6::{read, write, peek_format}`):

- **The format is peeked first**, as `Map2dDoc::from_json` does: a newer
  document is refused as newer ("this build reads up to 6"). Formats 1–5
  are sent back to today's reader, which is untouched and still refuses 6;
  `MAP2D_FORMAT` stays 5 until M3.
- **Refused by name:** a bad, marked (`@`, `/`) or repeated part id; a
  counter word that is no kind; a counter that would mint an existing id
  (an absent counter is 1); a bad or repeated ring key, or a `next_ring`
  below one; an override keyed by text that is not a node id; a repeat over
  64 copies; path data that isn't, or a command not built. An override
  naming a well-formed id with no node is read and kept (the patcher's
  problem, D12).
- **One canonical form:** pretty JSON, fixed field order, defaults left
  out, overrides sorted, numbers as the shortest text that reads back to
  the same `f32`. Read and written back, a document is the same bytes. The
  writer raises a counter below an existing minted id rather than save it,
  and refuses lamp facts, a flow-only port step, and non-finite numbers.
- **Schema evolution** is today's: unknown fields are ignored; a construct
  an older build would misread needs a new format.
- **Firmware serde rule** kept: every enum is externally tagged; no `tag`,
  `untagged` or `flatten`. The document layer has its own structs; the
  polygon, grid and filled polygon reuse today's, whose deserializers every
  firmware already links.

### 7. What the engine refuses (M2)

What the model can say but no output needs yet is refused plainly, never
approximated (D2): a start part-way into a lamp (the patcher gives the
address and reports it; the engine refuses the segment), colors other than
RGB, and path commands not built (the reader's refusal).

## Consequences

- **Format 6 is persisted** the moment M3 writes it into projects; from
  then it is read by firmware in the field for years and changes only by
  the schema-evolution rules above.
- **Sizes.** Zook's dome (the scale a microcontroller drives) is 969 B in
  format 6 (one per port) against 1,080 B today, and 1,612 B with an
  override on every copy: far under the device's 32 KB read trigger. The
  radiance dome was not measured: it will never run on a microcontroller
  (G1).
- **The C6 image** (measured at M1's end, split image, against the merge
  base): no symbol of `lpc_mapping::{part, patching, map2d6}` in the ELF;
  the same 111 `lpc_mapping` symbols; core unchanged in size (25,888 B to
  its next 32 KiB page); engine +20 B (1,860,656 → 1,860,676 B), gated
  headroom 236,496 → 236,476 B. The 20 B are one input section,
  `drop_in_place<Box<lps_glsl::hir::HirBuildFunctionState>>` (312 → 332 B),
  with renumbered outlined functions around it: code generation moved
  under a larger crate, not new code. Within DD13's bar (~10 KB on the C6).
- **M2** reads format 6 in the engine beside today's path; **M3** bumps the
  project format (12), upgrades every project, retires patch files and the
  old path; **M4** moves Studio's Graph onto this model and deletes the Map
  and Patch tabs.

## Alternatives Considered

- **Two files (layout 6 + patch 4).** Flows name port keys and starts name
  outputs, so either the layout points into installation facts or the patch
  points at part ids across a file boundary. Rejected at G1.
- **Points + jumper indices** (today's `points` and `gaps`). Rejected for
  SVG path data, which already says a jumper (a move) and leaves room for
  curves and import with no format change.
- **An ordered outputs list.** Its jobs (row compression, the default
  output) are gone, and an order of outputs means nothing when nothing
  runs into the next output. Rejected at G1.
- **Overrides as rows** (patch format 3's compact form). Rows were chosen
  for the radiance dome's size; the target is a microcontroller's map, and
  zook's is small either way. Rejected at G1 for an object keyed by id.
- **A circle beside a ring set.** Two circles for one idea, and a ring set
  saved as rings would lose its overrides (F17). Rejected at G1.
- **A ring of one lamp at the centre whatever its radius** (D7 / F7).
  Rejected in M1 P3 (§4): it moves lamps of existing ring sets.
- **Lamp fields now** (`colors`, `order`, a lamp spec's name). Deferred to
  the lamp-model plan's own bump (G1).
- **Named start rules** (At, Next port) in the file. They are presets; the
  file stores the ops they write.

## Follow-ups

- M2: the engine's side, an amendment to this ADR (the contract between
  fixture and output, port tables, refusals): written, below.
- M3: the upgrade, an amendment (the freeze into starts and overrides, the
  refusals, `PathPoints` fixtures into `points` parts).
- M4: Studio's side, an amendment; "Add centre lamp" adds a ring of
  radius 0.

## Amendment (2026-10-27): the engine's side

Mapping rebuild M2 (PRs #1314 and #1319; planning
`lp2025/2026-10-27-0114-mapping-rebuild`, `notes.md` "M2 planning",
Q36–Q51). What the engine does with a format-6 map2d, until M3 moves every
project onto it:

- **Two paths, chosen by the document's format (Q3).** The loader and the
  asset refresh read a fixture's map2d with today's reader first, unchanged;
  only a document it refuses as newer goes to `lpc_mapping::map2d6::read`
  (`nodes/fixture/map2d_document.rs`). A format 1–5 document costs exactly
  what it did (a peek before every read was one allocation and 8 B per
  fixture, caught by the engine heap ratchet). A newer document is refused
  naming 6. A format-6 fixture whose def also names a patch document is
  refused by name: one file holds the patching (G1, Q14).
- **The fixture patches itself and publishes runs that name their output
  (Q36).** The output is unchanged: it filters the fixture's `PatchedRun`s
  by its own name, as it does a patch document's. The output cannot take
  the patcher's `SegmentPatch`es because the output node is never gated
  out while `lpc-mapping` can be (`node-fixture`); and the patcher needs
  the whole fixture under one cursor, so it cannot run per output.
- **No default output, no auto-flow (Q37).** A format-6 fixture is always
  manual; with no start it lights nothing and says so.
- **Port tables come from the authored output defs through the same walk
  that cuts the wires** (`node::output_port_slices`, Q38), offered only for
  named outputs that read the fixture's bus channel. Capacity in channels
  is the port's samples (`count × 3`; RGB only). An address becomes a wire
  position as the port's first sample plus the channel (Q39). An unnamed
  output is not offered (Q40).
- **Refusals are per segment (Q41):** a start part-way into a lamp, colors
  other than RGB, an order other than the fixture's colour order. The rest
  light where the patcher put them, and a refused segment keeps its room on
  the port. Path commands not built are the reader's refusal and fail the
  load (keep-last-good on a live edit).
- **Problems are one status line (Q42):** the first problem in words, then
  "(and N more)". No wire change: Studio computes the full patching itself
  (M4).
- **A patching-only edit renders nothing (Q46).** A re-read mapping whose
  layout is the same (`Mapping::same_layout`: everything `render` reads)
  keeps the carrier and the mapping version; only the patching is done
  again. A document edited across formats switches the fixture's path.
- **What stays resident (Q43, M2 P6).** The mapping and the runs. The tree
  is rendered for each patch and dropped with it, and the patching is made
  without its walk record (`patching::patch_mapping_placed`): at device
  width the walk was most of a first frame's transient (a 512-lamp,
  64-segment fixture's largest allocation 29,184 B), and the resident tree a
  node and a segment, each owning its id, per segment (+13,480 B retained
  at load for that fixture). Interned ids (P6 step 3) were not done: they
  change `TreeId`, which Studio's edit operations (M4) build on.
- **Cost (fw-emu at device width; the C6 split image).** The format-6
  twins against their format-5 twins, and the image's growth against
  `main` with M2a, are in PR #1319. Both crossed the plan's bars (lean Q44
  for heap, Q45's 32 KB for the engine), which went to the director with
  the numbers rather than being tuned toward them.
