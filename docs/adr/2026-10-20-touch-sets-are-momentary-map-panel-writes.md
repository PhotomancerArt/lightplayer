# ADR: Touch sets are momentary map panel writes

- **Status:** Accepted 2026-10-20. Yona ruled the three Lasting names
  (the `touches` channel, the Fluid `coords` key, the `fluid-draw` slug and
  its knobs) before implementation. The rest are the plan's leans, taken at
  plan approval.
- **Date:** 2026-10-20
- **Deciders:** Photomancer
- **Plan:** `lp2025/2026-10-20-0019-drawing-on-the-picture` (vision and
  notes beside it; PR #1219)
- **Supersedes:** None
- **Superseded by:** None

## Context

lp2014 let you drag on the picture and a fluid on the LEDs followed the
finger. LightPlayer had every piece but the input. A panel write could hold
one leaf `LpValue`, so a set of fingers had no way onto the bus: the engine
materialized every panel writer as a leaf, and a by-key reader failed
("merge by key expected map input"). The Fluid node ignored where the lamps
were: its grid stretched over the whole render request, letterbox margin
included. And Studio's live pictures took no pointer at all.

## Decision

1. **The value.** A touch set is a `u32`-keyed map of
   `lp::touch::Touch { id, pos }`, by pointer id. `pos` is in pattern space
   (ADR 2026-09-24): the lamps' box, centred, long side −1…1, y up. `id`
   repeats the key, because a shader's sentinel mapping finds an entry by
   its key field. Velocity and direction are not carried: the consumer
   derives them from each id's previous position. Pressure comes later, as
   an added field. This closes panel.md P-Q5.
2. **The channel** is the well-known `touches` (kind `Position2d`, the
   nearest fit; the kind is a picker label).
3. **The write.** A panel write's value is `PanelWriteValue`: a leaf
   `LpValue`, or a `u32 → LpValue` map. **A map rides a momentary write
   only** (`ttl_ms` set); the server refuses a latched one. So
   `.lp/panel.json` and `LpValue`, both persisted, are unchanged. The engine
   materializes a map writer as `SlotData::Map` (`BindingSource::LiteralMap`).
   A finger renews by writing again, and a dropped link releases within the
   ttl with no help from anyone. **A writer's route reads the store**
   (`BindingSource::PanelWriter`): engaging a writer changes the graph, and
   re-writing an engaged one does not. Before this, every panel write
   rebuilt the whole binding graph (~40 ms on the C6), which halved a
   drawing pattern's frame rate
   (`docs/defects/2026-10-20-every-panel-write-rebuilt-the-whole-graph.md`). Wire 44 → 45.
4. **The Fluid node in pattern space.** `"coords": "pattern"` on
   `fluid.json`, the shader def's key and values. The grid spans the lamp
   box, and emitters are read in pattern units, y up, so a stamp is round on
   the lamps. When the key is absent (or `"pixels"`), the node publishes
   byte-identical to before. The key rides wire 45.
5. **Drawable.** A connected board's live picture (the card, Play, the
   editor's root hero) is drawable when a loaded module scope has a
   `touches` reader. Studio writes into the innermost such scope, so a
   reader in the playing pattern wins over the root. It reads the binding
   graph the board already sends: nothing new on the wire. A watched card
   or a library card is never drawable.
6. **Who does what.** The web reports pointers as a fact
   (`StudioCommand::Stroke`, like `Place`): every live finger as 0..1 of the
   drawn picture box. Core owns the rest: the fit back to pattern space
   (`LampPictureFrame`, the one fit LampView also paints through), the ttl,
   the pacing (≤ 30 Hz, newest wins), renewal while a finger rests, the
   release, and the faster picture feed while drawing. The web builds no
   action, and a stroke is not an offer: it is not a verb, so the agent
   reads "drawable" and does not draw.
7. **The pattern.** `catalog/patterns/fluid-draw` does the gate, the idle
   "friends", the chord interpolation, velocity and hue in one compute
   shader, touches → `fluid.emitters`. Its knobs (Fade, Viscosity) sit on
   that shader, where an unwritten channel falls back to its default.
8. **Rule 2** (a touch plays a drawing entry as an excursion from a
   playlist that has nothing drawable) uses **no named trigger**: a
   touch→trigger node with an authored numeric id puts a new `seq` on every
   tick that has touches, so the entry's duration counts from the last
   touch. It waits on playlists-on-the-bus D7 (the return to the picked
   entry) and is its own PR.

## Alternatives considered

- **`LpValue::Map`.** It would have changed a persisted vocabulary
  (project files, panel.json, schemas) to carry something that is never
  persisted.
- **A "Draw" mode or a panel XY pad.** A mode is ceremony; the picture is
  where the finger already is. Drawable is a fact core derives, and the
  only sign is the crosshair and the rings.
- **Converting to pattern space in the pattern's shader.** Compute shaders
  get no scope or extent, so they cannot. The Fluid node sees the scope when
  it is sampled, so it can.
- **A named `touch` trigger.** `trigger_ids` are numbers, and a name would
  be a new persisted vocabulary for one use.
- **The web building `PanelWriteOp`s.** That would hide the stroke from core
  and its tests, and raise the web-built-action ratchet.

## Consequences

- One new persisted name per thing a user can copy: `touches` in a
  pattern's bindings, `coords` in a fluid.json, and the `fluid-draw`
  entry. Each is additive.
- Wire 45 marks older boards Incompatible until they are flashed, as every
  bump does.
- The fit between a picture and the lamps lives in one place. Note that
  LampView's picture is the lamp-centre box grown by 0.9 × the cell radius
  (about 0.83 of a lamp spacing on a dense field), so a finger at the
  picture's edge is just outside the lamps. That is by design.
- Found and pinned, not changed: in pixels mode the Fluid reads its dye
  back half a cell up and left of where it stamps it, and it reads a plain
  strip's one-lane points as pairs.
- A pattern's knobs on a compute shader (fluid-draw's Fade and Viscosity,
  meteor's Meteors and Speed) do not reach a card's panel: only shader,
  fixture and clock faces carry panel controls, and a compute shader has
  no face. The channels are public and writable; a face for compute
  shaders is follow-up work.
- Follow-ups: palette colour for strokes (compute shaders cannot sample a
  palette yet); the Fluid grid could take the lamp box's aspect by itself;
  compute shaders could receive `patternExtent`.
