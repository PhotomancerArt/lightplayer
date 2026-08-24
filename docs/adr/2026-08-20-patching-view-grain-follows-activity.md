# ADR: The Patching view — grain follows activity

- **Status:** Accepted
- **Date:** 2026-08-20
- **Deciders:** Photomancer
- **Plan:** lp2025/2026-08-14-1115-patching-view (PR #436)
- **Supersedes:** None (extends `2026-08-13-one-project-canvas.md` and
  `2026-08-12-studio-workbench-panel-dock.md`)
- **Superseded by:** None

## Context

Patching — assigning mapped objects to places on output ports — lived
in an interim full-page `/patch` surface that bypassed the workbench,
reached by a header chrome toggle. The workbench Tree mixed grains by
dive state (a dived fixture showed authored objects while its
neighbours showed resolved instances with wire chips), the #411
selection pulse had no UI consumer, and the pulse itself was a 2 Hz
square blink. ~90% of lp2014's real-world clicks were this activity;
the interim page was scaffolding, never the home.

## Decision

**Patching is the workbench's third view** (`WorkbenchView::Patching`,
a `VIEWS` row labeled "Patch"), reached at `/p/<link>/patch` — short
and simple, ruled; `/patching` and `/map` parse as aliases that heal to
the canonical spellings. The chrome mode is retired: no Patch toggle,
no narrow-ladder fold rungs — the view band and the ≤820px summon strip
carry Patch at every width. Play remains a chrome zoom (revisit later).

**Grain follows activity, never dive state.** The Mapping view's Tree
is uniformly AUTHORED (objects, repeat interiors — dived through the
live session, undived parsed from loaded bodies); the Patching view's
Tree is uniformly RESOLVED (instances/ranges with their wire chips —
the chips are patching information and live only here). One selection
store (`UiPatchTarget`) serves both views; cross-view highlight is a
derived translation over the D46 path grammar (`/sector/2` → the
object with the matching sticky id), never a second store.

**The effective tree always displays; ids gate addresses, not rows.**
`ObjectInstanceSpan` carries its object index, so id-less documents
(old formats, imports) still expand for display — their rows carry
empty paths and select/pulse at range grain until ensure-ids stamps
the document. No address grain is invented that the patch format
cannot store, and no data age can collapse the Patch view.

**The patching center is the one canvas with patching furniture**: the
same `ProjectCanvasHost` (no dive), the #409 verb set as toolbar items
with printed keys plus the keyboard grammar, and live sprite colors
default-on (every fixture decodes its published frames — the walk-up
guide invariant). Port clicks in the Outputs dock carry the patch
grammar in this view only (armed swap completes; a fixture-side
selection assigns to the next free lamp). The Props stack gains
wire-side readout leaves (cell over port over output, deepest first).

**Selection drives the pulse** — the first UI consumer of #411's
`PatchPulseOp`: fixture-side subjects pulse in fixture numbering,
wire-side in wire numbering; deselection and view exit clear. The
pulse itself is now a raised-cosine BREATH (750 ms, dim floor to
37.5% crest, never dark) over a field dimmed to quarter power —
lp2014's field-proven selection language, replacing the square blink.

## Consequences

- The interim page, `PatchToggle`, `with_patch`, and the ⋯ menu's
  "Patch mode" row are deleted; the pure verb helpers survive in
  `app/patch/verb_ui.rs` and the byte-identical mini-dome e2e remains
  the parity oracle.
- The workbench's view-primary data model (#426's `VIEWS` table,
  map-keyed `PanelMemory`) proved out: the third view was ~7 edits in
  one file plus two shell gates. Panel content is now view-aware where
  it matters (the Tree's `TreeGrain`); panel homes stay fixed.
- Vocabulary: "universe" stays banned (DMX's fixed 512 makes it a lie
  at other lengths); other DMX borrowings are fine where they fit
  (footprint). "Segment" — WLED's own word — is the ratified name for
  a contiguous run on a port.
- The walk-up assignment flow is PASS 2, its design converged in
  `spikes/patching-controls/` (rounds 1–3): ONE patch panel (object
  section over output section), lamp strips that paint the light
  languages in the lamps (chase = blue head / red tail / sweeping dot,
  heads = clamp(1, 10, count/10); breath for segments), one selection
  where plain clicks never write and the explicit ASSIGN ARM (`a`, the
  swap-arm grammar) completes pairings, `m` advancing and keeping the
  arm. That pass adds the engine chase mode (ordered,
  direction-carrying spans in the highlight slot) and the `Segment`
  target. `docs/design/walk-up-patching.md` is the flow record.
- Pass 2 SHIPPED (2026-08-24). What it decided, including four rounds of
  gate rework on top of the description above, is its own ADR:
  `2026-08-20-walk-up-assignment-selection-model.md` (selection model,
  arm grammar, the per-fixture flow flag, the fixture card, object-grain
  canvas selection, one core-computed chase preview) — with the engine
  side amended into `2026-08-10-patch-selection-pulse.md`.

## Alternatives Considered

- **Patching as a Mapping mode** (the original unified-editor P6): one
  view with a mode switch. Rejected by the grain ruling — the two
  activities read different trees, and a mode toggle inside Mapping
  buried the activity the field work centers on.
- **Keeping the chrome toggle**: rejected at the #435 gate ("Patch and
  Play should probably be integrated into the View system").
- **Ids gating display** (the shipped pre-R2 behavior): rejected after
  the zook review — resolver knowledge was being withheld from the
  display for a storage concern.

## Follow-ups

- Pass 2 (own plan): the patch panel, `Segment` target, assign arm,
  engine chase mode, reactive lamp strip.
- Authored-grain wiring annotations in the dived Mapping editor
  (chipped: the per-instance arrows are noise and slow).
- Live values on foreign spans decode under the current fixture's lamp
  type (color order / RGBW) — the assumption is deliberate; revisit
  when per-port lamp-type data exists.
