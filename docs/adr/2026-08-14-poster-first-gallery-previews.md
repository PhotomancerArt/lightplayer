# ADR: Poster-first gallery previews — stable pictures at rest, motion on hover

- **Status:** Accepted
- **Date:** 2026-08-14
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Related:** `2026-07-16-preview-host.md`,
  `2026-07-16-primary-visual-product.md`,
  `2026-08-05-browser-sample-readback-is-async.md`,
  `2026-07-09-preview-fidelity-tiers.md`,
  `../debt/sidecar-preview-capture.md`

## Context

`PreviewHost` (the preview-host ADR) gave every gallery card a live,
continuously-rendering slot the moment it scrolled into view. On the
Explore page — eleven example cards, a two-worker pool — that meant every
card deployed at once: `start_pending_leases` started every pending lease
in one tick, budgets sized for a single lease expired under the queue, a
recovering slot destroys its canvas (`transferControlToOffscreen` is
one-shot), and a worker recycle blanked every co-resident card. The
result was the Explore-page flicker: cards decaying to `!` error badges,
whole rows going blank on a recycle, all from a page that never asked for
motion in the first place — a returning visitor does not need eleven
canvases ticking to see what a project looks like.

Prior art (Shadertoy, Pixelblaze) agrees: a gallery wall shows still
frames; motion is what you get by pointing at one. The preview-host ADR
had already reserved `PreviewProfile` as an empty, per-lease policy seam
for exactly this kind of per-project preview behavior — naming it there
meant this work extends the request instead of reshaping the service.

Three further facts shaped the design:

- **The frame tap already existed.** The root-module product-display work
  (`docs/adr/2026-07-16-primary-visual-product.md`, amended 2026-08-07)
  gave control-first projects a page-readable, per-lamp RGB frame that
  bypasses the preview canvas entirely — landed after the preview-host
  ADR listed "cached-frame static thumbnails" as out of scope for *that*
  ADR (not forbidden; the primary-visual-product ADR's amendment already
  anticipated a "save-time snapshot fallback"). This ADR is that
  follow-up.
- **The GPU tier's canvas cannot be read from the page.** GPU-tier
  presentation transfers the card's canvas into the worker via
  `transferControlToOffscreen`; `toDataURL` on it is impossible by
  construction. Any capture on that quadrant has to happen inside the
  worker.
- **Browser GPU readback is async.** `2026-08-05-browser-sample-readback-
  is-async.md` established the one-shot `map_async` pipeline for the
  per-LED sample path and left whole-texture readback native-only,
  "intentional." A poster capture needs exactly a whole-texture readback,
  once, on demand — not a per-frame concern, so it gets its own one-shot
  exit rather than reopening that gap generally.

## Decision

**A gallery card shows a stable picture instantly; live rendering is
spent only on demand.** Concretely:

### Display policy

Gallery cards (`example_card.rs`, `package_card.rs`) render `CardThumb`
in `ThumbMode::PosterFirst`: at rest they hold **no live slot at all**.
The docs hero keeps `ThumbMode::Live` — a single large canvas the reader
is actually watching is a deliberate motion spend, not a side effect of
load. `ThumbMode` is the whole policy surface; a future explicitly
"featured" card would opt back into `Live` the same way.

### `PreviewProfile.frame_budget` — the first field in the reserved seam

```rust
pub struct PreviewProfile {
    /// Present at most this many frames, then stop ticking the slot.
    pub frame_budget: Option<u32>,
}
```

