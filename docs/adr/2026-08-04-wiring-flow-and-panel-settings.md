# The wiring drawer is a flow view; PANEL names the module surface, SETTINGS the leaf strip

- Status: accepted
- Date: 2026-08-04
- Context: executes the two design passes docs/design/modules.md §10
  registered at the close of the modules-vision push ("wiring-drawer
  redesign" and "CONTROLS-vs-PANEL nomenclature"). The design record is
  the wiring-UI spike `spikes/wiring-ui/index.html` (three gates,
  2026-08-03) — the spike playground stays in-repo as the reference.
  Amends the presentation half of the relocate-don't-redesign decision
  (DY4) that moved the bus pane's rows onto the module card; the row
  *shape* that decision preserved is now replaced.
- Plan: `planning/2026-08-03-2210-wiring-ui-impl` (external planning
  root).

## Decision

### 1. Bus-as-writers/readers is drawn as a horizontal flow

One row per channel of the card's scope:
`[writer chips] → [value box] → [reader chips]`, wires drawn with
arrowheads. The value box is a `SlotPane` (violet header: channel name,
PRIMARY/kind badges; the detail popup stays one click away) whose body
shows *what's on the channel* — a live `ProductPreview` for visual
products, fixed decimals plus a position bar for unit floats, the
authored-default invitation for R6 channels, an error tone for
unresolved ones.

**Wire and chip color are the same signal** (gate feedback): violet =
the write actually driving the channel; orange = an engaged panel
writer (R10, matching the engaged-knob family); grey = readers and
out-ranked writers. E3 contention draws no winner — the box border and
a "2× fallback" badge carry the ambiguity in attention orange, display
only (the pick gesture stays future work, modules.md §5). Child-scope
readers — consumers in descendant module scopes whose reads resolve
here under R5 — list as chips with their scope path (spike gate 3;
playlist subtrees never surface, R2). A one-line key sits at the
drawer's foot. No dashed/dotted encodings: text marks and the popup
carry origin/scope semantics (gate feedback — dashes read as noise).

**Narrow containers stack** (`@container` below the `@md` width):
writers become a tree — trunk, elbow branches, an arrowhead dropping
onto the box — with the reader tree mirrored below, per-chip
arrowheads, and a lead-in gap. Same DOM, two layouts.

Geometry is deterministic everywhere: chips are fixed-height and
ellipsize, cells center on the row axis, so every connector SVG renders
from site counts alone — no DOM measurement, no resize listeners.

The view is fed by the enriched bus-view projection
(`UiBusSiteOrigin`, publish/shadowed/contended flags, child-scope
readers, `UiBusChannelPreview`), all derived client-side from the
existing binding-graph probe — no wire changes.

### 2. PANEL is the module's section; leaf cards say SETTINGS

The workspace section labeled `panel` appears on **module** cards only,
wearing the `panel-primary` tint and a ▶ rail icon — the teaching
device: the panel is *the thing play mode renders*. A **leaf** card's
own bound-slot strip is labeled `settings` (formerly `controls`): its
knobs configure that one node, while the same publicity simultaneously
surfaces as controls on the enclosing module's panel (one control, two
views — panel.md P1). The model is unchanged: every node still *has* a
panel (R8's derivation); the *label* is reserved for the module/product
surface so the two readings stop colliding.

"Control" remains the word for one channel's presentation on a panel
(panel.md P1). Internal identifiers (`UiNodeFace::Controls`, panel
control types) are not renamed by this decision.

## Alternatives considered (spike record)

- **Keep the value-hero panes** (the relocated bus-pane rows): rejected
  at gate 1 — a wiring surface whose wiring is invisible until a popup.
- **Value cards** (value hero + writer/reader lists): strong monitor,
  but resolution isn't drawn; its value treatment moved into the flow's
  box instead.
- **Vertical console strips**: fights the card's width; horizontal
  scroll hides the contended channel.
- **Node × channel matrix**: scales best, reads coldest; a possible
  later second view, never the default.
- **Node-graph editor**: real but deliberately later.
- **Naming A, "panel everywhere"**: one word for what reads as two
  concepts — the confusion being reported.
- **Naming E, "panel + controls, taught visually"**: zero rename cost,
  but "controls" keeps its collision with the `control.out` channel and
  stays vague about whose controls they are.
- **Dashed borders/wires for default-origin and child-scope**: built,
  then deleted at the gate — the encodings carried no meaning to the
  reader; text marks do.

## Consequences

- modules.md R8 gains the naming note; §5's status reflects the flow
  view; §10's two register entries close. glossary.md gains
  **Settings** and updates **Panel** / **Wiring drawer**.
- `control.out` naming a *product channel* is an unrelated third sense
  of "control"; it keeps its name until bus vocabulary lock-down (§7) —
  rename the channel then (e.g. `fixture.out`), not the UI words now.
- The E3 pick gesture remains the drawer's one dead end: the badge
  shows ambiguity but cannot resolve it (registered, modules.md §5).
- Selection-as-a-concept (click a site → workspace selection) was
  trialed in the spike and parked; chips keep dispatching focus
  navigation (D7) as before.
