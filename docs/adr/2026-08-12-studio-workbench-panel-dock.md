# The studio workbench — PanelDock chrome

- Status: accepted
- Date: 2026-08-12
- Plan: `2026-08-12-1112-studio-workbench-chrome`
- Design record: `spikes/studio-chrome/index.html` (rounds 1–3, ratified
  2026-08-12); precursor: `spikes/unified-editor-shell/index.html`
  (rounds 1–5)

## Context

The studio's project editor was a scrolling three-column document
(project pane · node-card workspace · device card), and the first
advanced editor (the unified mapping/patching editor,
`2026-08-11-2325-unified-editor-shell`) was planned as a parallel
full-page shell with its own tab row, its own rails, and its own tree.
Before implementation, Yona flagged the isolation: the app would have
grown two systems for the same things — two project trees, two homes
for output data, two layout systems — exactly when the mapping-centric
view is likely to become the main way people see LightPlayer.

A spike round explored the chrome itself, comparing three
pane-ownership models with the same two tenants (the node workspace
and the mapping editor at radiance density): Eclipse-style perspectives
(the active view owns the whole arrangement), VS Code-style documents
(the shell owns persistent side tool-views, documents own the center),
and free user arrangement (views movable between slots).

## Decision

The editor chrome is an IntelliJ-model **workbench**, mounted on the
project editor routes only (galleries untouched):

- **The shell owns two docks; views own only the center.** The center
  holds **view tabs** — routes, not modes: `/p/<id>` is the Nodes view,
  `/p/<id>/mapping` the Mapping view (`ProjectView` enum on
  `StudioRoute::Project`; same-session suffixes like `/play`). Future
  views (Live/Perform, Show, Files) are new tabs + defaults, no new
  chrome.
- **Four panels with fixed homes** — left: **Nodes** (the project pane;
  the whole workbench is the project) · **Fixtures**; right:
  **Device** · **Outputs**. Fixtures and Outputs are the project tree's
  "multi-node custom faces": derived slices over the patch-surface DTOs
  and node selection, not new data systems. The home table lives in
  code (`PanelId::side`) so experiments are a constant edit.
- **No user arrangement in v1** ("things have one home"): perspectives
  were rejected as disorienting — the room must never rearrange
  itself — and free arrangement rejected as machinery that substitutes
  for design. Customization stays a future option.
- **Radio per side, tabs when open, strip when closed:** at most one
  panel per dock; an expanded dock shows its side's panels as a
  horizontal tab row (active tab collapses the side), a collapsed side
  falls back to a vertical edge strip. One panel per side keeps the
  dense panels honest (the radiance-scale outputs rail never shares a
  column).
- **Per-view panel memory**, seeded by defaults (Nodes view → Nodes +
  Device; Mapping view → Fixtures + Outputs), ephemeral by design:
  the view helping, never the room rearranging.
- **Full-height frame, no box:** editor routes trade the scrolling
  document for a viewport-height frame whose docks and center scroll
  internally, separated by hairlines only. On mobile the workbench
  becomes one full-screen main view with a summon toolbar (the edge
  strips folded); a summoned panel temporarily replaces main.

## Consequences

- The unified-editor plan builds INTO this chrome (its P3 was amended
  to a workbench-integration phase): the arrange canvas mounts in the
  Mapping view's center, and its DTO/undo substrate grows the existing
  panels instead of building rails. The interim `/patch` page survives
  until that plan re-houses patching as a Mapping mode.
- The project pane's rich [i] detail moved to the workspace ROOT
  card's [i] (`ProjectDetailSections`, one component, no fork) — the
  docked pane renders flat, and the root card is the project's card.
- Panel visibility is lost on reload (one click to restore); promoting
  it to localStorage is deliberate future work, as is any status bar,
  header slimming, or arrangement customization.
- Mobile stack-and-scroll is gone on editor routes; any new panel must
  fit the summon model and a dock column, which constrains future
  panel design toward vertical layouts.

## Amended 2026-08-14 — one band, the Tree merge, the header chip

