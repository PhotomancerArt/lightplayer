---
status: open
found: 2026-08-14      # how: live-debugging (G1 gate re-verification, gallery preview)
fixed:                 # engine-level tie unresolved; preview-side symptom mitigated, see Fix
area: lpc-engine (bus resolution) + fw-browser preview runtime
class: state-conflation
related:
  - ../adr/2026-08-14-poster-first-gallery-previews.md
  - ../adr/2026-07-16-primary-visual-product.md
  - chip task_c50c3331 (engine-level semantics)
---
# Sibling-module bus ties read as "no product," not "these merge"

**Symptom** — Three real examples (mini-dome, peach-1d, peach-2d) never
rendered in any gallery build: every card decayed to a `!` error badge,
and the failing present errors repeatedly recycled their preview worker,
blanking every co-resident card on the same worker. The recycle storms
this produced were the root trigger of the original Explore-page flicker
this plan set out to fix — one bad multi-module card was enough to blank
its neighbours.

**Root cause** — `resolve_bus_visual_product` / `resolve_bus_control_product`
(`lp-core/lpc-engine/src/engine/engine.rs`) answer one question — "does
the root scope's channel resolve to a single value?" — and the
primary-visual-product ADR's tie-break (equal priority → registration
order decides) is scoped to bindings *within one authored scope*. A
project assembled from **sibling modules** — independent top-level
producers, not one scope's binding list — ties on `visual.out`, and (a
G1-feedback discovery) ties on `control.out` too: neither resolves to a
single value, so both calls return `Err`.

That resolution failure is legitimate for a genuinely shader-only project
("nothing publishes here"), but for a multi-module fixture-driving
project it means something entirely different: **every sibling module IS
publishing**, and their control-product fragments are designed to merge
(the same reason these projects drive real hardware correctly) — there
is no ambiguity about *whether* the project drives lamps, only about
which single resolved value a bus-lookup API can hand back. One `Err`
state is being asked to answer two different questions — "nothing here"
and "multiple legitimate producers, ask the outputs instead" — and every
caller that treated `Err` as the first meaning got the second one wrong.

**Fix (preview-side, this change)** — `fw-browser/src/runtime.rs` no
longer treats this resolution failure as an error when the project also
resolves `control.out` OR has already published a non-empty output
frame: `present_bus_texture` becomes a cadence-preserving no-op, CPU
frame delivery serves opaque black under the lamp layer, and poster
capture defers to the lamp path. The **display verdict** (lamps vs.
raster card) stays resolve-based when the visual side resolves normally,
so published outputs break the tie only when the visual side already
took this fallback — otherwise a raster-led card with non-drawable lamps
(1D layouts) would flip into an empty lamp card. Shader-only projects —
failure with *neither* signal — still error loudly. See
`docs/adr/2026-08-14-poster-first-gallery-previews.md` for the full
mechanism and commits (`d5e154aec`, `573914d86`).

This is a **preview-runtime workaround**, not a fix of the engine's bus
resolution: `resolve_bus_visual_product`/`resolve_bus_control_product`
still return `Err` for these projects; only `fw-browser`'s caller now
interprets that `Err` correctly for the control-first case. Any other
caller of these engine methods — today none besides the preview runtime
— would still get the wrong meaning.

**Regression coverage** — none automated for the engine-level tie (no
native repro landed in this change); the preview-side behavior was
verified manually (two consecutive Explore loads render all eleven
examples, zero worker recycles). A native repro exists on the discovery
branch and is the seed for chip `task_c50c3331`.

**Lesson** — A bus-resolution API whose contract is "one value or an
error" cannot, by construction, distinguish "no provider" from "multiple
legitimate providers that merge downstream" — both are "not one value."
Callers that need the second answer (a control-first preview deciding
whether to show lamps) must not treat the API's `Err` as a single fact;
this fix works by asking the outputs directly instead, which is honest
but means the true fix — an engine-level answer to "is this project
control-first," inclusive of the sibling-module case — is still owed.
Watch for the same shape wherever a "resolve to one value" API is asked
a "does anything provide this" question instead.
