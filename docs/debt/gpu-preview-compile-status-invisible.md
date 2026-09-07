---
status: carried
since: 2026-08-24     # fidelity-tiers badge policy: badges only for lease errors
logged: 2026-09-07
area: Studio preview host + gallery cards (GPU tier)
related:
  - docs/adr/2026-09-06-gpu-tier-loop-bounds.md
  - https://github.com/PhotomancerArt/lightplayer/pull/561
  - docs/adr/2026-09-02-fault-is-never-black.md
---
# A GPU preview slot's compile status is invisible

**Shape** — the GPU tier runs only inside preview slots (gallery cards, the
docs hero, the preview lab); the editor's own session runtime is always the
CPU tier (`BrowserInputEnvelope::CreateRuntime`: "the boot runtime … is
always CPU-tier"). A preview slot deploys the project and presents frames,
but its runtime's *node* status never crosses to the page: the preview host
handles lease/present errors (`PreviewError`, badge) and relays worker
`warn` logs at `log::debug!` (`preview_worker.rs`), below the Studio's fixed
`Info` capture floor (`web_app.rs` `install_log_sink`). So a shader the GPU
tier refuses or fails to compile — PR #556's unbounded-loop refusal, a naga
validation error such as `wgpu-refuses-scalar-uniform-arrays` — produces a
card that simply never gets a poster (placeholder) and, on hover, a black
canvas: no badge, no message, and the authored-line diagnostic PR #561
made the producer emit is shown nowhere. This is structural: the CPU-tier
editor cannot show a GPU-only failure, and the GPU-only surface has no
status channel.

**Carrying cost** — a shader that previews fine in the editor (CPU tier)
and goes dark on its gallery card has no explanation in the product; the
distinction from a slow poster capture is invisible. Diagnosing needs the
worker console in DevTools. Visual gates on GPU-tier diagnostics cannot be
run from the Studio (PR #561's gate fell back to a story + unit tests).

**Workarounds** — open DevTools and read the *worker* console
(`[shader-node] compilation failed … naga …` / `unbounded loop …`); or
compile the source through `lp-gfx-wgpu` natively
(`cargo test -p lp-gfx-wgpu` has `compile_error_message` helpers); the
hero/docs previews behave the same way.

**Incident log**
- 2026-09-07 — PR #561 (GPU diagnostics at authored lines): live gate could
  not show the refusal in the Studio; fault-demo's imported card sat on its
  placeholder, hover = black canvas, no message.

**Exit criteria** — a GPU preview slot whose project has a node in
`Error`/`Fault` surfaces that state on the card (badge + the parsed
diagnostic headline, reusing `UiShaderError`), or the Studio offers a way
to run the editor session on the GPU tier so the existing error strip shows
it. Either makes PR #561's authored-line diagnostics reachable without
DevTools.
