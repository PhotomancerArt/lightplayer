---
status: retired
since: 2026-08-05      # honest-device-preview G1 walk surfaced it as "not tracked"
logged: 2026-08-05
retired: 2026-08-07
area: lp-gfx (browser GPU tier) / lpa-server preview host — control-product sampling
related:
  - ../adr/2026-07-16-preview-host.md
  - ../adr/2026-06-26-control-product-preview-probes.md
  - ../adr/2026-08-05-browser-sample-readback-is-async.md
---

> **Retired 2026-08-07.** This file was logged hours before its own fix
> merged: the async one-frame-latency `sample_rgba16` pipeline (fix
> direction 1 below) shipped 2026-08-05/06 with
> `../adr/2026-08-05-browser-sample-readback-is-async.md` (PR #367),
> which also retired this condition's earlier file,
> `gpu-tier-cannot-sample-led-output.md` — two sessions crossed and
> nobody circled back to this one. Verified live 2026-08-07 (control-
> first product display work, PR #387): GPU-tier preview slots publish
> real control output frames — the Explore/Projects lamp cards render
> from them — with a clean console and no per-tick retry storm (the
> restate-every-frame half was separately fixed by
> `../defects/2026-07-28-tick-error-restated-every-frame.md`'s
> rate-limiting in `lpa-server`).
# The browser GPU preview tier cannot render control products

**Shape** — rendering a CONTROL product (fixture sampling its visual into
LED samples) needs `sample_rgba16`, a blocking GPU→CPU readback that the
browser GPU tier does not implement (`blocking readback is native-only;
LED-output sampling runs on native servers or the CPU tier`). Every
control render on the GPU-tier preview host fails, so any surface asking
it for lamps waits forever:

- the module face's control hero / Control Output row sits "not
  tracked" / pending (the G1-walk sighting, 2026-08-05 — module control
  products never load in the live editor);
- the preview host retries EVERY tick and logs the same error —
  observed at "512 consecutive frames", which is also a per-tick log
  and work leak.

The **sim worker path is unaffected**: the fw-browser session samples on
the CPU tier, so the sim engine publishes real output frames — the
honest-preview card feed (P3b, sim ▶) rides that path and works.

**Fix directions** (engine-scale, split out of the honest-device-preview
plan at G1 per its p3b scope rule):

1. Async readback on the GPU tier (`map_async` + a frame of latency) —
   the honest fix; control previews tolerate a frame's lag trivially.
2. Route control-product sampling to the CPU tier while the visual stays
   GPU — hybrid, no readback, costs a CPU render of the sampled region.
3. At minimum: stop the per-tick retry storm — a product whose render
   capability is absent should fail ONCE into a classified state the UI
   can show honestly ("preview unavailable on this GPU tier"), not
   pending forever.

**Pointers** — error text from `lp-gfx` browser tier sampling
(`sample_rgba16`); the per-tick retry lives in `lpa-server`'s preview
tick (`LpServer::tick: Project preview tick error persists`). The
consuming UI states are `UiProductTrackingState` +
`ControlProductPreview` (`produced_product_view.rs`).
