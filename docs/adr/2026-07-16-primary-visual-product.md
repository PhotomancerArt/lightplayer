# ADR: Primary Visual Product — the Project's Face Is a Bus Convention

- **Status:** Accepted
- **Date:** 2026-07-16
- **Deciders:** Photomancer
- **Supersedes:** the informal `channel.name == "visual.out"` check in
  `ProjectController::ui_bus_view` (the bus pane's PRIMARY badge)
- **Superseded by:** None

## Context

Several surfaces need one answer to "what do I render to represent this
project?": the Studio editor's default preview, the bus pane's PRIMARY
badge, home-gallery card imagery (the GPU live-gallery plan's
PreviewHost and its static snapshot fallback), and any future headless
thumbnailer. Today the answer is folklore: the default visual-output
policy targets `bus:visual.out` (ADR 2026-07-09), fixtures sample it,
and the bus pane hardcodes the channel name for its badge — a
coincidence of defaults, not a contract. Consumers that re-derive
"which product is the face" will disagree at the edges (multiple
providers, no provider, provider that fails to resolve).

## Decision

**The project's primary visual is the resolved value of
`bus:visual.out`: the product produced by the channel's
highest-priority provider.**

- The channel name is a constant in the well-known registry
  (`lpc-model/src/bus/well_known.rs`, `PRIMARY_VISUAL_CHANNEL`) — the
  one place the name is written.
- **Resolution is the engine's, not the client's.** The binding-graph
  probe (ADR 2026-07-06) already resolves each channel's value by
  provider priority; for `visual.out` that value is a
  `LpValue::Product(ProductRef::Visual(..))`. Clients read the resolved
  answer instead of re-deriving it from provider lists —
  `ProjectController::primary_visual_product()` is the client-side
  helper, resolved from the cached graph with **no new wire surface**.
- **Determinism:**
  - No provider, unresolvable value, or a non-visual product → `None`:
    the defined empty state (consumers show their placeholder; nothing
    is invented).
  - Multiple providers → binding priority (authored > default, per the
    existing `BindingPriority` ordering).
  - Equal priority → the binding index's registration order decides,
    which the graph probe reports deterministically
    (`Reverse(priority)`, then `BindingRef`). This tie-break is now
    contract, not accident.
- `control.out` is the analogous convention for control-first projects.
  It is declared here for symmetry and left unconsumed: no
  control-product preview surface exists yet.

## Consumers

- **Bus pane PRIMARY badge** — same constant, same meaning.
- **Studio always-live preview** — the primary product is subscribed
  whenever a project is open (independent of node focus), so the
  project's face is always streaming somewhere stable.
- **Gallery card imagery** — the GPU live-gallery plan's PreviewHost
  presents the primary product per leased card; its save-time snapshot
  fallback captures the same product. (See
  `Planning/lp2025/2026-07-16-gpu-live-gallery-cards/`.)
- Future: headless thumbnailers, device-side "what's playing" surfaces.

## Consequences

- Exactly one definition of "the project's face"; surfaces cannot
  drift.
- Projects whose primary output is intentionally not `visual.out`
  (control-first installations) have no visual face by definition —
  their preview surfaces show the empty state until a `control.out`
  preview story exists.
- The helper is only as fresh as the cached binding graph; consumers
  needing synchronous freshness after an edit should treat `None` /
  stale as "keep the previous frame", never as an error.
