---
status: carried
since: 2026-08-05      # G1 of the view/edit-split round: visually smooth, ~50ms frames
logged: 2026-08-05
area: lpa-studio-web / lpa-studio-core — dev-profile frame cost at dome scale
related:
  - ../adr/2026-08-05-map2d-editor-selection-tree-model.md
---
# Dome-scale dev frames sit near 50ms, not 16ms

**Shape** — with 1500 lamps live, the dev-build Studio renders *visually
smooth* but a profile shows ~50ms frames. Accepted at the 2026-08-05 G1
gate ("we can probably punt but long term we're going to need better
solutions"). Dev-build feel is a product signal on this project (the
reference machine is an M2 Max; users run release on weaker hardware),
so this is carried, not closed.

What the 2026-08-05T16:03 trace establishes (even without reliable wasm
symbolization — the binary was rebuilt under the trace):

- The wire-parse whale is gone: per-message post-message cost fell
  ~12.4ms → ~4.2ms after the 857d85a90 opt-level/Rc/memo fixes, and the
  render-side whales (per-frame div/VDOM rebuilds) fell to #347's
  follow-up round (LampView canvas; editor live colors as direct DOM
  writes).
- ~31% of samples are wasm-bindgen-futures task plumbing and ~26% one
  unidentified wasm function. The strongest suspect for the latter is
  the per-tick view-DTO rebuild in `lpa-studio-core`, which is still an
  **opt-level-0 workspace crate** — it is deliberately NOT in the
  `[profile.dev.package.*]` override list because O2 would tax the
  studio iteration loop (it is the most-edited crate). That trade is a
  product-owner call.

## Candidate levers, cheapest first

1. Re-profile with a build that matches the trace (capture + symbolize
   in one sitting) and confirm the 26% suspect before spending anything.
2. `[profile.dev.package.lpa-studio-core] opt-level = 2` (and possibly
   `lpa-client`) — likely the single biggest win if (1) confirms;
   costs studio-core edit-compile time.
3. Thin the per-tick DTO rebuild (dirty-flag or product-keyed reuse so
   an unchanged card rebuilds nothing) — the structural fix, sized like
   a small plan.
4. WebGL/instanced LampView backend — the documented upgrade path;
   keyed to Radiance (~30k) scale, not this.

## Exit criteria

Dome-scale dev frames at or under ~16ms on the reference machine, or an
explicit decision that N ms is the accepted dev bar.
