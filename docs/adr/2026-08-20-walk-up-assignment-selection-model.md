# ADR: The walk-up assignment selection model

- **Status:** Accepted
- **Date:** 2026-08-20
- **Deciders:** Photomancer
- **Plan:** lp2025/2026-08-20-1826-patching-pass2-walkup-assignment (PR #439)
- **Supersedes:** None (extends
  `2026-08-20-patching-view-grain-follows-activity.md`, whose Consequences
  named this pass)
- **Superseded by:** None (extended 2026-08-27 by
  `2026-08-27-one-selection-one-tree.md`: "one selection" now spans the
  Mapping dive too — `UiPatchTarget` generalizes to the sibling-set
  `UiSelection`, plain-clicks-never-write and the arm grammar are
  untouched, an arm requires exactly one end, and the object-grain
  canvas pick (Q10) is preserved as Patching's view policy)

## Context

Pass 1 gave patching a home (the workbench Patching view, one canvas, one
`UiPatchTarget` selection, the #411 breath pulse). It did not give it the
ASSIGNMENT gesture: a port click on the Outputs dock auto-assigned the
fixture-side selection to the port's next free lamp. That made looking
dangerous — a click meant to check "which lamps is this?" wrote a patch —
and it had no counterpart in the other direction (you could not start from
free wire space and say "this chunk is that object").

The field activity this surface exists for is a person standing at a dome
at 2am with a phone: click a chunk of wire, see it light up, look up, see
which physical piece is dark or breathing, tap it, move on. lp2014 proved
the loop; its cost was a user-typed chunk number and no verification that
the pairing was right.

The design converged over three gate-ruled rounds in
`spikes/patching-controls/index.html` (§1 the panel; §2 the two flows; §3
the ruling matrix; §4 the reactive strip). The rulings were ratified in the
pass-1 planning notes (`2026-08-14-1115-patching-view/notes.md`, sections
"R3 SPIKE ROUNDS 2–3 RULINGS" and "R3 SPIKE BRIEF"), and the flow record is
`docs/design/walk-up-patching.md`.

## Decision

**One selection.** The surface keeps exactly one `UiPatchTarget`, core-owned
and shared by the canvas, the docks and the panel. A selection is either a
fixture-side object (`Fixture` / `Instance` / `Range` / `Cell`) or a
wire-side place (`Output` / `Port` / `Segment`). There is no "source and
destination" pair of selections, and no second store.

**Plain clicks NEVER write.** Every click — a sprite, a tree row, a port
row, free port space — only ever selects. Pass 1's auto-assign on port
click is deleted. Looking is free in both directions, always.

**Linking is an explicit ARM.** `ArmedVerb { Assign | Swap }` lives beside
the one selection (frame-scoped signal, not in the selection store):
`a` (or the panel's invitation button) arms an assign, `s` a swap. The
next counterpart click COMPLETES the link as one real, undoable write
through the existing `PatchVerbOp` path — there is no tentative lane and no
preview state to reconcile. `Assign` carries no payload: both ends resolve
at completion from the live selection plus the thing clicked, because the
selection deliberately moves under a live arm. Only the two ends of a link
can arm: an unmapped object or a free segment. Mapped things always
plain-reselect, so a click on something already on a wire can never steal a
pending link. Esc is a ladder — rung 1 disarms, rung 2 deselects.

**`Segment` is the wire-side unit for FREE space** — `{ node, port, start,
lamps }` in wire numbering, WLED's own word ("universe" stays banned). A
click on free port space draws one, auto-sized to the next unmapped
object's lamp count (lp2014's typed chunk number, taken over by the
surface). `[`/`]` walk it inside its own free run and `-`/`=` narrow/widen
it — selection nudges, never doc writes, because a window is not a patch
until the arm completes. A resize CREATES a size override that later
segments keep. A MAPPED run is not a segment: it keeps selecting as its
`Cell`, which already names it and speaks the fixture's language.

**`m` advances and keeps the arm** (D3): it scans the selected output's
ports in order from the current position, wrapping once, sized by the
override or the next unmapped object. No output hop in v1. Keeping the arm
is what makes the walk-up loop one key and one click per object.

**Pickers direct-assign.** The panel's inline pickers (the object side's
"which object", the output side's "which port") write on pick, through the
same verb path the armed click uses. They are the ratified tight/mobile
path — no popover dependency, no separate write path.

**The panel is the surface's one control room** (D8): always present as the
Patching center's bottom region, OBJECT section over OUTPUT section, empty
states included. Which section wears the selection edge follows core's D9
space matrix (`UiPatchTarget::pulse_space`) rather than a panel-local rule,
so panel emphasis and lamp language can never disagree. The section without
a counterpart carries the invitation. The keys row is the panel's footer,
replacing the pass-1 help overlay; the toolbar keeps only history and
status (D4) because the verbs now sit beside the thing they act on.

**A1 — the decode assumption.** Live wire values shown on a port strip are
decoded under the CURRENT fixture's lamp type: the owner's fixture when the
segment is mapped, otherwise the next-unmapped object's (the fixture that
sized the segment). Ports carry no per-port lamp-type data today; the
alternative is showing nothing. The panel states the assumption on the
strip's own line rather than implying certainty.

**Honest sprites (Q2).** The canvas does not fake the chase: mapped sprites
show the chase because the engine painted it into the published frame
before the bytes ever reached the client; unmapped objects stay dark with
their selection ring. No client-side canvas animation is invented for
lamps the wire is not driving. A ghost treatment (stroke-only head/tail
ticks) is deferred to a G1 finding — it ships only if walking the loop
shows the direction cue is missed.

## Amendment 2026-08-21 (G1): the flow flag makes the model reachable

The G1 gate found the model's precondition missing: **nothing could be
unmapped.** A patch is sparse anchors over auto-flow, so `Clear` returned
an object to the flow and the flow re-placed it (past the wire's end when
the ports were full). Every state this ADR describes on the unmapped
side — the invitations, the armed pairing, the chase on an object with no
wire — was unreachable in real data.

Ruled: auto-flow is a **per-fixture flag**, not a law of the model.
`{stem}.patch.json` gains `"flow": "auto" | "manual"` (patch format 3;
absent field and absent file both mean `auto`, so no existing document
changes meaning). Under `manual`, only authored entries place and unnamed
lamps are on no wire. Two verbs join the set through the same
`PatchVerbOp` path — `SetFlow { manual }` (fixture grain, one undo step)
and `UnmapAll` (delete every entry, one write, no confirm) — and the panel
shows the flag as a fixture-level fact on the object section.

Consequences for this model specifically:

- A **fixture-grain click** (a canvas sprite, a Tree fixture row) now
  resolves its assign subject to the fixture's next object still waiting
  for a wire — the same object the free segment was SIZED for. Sprites
  are honest (D2): the canvas can only name a fixture, and a whole-fixture
  subject is not something `assign` can place.
- A fixture whose lamps reach NO wire still appears on the patch surface
  (an empty row carrying its own lamp count). Before the flag,
  "no runs" meant "not a patchable thing"; now it is the state whose
  objects the user most needs to click.
- The flag's home is the patch file, not `map2d` (shape is not wiring
  policy) and not a def slot (two wiring truths, and `lpc_model` serde is
  the flash lever). Per-object tombstones were rejected outright: wrong
  grain for what is fixture identity, absurd at dome scale.

The three cases this serves are recorded in
[`docs/design/use-cases/`](../design/use-cases/README.md) — scarf (auto's
home turf), sign ("not mapped = not lit" is the progress bar), dome-scale
(never auto-mapped, re-wired every build). Creation-time defaults are
deferred to a real-hardware walk of those three; a resolve-time heuristic
is banned, as it would flip existing documented fixtures dark.

## Consequences

- The arm is the only write gesture on this surface besides the transport
  verbs, and every completion is one undo step because it reuses
  `PatchVerbOp` — no parallel write path exists to keep consistent.
- Any surface that can be clicked in the Patching view must route through
  the shared completion helpers (`complete_assign_on_object` for the
  fixture side, the Outputs panel's port-click grammar for the wire side);
  a new clickable surface that writes directly would reintroduce exactly
  the pass-1 hazard.
- A selection that goes stale under a live arm (its object gets mapped, its
  port disappears) refuses at completion rather than guessing: the arm is
  spent by the click either way, and the click falls through to plain
  selection.
- The panel derives every fact from the selection plus the surface DTOs;
  it holds no selection state, so stories can pose any state by posing the
  selection alone.
- Free-segment nudges are selection state, which means they are lost on
  deselect — deliberate: the size override belongs to the segment it was
  nudged on, and the next segment sizes itself off the next object again.
- "Segment" now has a precise meaning (free wire window) distinct from
  "cell" (a mapped run). Copy that blurs them will read as a bug.

## Alternatives Considered

- **Plain-click assign** (pass 1's shipped behavior): rejected — it makes
  verification dangerous and gives the wire side no way to start a link.
- **A tentative/pending lane** (arm writes a draft the user confirms):
  rejected at the spike gate — a second state to reconcile, and undo
  already gives the walk-up user the escape hatch immediately.
- **Two selections (source + destination)**: rejected — the panel would
  have to explain which one a verb acts on, and every surface would need
  two highlight languages at once.
- **The arm capturing its counterpart at arming time**: rejected — `m`
  moves the selection under a live arm, so a captured end goes stale on the
  second lap of the loop.
- **Panel as a dock or a floating HUD** (spike concepts B and C): rejected
  round 2 — dock width forces permanent gradient strips and splits the two
  halves of one assignment; a HUD covers the sprites being checked.

## Follow-ups

- P5 fills the strip containers with live lamps and the chase/breath
  languages, and makes the decode line name the layout it actually used.
- G1 (feel gate) decides the Q2 ghost treatment and the panel's height on
  desktop/mobile.
- The lamps−/+ control is mock-level room only; the count edit is a mapping
  write, and it needs its own decision.
- P6 cross-references this ADR from
  `2026-08-20-patching-view-grain-follows-activity.md`'s Consequences.

## Amendment — 2026-08-23: G1 round 2 (Q7–Q11)

The feel gate's second walk ruled five changes. They do not replace the
selection model above; they change what a selection SHOWS, who computes
the light, and what a click on the canvas names.

### One chase, computed core-side (Q9) — supersedes the honest-sprites clause

D2 ("honest sprites") said the unmapped chase lives only in the panel
strip, because the strip is a readout and the canvas is a picture of the
piece. That clause is **superseded**, in Yona's own words at the gate: a
panel-only chase "implies we're not driving those views from the same
data — that should be generated server side. very fishy". The objection
is about PROVENANCE, not about pixels: two views of one object were being
painted from two computations, and only one of them was the engine's.

So the unmapped chase is computed ONCE, in `lpa-studio-core`, and shipped
on the surface DTO (`UiPatchSurface::chase_preview` —
`app/project/patch_preview.rs`). The panel strip paints it; the canvas
sprites paint the same colors through the existing `live_fills`
direct-DOM feed. No client-side chase renderer survives: the web crate's
`chase_colors` / `use_chase_phase` are deleted, and the panel keeps no
clock at all.

- **The numbers are shared, not copied.** The language's constants and
  functions moved to `lpc_model::nodes::output::chase` — head/tail hues,
  `head_lamps`, sweep period, the dot window, body floor/crest. The
  ENGINE imports them for `paint_chase`; the controller imports them for
  the preview. Two copies of these constants was the exact smell this
  ruling kills, and an engine test now asserts the wire's samples equal
  `chase::lamp_rgb_16` lamp for lamp.
- **Colors travel in the engine's space** (16-bit linear unorm), so every
  client renders them through the same linear → sRGB transfer it already
  decodes published frames with. The mapped chase and the unmapped one
  land in the same greys by construction.
- **The clock is the lens's frame counter**
  (`OutputFrameCache::frames_seen`), not a wall clock. The engine states
  the sweep in seconds because it holds a frame clock; the controller
  does not, and counting published frames buys the property story capture
  requires: with no frames flowing the preview FREEZES, in ONE place
  (`preview_phase`), rather than each view freezing itself. A repeated
  frame revision is not a frame, so a patch edit that re-cuts the wire
  without republishing does not animate anything.
- The MAPPED path is untouched: published bytes remain the one truth
  there, and byte-identity when the highlight slot is empty still holds.

### The fixture card (Q8)

A whole-fixture selection is **not an object**, and the panel stops
pretending it is. `UiPatchTarget::Fixture` renders a FIXTURE CARD:
fixture name, lamp/object/placed counts, the flow selector, and — in
manual mode — `unmap all`. No chase strip, no object transport, not
armable, not assignable. The canvas selection ring and the engine-side
pulse are unchanged: a wire-side breath of the fixture's mapped runs is
still an honest answer to "which lamps are this fixture".

EXCEPTION: a fixture with no sub-objects (the count-only strand — the
scarf) **is** its own object. It keeps the object treatment, including
the chase and the arm, and wears the flow selector on that row instead.

P5b's `assign_subject_target` fixture→next-unmapped resolution survives
only for that scarf case and for the pickers. A fixture ROW click under a
live arm now refuses and disarms, like any other nonsense pair.

### Object-grain canvas selection (Q10)

Canvas sprites already carried true lamp indexes for the live-fill feed
(`data-sprite-lamp`, stride-corrected). A sprite click now resolves the
nearest drawn lamp to its TRUE index (`nearest_lamp`), and the shell maps
that lamp to the object span containing it (`sprite_target`), emitting an
`Instance` — or a `Range` for id-less documents, following the same
addressability rule every other surface uses.

- Armed completion on a canvas click therefore targets the object the
  user pointed at, which is strictly stronger than P5b's
  next-unmapped guess: a click says WHICH.
- Fixture-grain selection stays reachable from the tree row.
- Two honest fallbacks: a body that draws no lamps (placeholder, strip)
  names no lamp, and a lamp no span covers falls back to the fixture
  rather than guessing.
- Display subsampling does NOT lose the grain: drawn point `i` stands for
  true lamp `i * stride`, so what comes back is always a real lamp of the
  fixture's own document. Only every k-th lamp is reachable, so an object
  shorter than the stride cannot be clicked — at MAX_DISPLAY_LAMPS =
  2000 such an object is sub-pixel on screen anyway, and the tree row
  still names it.

### Mode-gated grammar (Q11)

An AUTO-mapped fixture reflows its own unnamed lamps on every resolve, so
every gesture that pins one of its objects to a wire would be fought by
the next frame. Auto fixtures therefore get a LEAN panel:

- object rows show facts and the strip only — no transport, no
  invitations, no arm affordances;
- `is_armable` gains the mode check, so `a` does not arm on an auto
  object and a click on one cannot complete an armed assign;
- `next_unmapped_lamps` and the invitation pickers consider MANUAL
  fixtures' objects only, so a free segment is never sized by an object
  that was never waiting;
- the flow SELECTOR (Q7) is what unlocks the grammar, and it lives on the
  fixture card — one click away from any of its objects.

The Outputs panel's free-run click targets are unchanged: they are
port-side, and an auto fixture nearby changes nothing about them.

### The flow control is a selector, not a toggle (Q7)

`flow: manual` was a bare toggle button — it named one state and left the
user to guess the other. It becomes an EXPLAINING selector: label
"mapping", two always-visible cards with icon, title and a line of
consequence — **auto-mapped** ("objects place themselves along the wire —
just works") and **manual** ("only what you patch lights up — unmapped
stays dark"). Picking dispatches the same undoable `SetFlow` verb.

The card-picker pattern existed inline in the node face's space section
(the shape/modifier tiles, ruled at G1b: inline tiles, no popover, no
dropdown; selected = accent border + accent wash + check badge) but had
never been extracted. It now lives in `base::option_cards` with the
smallest honest API — options of `{id, icon, title, blurb}`. The space
section keeps its own component (its faces are projection drawings and
each tile dispatches a slot-op sequence) and now shares the STYLING from
that module, so there is one visual language rather than two copies of
it.


## Amendment 2026-08-22 — chase is direction, breath is identity (G1 round 3)

Round 3 of the feel gate walked the panel against mini-dome and caught
the light language mumbling: selecting a FIXTURE ran the chase across
every one of its runs, so a five-sector dome read as "two objects
selected" — a blue head on one sector, a red tail on another, and no
direction claim worth making in between.

The matrix sharpens (D9 amended):

| selection | language | it answers |
|---|---|---|
| object (instance / range / cell) | CHASE | "which way does it run?" |
| fixture | BREATH | "which lamps are this fixture?" |
| output / port / free segment | BREATH | "which lamps are this wire?" |

**Chase is an OBJECT-ONLY language.** Direction is a property of one
contiguous run in object order; a fixture is a bag of objects and has no
single direction to claim. A fixture selection now breathes all its
mapped lamps — identity, not direction — which is also what the fixture
CARD (Q8) already said in chrome: fixture grain gets fixture-grain
answers. A richer fixture-level canvas indicator stays future work.

Presentation followed in the same round (recorded here for the trail,
not as decisions of ADR weight): objects render as clickable HULL bodies
on the canvas (round 3: "they should feel like individual THINGS not
collections of hard to target tiny things" — the whole hull is the hit
target, with the nearest lamp as the overlap tiebreak); hotkeys became
kbd chips; an armed button pulses instead of changing its label, and the
COUNTERPART section wears an attention ring; the port pickers carry
occupancy; and at the mobile fold the object-first invitation summons
the Outputs panel as the picker, which dismisses itself after the pick
(the general fold rule: a summoned panel closes once a selection or
write happens inside it).
