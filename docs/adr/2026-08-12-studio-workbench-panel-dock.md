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