A poster lease is a *producer of one image*, not a forever-running
preview: it asks for a small budget (3 presents — the GPU tier's lamp
samples trail the render by one frame per the async-readback ADR, and a
program's first frame is usually still black), waits for the budget to be
fully spent (`FrameSchedule::note_present` / `frame_budget_spent`, so
spent budget survives a re-lease `start()`), captures whatever the slot's
quadrant offers, and drops the handle. `None` (the default) is the
live-preview behavior every lease had before this field existed — the
docs hero, and any other `Live`-mode consumer, is unaffected.

### Bounded lease starts

`start_pending_leases` no longer starts every pending lease in one tick.
`slot_policy::choose_starts` bounds in-flight deploys to
`max_concurrent_deploys` (the pool size — one deploy per worker),
preferring visible candidates and then ascending slot id, so a page load
fills progressively instead of stampeding the pool. A slot not chosen
this tick stays pending and is reconsidered on the next one; nothing
starves.

### Capture sources, per tier × kind

| | control-first (lamps) | shader-only (raster) |
|---|---|---|
| **CPU tier** | rasterize the output frame (`thumb_poster::lamp_poster`) | read the live canvas back (`canvas_poster`, plain `putImageData` target) |
| **GPU tier** | rasterize the output frame (`lamp_poster`) | worker-side capture (`pixel_poster`, below) |

A control-first project's picture is its lamps, on both tiers alike: the
output frame (per-lamp U16 RGB + 2D geometry) is already carried home by
the host, so the lamp field is rasterized in Rust
(`lamp_view::rasterize_lamp_field`) rather than read from any canvas —
the poster and the live `LampView` layer draw the identical source, so a
hover swap (below) never jumps between two different pictures. A
shader-only project's raster canvas on the CPU tier is a plain
`putImageData` target and reads back directly. The one quadrant with **no
page-readable picture** is shader-only on the GPU tier, whose canvas the
page gave away.

### The worker poster-capture message

For that one quadrant, the host posts a new envelope,
`CapturePoster { runtime_id, channel, width, height, frame_id }`
(`lpa-link/src/providers/browser_worker/worker_envelope.rs`). The worker
renders the bus visual **once**, at the requested poster size, independent
of any attached surface and without ticking the clock, and reads the
texture back with a new one-shot async pipeline,
`read_back_texture_async` (`lp-gfx-wgpu/src/read_back.rs`): submit the
copy, then await the map on the worker's event loop — the same shape as
the sample pass's `map_async` pipeline, but a single on-demand capture
where the sample pass is a steady per-frame one. This is the **poster
exit** from the "GPU products stay GPU-resident, only the sample pass
reads back" doctrine: the 2026-08-05 ADR's "whole-texture readback stays
native-only, intentional" is amended by this one-shot path — it remains
true that there is no *per-frame* whole-texture browser readback; a
poster is explicitly not that.

The bytes come home as a transferable `poster_pixels` binary message
(surfaced as `PreviewPosterFrame`) on success, or a `poster_error` JSON
message on failure. `PreviewSlotHandle::request_poster_capture` /
`poster_frame` expose the round-trip idempotently — a repeated request
while one is pending is a no-op, and the request survives a worker
recycle unanswered until the next attempt. A poster failure never
condemns the slot or the worker; the card keeps its gradient placeholder.

### Session cache and reveal

Captured posters (PNG data URLs, `HtmlCanvasElement::to_data_url_with_type`
— zero new dependencies) are kept in a `thread_local!` `RefCell<VecDeque>`
cache (`thumb_poster.rs`), the same singleton shape as the host and the
library-host uid map, capped at 64 entries with insertion-order eviction
and in-place replace on a recapture. Data URLs, not blob URLs, so
eviction is a plain drop — nothing to revoke. A card resolves its cache
entry **at mount**, before any host boot, so a returning visit to
`/explore` paints instantly with zero leases. The poster is the third
layer of `CardThumb`'s stack (gradient base → poster `<img>` → live
canvas → lamp overlay), sitting **under** the live canvas so a lease
starting or ending can never blank the card — motion arrives and leaves
over a picture that is already there.

### Motion on hover

A poster-first card plays while the cursor is on it: a page-scoped
`HoveredCard` signal names at most one card (so at most one hover lease
exists, by construction), a 120 ms debounce keeps a cursor sweeping the
grid from deploying anything, and the lease is continuous (no frame
budget) over the intact poster. Hover is a courtesy: it writes no badge,
spends none of the at-rest recovery budget, waits for a poster capture
already in flight to finish first (one card never holds two slots), and
returns to the poster in silence on any failure, with its own small
bounded remount budget for the one *expected* failure (the poster lease's
GPU canvas was already consumed by transfer, so the first hover lease on
such a card remounts once by design).

### Recovery budgets meter progress, not lifetime

A card's error/remount budgets now reset whenever its lease actually
presents a frame. Without this, an innocent card sharing a worker with a
permanently-failing neighbour burned its whole remount budget on that
neighbour's recycles and parked on an error badge despite every one of
its own leases being healthy.

### Control-first fallback: outputs are the ground truth when the bus ties

A multi-module project (mini-dome, peach-1d, peach-2d — sibling modules,
not one authored scope) ties on `visual.out` bus resolution, and — a
G1-feedback discovery — ties on `control.out` resolution too, so
"control-first" cannot be decided from bus resolution alone for these
projects. Such projects are nonetheless genuinely control-first: their
lamps are the picture, and their outputs are published anyway (fragments
merge across sibling modules — the same reason they work correctly on
real hardware).

The engine-side ruling (Yona, G1 feedback): **a project's published
output frames are the ground truth**, ahead of bus resolution, for
deciding whether a project drives lamps. Concretely, in
`fw-browser/src/runtime.rs`:

