---
status: retired
since: 2026-07-09      # best-effort: the tiers ADR; the gap opened as LED sampling landed beside it
logged: 2026-07-28
area: fw-browser/tier + lp-gfx-wgpu + lpc-engine/fixture
related:
  [
    "../defects/2026-07-28-tick-error-restated-every-frame.md",
    "../adr/2026-07-09-preview-fidelity-tiers.md",
    "../adr/2026-07-09-gpu-path-forks-at-glsl.md",
    "../adr/2026-08-05-browser-sample-readback-is-async.md",
  ]
---
# A GPU-tier browser runtime cannot render a fixture control

**Shape** — Two designs meet and do not fit.

The **fidelity-tiers ADR** picks a runtime's tier once, at creation, on
a single question: *is WebGPU available in this worker?* A `gpu` request
is granted, or falls back to CPU with a recorded reason. Gallery
previews request `gpu`.

**Fixture control rendering** samples the visual at each LED's point
(`render_direct_fixture_control` → `ctx.sample_visual_into` →
`sample_rgba16`), which on wgpu needs a **blocking readback**. That is
native-only: `lp-gfx-wgpu` compiles the sample pass under
`#[cfg(not(target_arch = "wasm32"))]`, and the browser GPU tier answers
every such call with an error.

So a project containing a fixture node — most real LightPlayer projects
— cannot render on the browser GPU tier at all, and because tier is
chosen once and never revisited, it fails on every frame forever. Tier
selection asks whether the *device* can do GPU work; it never asks
whether the *project* needs a capability that tier lacks. That missing
question is the structural part: any future tier-only capability will
land in the same hole.

The ADR does not mention sampling, readback, or fixtures — it predates
the LED-output path. This is an unanticipated interaction between two
sound decisions, not a violated one.

**Carrying cost** — Live gallery previews are dead for fixture-bearing
projects; the card shows no frames. The tier badge reports `gpu` as
*granted*, which is true and useless. Until 2026-07-28 the only
evidence was ~60 console errors per second (now rate-limited), so the
failure is currently **quiet** as well as broken — cheaper to live
beside, harder to discover. Anyone debugging "preview shows nothing"
re-derives this chain from scratch.

**Workarounds** — None for the user. For diagnosis: the failure prints
once per project at `warn` from `LpServer::advance_frame`, naming the
node and `sample_rgba16`; a preview that renders nothing with that line
in the console is this. Forcing the CPU tier (the boot runtime is always
CPU) renders correctly.

**Incident log**
- **2026-07-28** — Found in prod: a fixture project's gallery preview
  flooded the console at frame rate, drowning an unrelated firmware
  flash the user was running. Spam fixed
  (`2026-07-28-tick-error-restated-every-frame`); the underlying
  capability gap filed here.
- **2026-08-05** — Resurfaced at the honest-device-preview G1 walk:
  the module face control hero, Control Output produced-product rows,
  and `ControlProductPreview` all sat "not tracked" forever against a
  GPU-tier preview host, with the tick error repeating 512+
  consecutive frames (rate-limited). Split out of that plan (ruling 4,
  `2026-08-05-1534-honest-device-preview/p3b-g1-outcomes.md`).
- **2026-08-05** — **Retired via exit 3** (async readback):
  `sample_rgba16` now works on the browser GPU tier as a
  one-frame-latency `map_async` pipeline in `lp-gfx-wgpu`'s sample
  pass (black on the very first frame, then trailing the visual by one
  frame). Decision recorded in
  `../adr/2026-08-05-browser-sample-readback-is-async.md`.

**Exit criteria** — A GPU-tier preview of a fixture-bearing project
either renders, or is never created in the first place with a reason the
UI states. Concretely, one of:

1. **Capability-aware tier selection** — inspect the project at creation;
   anything sampling LED output gets the CPU tier with a recorded
   reason. Honest and cheap, but hands most real projects back to the
   CPU tier — close to deleting the GPU preview.
2. **Don't render fixture controls in preview** — a card thumbnail wants
   the *visual*, not each fixture's LED swatch; skip control rendering
   on preview-tier runtimes and the sample path is never reached. Needs
   a real answer to what a preview is *for*, and the engine currently
   ticks every node.
3. **Async readback on the browser GPU tier** — make `sample_rgba16`
   work by not blocking. Most faithful, most work, and it fights the
   engine's synchronous sample API.

Option 2 looks cheapest and best-aligned with the preview's purpose, but
it is a product decision about preview fidelity, not a mechanical fix —
whichever is chosen amends the tiers ADR.

**Whoever picks this up**: the tier request originates in the gallery
preview host (`gallery_preview.rs`, `tier: "gpu"`), is granted in
`fw-browser/src/tier.rs`, and the failure surfaces from the wasm-side
`sample_rgba16` in `lp-gfx/lp-gfx-wgpu/src/render.rs`.