- Plan: `2026-08-14-0826-workbench-bar-chrome`
- Design record: `spikes/studio-chrome/index.html` rounds 4–6 (same
  spike file, later commits on its branch — the round-6 direction
  D7–D13 was ratified 2026-08-14); gate G1 passed on the live app the
  same day.

The PanelDock **model** above stands (fixed homes, radio per side,
per-view ephemeral memory, full-height frame). Its **presentation** is
superseded, and two of its decisions are amended:

- **One band replaces the three tab surfaces (D7).** The per-dock tab
  rows, the center's view-tab row, and the edge strips merge into ONE
  chrome band across the workbench top: each dock's tabs sit in a
  segment sized exactly to its dock (shared width constants), with the
  view tabs centered between them. Panel tabs wear the ATTACHED
  treatment — the active tab shares its dock's fill and breaks the
  band's bottom hairline — which answers the round-4 rejection
  ("toggles don't look connected to the panels").
- **Collapse leaves the tab row in place (D11).** Pressing the active
  tab still collapses its side, but the side's tab row PERSISTS with no
  active tab — the persistent row is the reopen affordance. The
  vertical edge strips and the «» hide chevrons are deleted; "tabs when
  open, strip when closed" is no longer true.
- **One panel, one ROLE — the view supplies the content (D10).** The
  `Nodes` and `Fixtures` panels merged into one left panel named
  **Tree**, whose body is keyed by `(panel, view)`: the project's node
  tree on the Nodes view, the fixture → object → instance tree on the
  Map view (today's mixed grain, deliberate until the R5 patching plan
  splits authored from effective). Rosters and defaults are data
  (`roster(view, side)` / `defaults(view)`) over a `VIEWS` table, so a
  new view is a table row plus roster arms. Panel-level furniture rides
  the panel, not a view's body: the Save/Revert row (`TreePanelActions`)
  renders on every Tree body, and Finder-style summary footers (D12)
  belong to the dock composition (`panel_footer`).
- **The project's identity moved to the site header (D8 — reversing
  this ADR's root-card ruling).** The root-card [i] mount is retired;
  a header PROJECT CHIP (state glyph + name + amber unsaved count,
  toned by the shared affordance vocabulary) opens the same
  `ProjectDetailSections` popup — the Google-Docs pattern: document
  state lives in the chrome, visible on every view at every width (the
  chip is one ungated mount, so no container-query double-mount). The
  non-workbench pane-header [i] survives unchanged.
- **View tab labels read `Nodes · Map` (D9)** — short sibling nouns; the
  `/mapping` route string is unchanged. The "panels live on view" open
  question is answered: they live on the VIEW via rosters, but content
  is view-supplied per role.
- **The fold moved 960px → 820px (G1 ruling).** md-width windows keep
  real docks — the docks only ever take a portion of the screen — and
  the full-view summon model now begins at genuinely narrow widths.

Follow-up recorded (G1): the header now holds both the project chip and
the lensed session's chip, which read as near-duplicates on a sim
session; Yona's direction is one grouped device+project cluster
(possibly absorbing Save/Revert). Tracked as a separate task, not part
of this change.

## Amended 2026-08-24 — the fold's tablet rung (summon sheet)

Flagged at the patching round-2 G1 gate: below the 820px fold a
summoned panel replaced the ENTIRE main view at every width — phone
behavior at tablet width ("at md-breakpoint the panels take up the
whole width of the view"). The summon model now has two rungs:

- **Phone (<560px** — the site chrome's phone rung**):** unchanged —
  the summoned panel replaces main under a "‹ back" header.
- **Tablet (560–820px):** the summoned panel is a side-anchored
  **sheet** at its dock's full desktop width (270/320), sliding over
  the canvas from the panel's home side (`PanelId::side` — Tree from
  the left, Device/Outputs/Props from the right). It wears the dock's
  fill with a hairline inner border and shadow; the canvas stays
  visible and live beside it, and the header's dismissal is a ✕. No
  scrim — a pick surface (the Patching object-first invitation) wants
  the canvas's waiting object in view while picking.

The summon strip, radio semantics, per-view memory, and the
dismiss-on-pick behavior are unchanged; the rungs are presentation
only (`summon_overlay_class` in `workbench/mod.rs`).