- `render_bus_texture`'s resolution failure is a **cached state, not an
  error**, when the project also resolves `control.out` OR publishes a
  non-empty output frame: `present_bus_texture` becomes a no-op (cadence
  intact — present acks and frame budgets tick as if a frame had landed),
  `render_bus_texture_rgba8` (CPU-tier byte transport) serves opaque
  black under the lamp layer, and poster capture defers entirely to the
  lamp path (`begin_poster_capture` errors with a message the poster flow
  reads as "nothing to capture here," never a card-visible failure).
  Shader-only projects — resolution failure with **neither** signal —
  still error loudly; swallowing there would render black over a real
  defect.
- The **display verdict** (lamps vs. raster card) stays resolve-based
  when the visual side resolves normally, so a raster-led card with
  non-drawable lamps (e.g. 1D lamp layouts, not renderable by `LampView`)
  is not flipped into an empty lamp card. Published outputs break the tie
  **only** when the visual side already took the control-only fallback.

This fixed three examples that had never rendered in any gallery build,
and ended the present-error worker-recycle storms that were the original
flicker's root trigger (a failing multi-module card's repeated present
errors recycled its worker and blanked every co-resident card).
**Engine-level semantics — a true visual merge, or a root-scope
preference in bus resolution for genuinely ambiguous sibling ties —
remain open, tracked separately (chip `task_c50c3331`).** This ADR
records the preview-visible fix only.

## Alternatives considered

- **Keep every card live, just throttle fps.** Rejected: throttling
  reduces CPU cost but not the flicker mechanism (still N canvases racing
  N budgets on 2 workers) and does nothing for the "wall of motion is
  noisy" product objection.
- **Build-time committed poster PNGs for the embedded examples.**
  Rejected for this slice: the same one-frame producer is shared with the
  Projects page, where user projects cannot be pre-committed, and it
  doubles as the future sidecar producer (`../debt/sidecar-preview-
  capture.md`). Left as a later optimization for the compiled-in
  examples specifically.
- **`convertToBlob` / canvas-snapshot capture for the GPU-tier worker
  quadrant.** Rejected: the compositing-clear trap (a worker-side canvas
  snapshot races the browser's own paint/clear cycle) is avoided
  entirely by reading the render-product texture back directly, which
  also mirrors the sample pass's existing `map_async` shape instead of
  adding a second capture mechanism.
- **Persist posters to `SidecarMeta.preview_png` now.** Deferred: no
  persistence exists yet (local examples are `include_bytes!`, no remote
  Explore endpoint); the producer built here is deliberately the future
  sidecar producer (see the debt-doc update below), but wiring `put_blob`
  + save-time invalidation is separate work with its own trigger.
- **Engine-level fix for the sibling-module bus tie**, ahead of a preview
  workaround. Rejected for this plan: the preview-visible breakage (three
  examples never rendering, recycle storms) was blocking the whole
  poster-first slice, and the true merge semantics are a bigger,
  separable engine question now tracked as `task_c50c3331`.

## Consequences

- Explore and Projects cards are stable at rest with zero flicker;
  motion is deliberate (hover) rather than a load side effect.
- `PreviewProfile` now has a real field; future per-project preview
  behavior (auto input playback, audio sources, "featured" live cards)
  extends the same struct instead of reshaping the lease API.
- The worker gained a second capture path (`CapturePoster` alongside
  `PresentFrame`/`PreviewFrame`); it is additive — no wire version bump,
  since the worker JS and wasm still ship as one bundle.
- Stories gained a `static_poster` prop on `CardThumb` (following the
  `static_badge`/`static_lamps` pattern) so poster states are posable
  deterministically without a live slot; motion states remain
  intentionally un-posable in stories.
- The control-first fallback means a preview-only signal (published
  output frames) now participates in what would otherwise be a pure bus-
  resolution decision. That coupling is deliberate and scoped to the
  preview runtime (`fw-browser`); it does not change engine-side bus
  semantics, which is exactly why the underlying ambiguity is tracked
  separately rather than declared fixed.
- Remaining debt: session-only cache (no persistence — `../debt/sidecar-
  preview-capture.md`), and the evicted-while-visible resume defect
  (`preview_host_impl.rs` `evict_slot`/`resume_requested`), explicitly
  untouched by this work and tracked elsewhere (chip `task_dd4ed6ef`).

## Follow-ups

- Persist the poster producer into `SidecarMeta.preview_png` at save time
  before sharing is announced (`../debt/sidecar-preview-capture.md`).
- Engine-level resolution for the sibling-module bus tie (`task_c50c3331`).
- A main-page fully-animated hero and a logo-triangle live scene were
  both parked as future work on the `PreviewProfile` policy seam this
  ADR's `frame_budget` field opened; out of scope here.
