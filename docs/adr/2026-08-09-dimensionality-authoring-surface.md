# ADR: Dimensionality authoring surface — factored projections, no deferring default, one writer per row

- **Status:** Accepted
- **Date:** 2026-08-09
- **Deciders:** Photomancer (Yona's gate rulings G1 / G1b / G2 and the
  follow-up chats recorded in
  `2026-08-08-0959-dimensionality-studio-surface/p4b-g1-rework.md`)
- **Supersedes:** None (amends `2026-08-07-two-sided-space-model.md`)
- **Superseded by:** None

## Context

`2026-08-07-two-sided-space-model.md` made space a declared, two-sided
fact: a shader declares `ShaderSpace` (`OneD`/`TwoD` carrying the answer
cell for the opposite dimension), a fixture declares
`strip_order_meaningful` plus a consumer policy, and the producer runs
the coordinate map at the sampling boundary. That ADR shipped the engine
core. Nothing was authorable: the slots rendered as raw rows in the
advanced drawer, which is where nobody reads them.

Yona's stated success criterion for the surface work was that the model
itself is on trial — *"the UX works as expected" is the evidence the
model is right*. So the surface was built, shown, and ruled on three
times, and the rulings ran backwards into the model twice. This ADR
records what survived, because most of the interesting decisions are the
ones that reversed something already shipped.

The design record — every ruling verbatim, in the order it was made,
including the reversals — is the planning directory
`2026-08-08-0959-dimensionality-studio-surface/` (`plan.md`,
`p4b-g1-rework.md`). Read `p4b-g1-rework.md` before changing any of this;
several plausible-looking simplifications were tried and rejected there
with reasons.

## Decision

### 1. One `dimensionality` drawer per card, producer and consumer mirrored

Both sides of the model get the same control shape in the same place: a
default-collapsed drawer labeled `dimensionality`, on the shader card
between `code` and `advanced` (it is authoring, and it belongs beside the
code), on the fixture card below `settings`. A D1 mismatch (declared
space vs. GLSL entry) forces the drawer open — an error folded away is an
error hidden.

The producer's drawer leads with a `1D | 2D` tab pair; beneath it, "show
in 2D by" (a 1D shader's answer). The consumer's drawer is one control:
"show 1D sources by", whose first choice is **along the wire** — the
`strip_order_meaningful` bit — followed by "follow the source" and the
explicit shapes.

### 2. Projection factors into shape × mirror × flip

The per-pair answer is a flat record —
`Project { shape: {ExtrudeX | ExtrudeY | Radial | Angular}, mirror, flip }`
— not a variant per projection with its own direction vocabulary.

The vocabulary the surface work grew organically (extrude with four
directions, mirror with four fold senses, radial in/out, angular cw/ccw)
is exactly this product: extrude's four directions are extrude-x/y × flip,
mirror's four folds are extrude-x/y × mirror × flip, radial in/out is
radial × flip, angular ↻/↺ is angular × flip. Sixteen meaningful states
from four tiles and two toggles, with angular × mirror and radial × mirror
falling out free — both of which someone asked for and neither of which
had been implementable.

A **flat record, not per-variant payloads**: switching shape keeps your
mirror and flip, which is what the control needs to do, and the UI maps
1:1 onto the model.

The engine is one uniform chain replacing the per-shape arms:

```text
t = shape_coord(shape, u, v)
if mirror { t = 1 − |2t − 1| }   // fold around the midpoint
if flip   { t = 1 − t }          // reverse the strip
```

### 3. The producer always declares — no deferring `Default`

`SpaceAnswer2` has no `Default` variant. A 1D shader's answer cell is
always a concrete `Project` record; a fresh one is plain extrude-x, which
is exactly what the old `Default` resolved to.

This is G1 ruling 11 taken all the way: *"'consumer decides' is wrong:
there should always be a producer-side default."* A choice that means
"I decline to answer, you decide" reads as an option among options and is
not one — it is the absence of the decision the control exists to
capture. Presenting it concretely at the UI layer was tried first
(cheaper, no format break) and it merely moved the lie.

Consequently the precedence ladder loses a rung: origins are `Declared`
or `Forced`, never `ConsumerDefault`, and the model can no longer express
a silent producer.

### 4. A section that claims a row owns every write to it

Rows the `dimensionality` section renders are *claimed* out of the
advanced raw-slot drawer, and every path that writes them — a tile click,
a guided Shape preset, an agent tool — dispatches the same slot ops. A
second spelling of "how a space declaration is written" is a defect, not
a convenience.

This is why the fixture's Shape presets write through a narrow declared
seam rather than a parallel path, and why the `declare_space` agent tool
is a follow-up rather than a quick addition: the cheap version of it is a
second writer.

### 5. Controls that gate each other are one control

`strip_order_meaningful` shipped as a checkbox beside the projection
dropdown, and the engine reads it as a *gate*: checked ⇒ wire-order
sampling ⇒ the projection never fires. Two sibling controls, one silently
disabling the other. It became the first choice of the one control
instead, with a `[forward | reversed]` row for the direction. Same two
slots, no model change; the surface stopped lying about what is live.

### 6. Glyphs derive from the transform, they are not drawn per case

One drawing function runs the chain from §2 over a ramp grid, so every
tile draws what that choice actually does — including combinations nobody
enumerated. The modifier rows are consequently *mutually reflective*: the
shape tiles redraw under the live mirror/flip, and each modifier row's two
faces show the current shape with that bit off vs. on. Reflectivity came
nearly free because the drawing shares the engine's semantics rather than
imitating them.

