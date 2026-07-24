# ADR: PreviewHost — leased, pooled, budgeted project previews

- **Status:** Accepted
- **Date:** 2026-07-16
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

lp-gfx M3 (browser integration) landed the full GPU preview machine:
each fw-browser worker requests one WebGPU device at boot; runtimes are
created with an explicit tier (`CreateRuntime { label, tier }` →
`RuntimeCreated { tier, tier_reason }`); a card's `OffscreenCanvas` is
transferred into the worker and the runtime's visual product renders
straight to it (`attach_preview_surface` / `present_bus_texture`, zero
readback); selection and failure states are surfaced, never silent
(`docs/adr/2026-07-09-preview-fidelity-tiers.md`).

Its only consumer was the dev-only preview-lab page, which also proved
the economics: 88–91% page CPU savings GPU vs CPU at gallery-like card
counts, GPU-bound with essentially no per-pixel host scaling (earlier
POC-A measurement). Meanwhile every product surface that wants "a
project, rendered live, in a box" — home-gallery thumbnails today;
visual-module previews and preview-mode authoring next — would each
need the same worker boot, runtime lifecycle, per-runtime deploy,
present scheduling, visibility handling, and failure containment.

Two hard facts shape the design. First, runtimes could be created but
never destroyed — fine for a lab page torn down wholesale, wrong for a
long-lived gallery where cards scroll in and out. Second, a hostile
shader can hang a GPU device unrecoverably (established during the M6
wgpu filetest work: a hung submission poisons the device and wgpu-core
panics in cleanup) — so "how many previews share one device" is a
failure-containment question, not a throughput question.

## Decision

One service, `PreviewHost` (module
`lp-app/lpa-studio-core/src/app/preview_host/`), owns live project
previews end to end. Consumers ask it to load content and render to a
canvas; they receive a **slot lease** and never touch workers,
runtimes, or envelopes.

### Leases over a worker pool

`PreviewHost` boots a small pool of preview workers (default **2**;
config) separate from the Studio session worker. A
`lease(PreviewSlotRequest)` picks the least-loaded worker, creates a
tiered runtime, deploys the requested content (an example's deploy
files, or a library project materialized by uid), attaches the
consumer's transferred canvas on the GPU tier, and returns a
`PreviewSlotHandle` exposing observable status — `Deploying`,
`Live { tier, tier_reason }`, `Suspended`, `Error { reason }` — plus
`set_visible(bool)` and release. Releasing (or LRU-evicting past the
live-slot cap) destroys the runtime via the new same-bundle
`DestroyRuntime` envelope; no wire version bump because the worker JS
and wasm ship as one bundle.

The pool size of two is an **isolation** choice: device loss is
per-worker, so one hostile project takes down at most half the
previews, and the host recycles that worker (respawn, re-lease the
still-visible slots) while the other keeps presenting. CPU parallelism
is not the motivation — the measured path is GPU-bound.

### One scheduler, explicit budgets

A single host-side deadline scheduler drives every slot across the
pool: per-slot fps (thumbnails default ~12), per-slot in-flight
backpressure, suspend/resume from consumer visibility signals, and a
global live-slot cap. Card runtimes are explicit-tick by construction
(worker self-tick drives only the boot runtime), so all pacing policy
lives host-side in one place. The scheduler paces off a worker-backed
sleeper, keeping previews honest under hidden-tab timer throttling —
the preview-lab lesson.

### Failure stays visible

Tier selection, CPU fallback reasons, present errors, and device loss
all surface on the slot's status and thus on the consuming UI, per the
fidelity-tiers ADR. Recycling is deliberate (respawn + re-lease), never
an automatic retry flap.

### The PreviewProfile seam

`PreviewSlotRequest` carries a `PreviewProfile` — today an empty,
default-only struct. It is the reserved seam for per-project preview
behavior: auto-playback of inputs (button presses), audio sources for
music-reactive programs, and eventually authoring a project "in preview
mode". Naming the seam now means those features extend the request
instead of reshaping the service contract.

## Alternatives considered

- **Per-consumer wiring (preview-lab pattern copied into the gallery).**
  Rejected: every future preview surface re-implements lifecycle,
  scheduling, and failure handling; budgets fragment per consumer.
- **Reuse the Studio session worker for card runtimes.** Rejected: its
  lifecycle is tied to an open project session (absent on the home
  screen), and gallery failures would share a device with the editing
  session.
- **One preview worker.** Rejected on blast radius: a single hostile
  project would darken every preview until recycle.
- **Worker-per-preview.** Rejected: one WebGPU device request per
  worker; dozens of devices for a gallery is waste with no isolation
  benefit at that granularity.

## Consequences

- Gallery cards (and future module previews) consume a handle, not a
  transport; the host is the one place budgets and containment are
  enforced.
- `DestroyRuntime` joins the worker envelope set; runtimes become
  fully lifecycle-managed.
- Stories must render deterministic non-live card states (byte-compared
  baselines); live canvases never appear in stories.
- A future GPU fluid solver (wanted for CPU-bound sims) will change
  per-slot cost assumptions; the scheduler's budgets are config for
  that reason.
