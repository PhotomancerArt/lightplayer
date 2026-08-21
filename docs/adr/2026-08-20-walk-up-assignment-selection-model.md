# ADR: The walk-up assignment selection model

- **Status:** Accepted
- **Date:** 2026-08-20
- **Deciders:** Photomancer
- **Plan:** lp2025/2026-08-20-1826-patching-pass2-walkup-assignment (PR #439)
- **Supersedes:** None (extends
  `2026-08-20-patching-view-grain-follows-activity.md`, whose Consequences
  named this pass)
- **Superseded by:** None

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