### 7. Format v9 and its migration

`PROJECT_FORMAT_VERSION` 8 → 9, `lpa-upgrade` step `v8_to_v9`, wire
`WIRE_PROTO_VERSION` 15 with the four firmware manifests in lockstep.
Yona's ruling on the cost: *"no one uses it yet, lets fix it right. its a
version bump, oh well."*

The migration's one non-obvious mapping: v8 `Mirror` becomes
`Project { ExtrudeX, mirror, flip }`, **both** modifiers. v8's mirror was
the outward fold (`|2x−1|`) and the factored `mirror` modifier alone is
the inward fold, so bit-identity requires the flip.

## Consequences

- **Format v9 is a break, and it is cheap** because no released version
  ever shipped a v8 projection cell. Format 8 landed 2026-08-08 (PR #381)
  and the projection vocabulary was still branch-local when the
  factorization ruled; the migration exists for correctness and for
  anyone's in-flight local projects, not for a shipped install base. A
  later equivalent ruling would not be this cheap.
- **The model cannot express "no opinion" anymore.** Code that read
  `Option<CellProjection>` as "producer declined" now reads it as "this
  is a 2D shader". The fill-the-silence precedence rung and the
  `ConsumerDefault` origin are gone from the engine, the wire, and the
  captions.
- **Every 1D shader's declaration is now load-bearing for what a 2D
  fixture shows.** The three WLED ports and the 1D pattern template all
  declare explicitly; there is no path where a shader stays silent and a
  consumer guesses.
- **The advanced drawer is thinner and honest**: `def.space`,
  `def.consume`, `def.strip_order_meaningful` and `def.wire_reversed` do
  not appear there, because the dimensionality section claims them.
- **Heap grew, deliberately**, and the budget ratchet was re-baselined
  in-commit each time (the `Project` record is three slots where the old
  variant was one; `wire_reversed` adds a fourth fixture slot).
- **`wire_reversed` is interim by construction.** The mapping/patching
  work's per-range `reversed` (that plan's slice 1) supersedes it, and
  its D15 declared-fixture-space may absorb `strip_order_meaningful`
  entirely. Both are documented as such at the field.
- **The 1D pattern template changed shape**: `New → 1D pattern project`
  now scaffolds `render_1d(float)` under a `OneD` declaration. The
  template's GLSL is not source-compatible with what earlier copies of it
  produced, which is fine — it is a scaffold, not an API.

## Alternatives Considered

- **Per-shape direction vocabularies** (`ProjectionDirection` on extrude,
  `MirrorDirection` folds, `RadialDirection`, `AngularDirection`).
  Implemented and shipped to a gate, then deleted. It grew a fifth
  vocabulary every time a shape was asked to do one more thing, could not
  express angular × mirror at all, and forced the UI to render a
  different control per shape. The factorization is the same expressive
  power with one vocabulary.
- **Keeping `Default` and presenting it concretely at the UI/DTO layer.**
  Shipped first, precisely to avoid a format break. It worked — and then
  the picker had two options ("extrude · default" and "extrude") that did
  the same thing, which is what made the model problem visible. Rejected:
  presentation cannot fix a model that encodes a non-answer as an answer.
- **Tiles inside the enum dropdown** (the original D16 direction, ratified
  at G1). Rejected on use: a collapsed drawer containing a dropdown that
  expands into a tile grid is two nested expansions to reach one choice.
  The grid renders inline in the section body instead, and the anchored
  popover left this surface entirely (its open-render drift class is
  filed as `docs/debt/anchored-popover-open-render-drift.md`).
- **Checkbox modifiers.** Shipped, then amended: *"very small and
  non-visual compared to the projection… the big cards just work so much
  better visually."* Each modifier is a two-card single-select row in the
  shape row's exact treatment. `mirror`/`flip` are two-variant *mode*
  enums rather than bools so future fold refinements stay additive.
- **Folding the space controls into a renamed `definition` section**
  beside the code (G1b candidate A, built as a story for side-by-side
  judging). Rejected in favor of a sibling drawer in the existing drawer
  stack — the drawer stack is already the card's authoring grammar.
- **Deriving space from the GLSL signature**, rejected in the parent ADR
  and re-confirmed here: the declaration must be visible to the node
  layer, the UI, and the registry, and the *mismatch* between declaration
  and signature is a diagnostic worth having.
- **A wizard step for the fixture Shape moment.** Rejected (vision D13):
  the moment belongs to the card, so it happens on every create path —
  "+ fixture", paste, wizard — instead of only the guided one.

## Follow-ups

- **`declare_space` agent tool.** The Studio shader agent's tools are
  `iterate` / `upsert_param` / `speed`; none writes `ShaderDef::space`, so
  an agent can stage a `render_1d` body onto a `TwoD` node and be unable
  to repair the mismatch it created. The system prompt also asserts the 2D
  entry unconditionally, which is false on a `OneD` shader. Must reuse the
  section's write path per §4.
- **`dimensionality` as the section name** is Yona's stated preference
  with a noted reservation ("a bit long"); `dimension` / `geometry` remain
  a one-const swap at `SPACE_SECTION_LABEL`.
- **Paste-guidedness is a heuristic** (no mapping and height ≠ 1) because
  no model-level "declared" marker exists; the mapping/patching work's
  D15 declared fixture space retires it.
- **Multi-entry `Native` answer cells**, the explicit projection node,
  palette-side declaration, 3D cells — all still parked in
  `docs/future/2026-08-07-dimensionality-follow-ups.md`.
