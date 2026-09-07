---
status: open
found: 2026-09-06      # how: G1 walk of the catalog branch (PR #536) — the landing hung in Brave after the posters loaded; Firefox took the machine down; a native wgpu harness run of the same entry glitched the whole macOS desktop
class: contract-gap
area: lp-gfx-wgpu (GPU tiers: browser WebGPU preview, native wgpu harness) + catalog content
related:
  - docs/adr/2026-09-02-fault-is-never-black.md
  - docs/adr/2026-09-06-catalog-content-tree.md
  - docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md
  - lp-gfx/lp-gfx-wgpu/src/read_back.rs (the `timeout` comment already states the gap)
---
# A GPU tier executes an unbounded shader with nothing to stop it

**Symptom** — on the catalog branch, the Studio landing page hung in
Brave ~30 s after the gallery posters came in, a neighbouring card
(Plasma Duo) showed a frame of striped garbage, and in Firefox the whole
machine locked up and had to be rebooted. Running the same entry through
`lp-gfx-harness --engine gpu-f32` natively on the M2 Max glitched every
window on the desktop and crashed the Claude harness UI.

**Cause** — `fault-demo`'s shader is `while (true) { acc += 0.001; }`
on purpose: the never-black ADR's deterministic runtime fault, which
relies on the LPVM's per-invocation fuel meter (`DEFAULT_INVOCATION_FUEL`
= 100,000 back-edges) to trap it. Every LPVM backend meters fuel; **no
GPU tier does**. `lp-gfx-wgpu` compiles the same GLSL through naga and
dispatches it; the only thing that can stop a kernel that never returns
is the driver's watchdog, which resets the device — corrupting every
other surface in flight (the striped frame), poisoning the preview
worker, and on some driver/OS combinations taking the compositor or the
kernel with it. `read_back.rs` already says so in a comment ("corpus
shaders may not terminate … the GPU has none — an unbounded wait hangs
the process") and bounds the *filetest* wait; the product paths pass
`None`.

The gallery never ran it before because the card was the fourteenth and
last on main's landing and a poster is captured only when its card
scrolls into view; the grouped landing put it second in the Patterns
section, so every visit captured it.

**What was done (PR #536)** — `fault-demo` moved from `catalog/patterns/`
to `projects/test/`: it is no longer embedded, no gallery card previews
it, and it stays the engine/server tests' subject and an `lp-cli dev`
rig. The catalog README says why it is absent.

**What is still open** — the contract gap. Content authored for the
fuel-metered tiers can reach a GPU tier unchanged; nothing refuses or
bounds it. Candidate fixes, cheapest first:

1. A static "loop with no exit" refusal at GPU compile time in
   `lp-gfx-wgpu`'s naga path: an LPIR/naga loop whose body has no `break`
   or `return` on any path is a shader-creation error on the GPU tier
   (it becomes a compile fault, and the never-black pattern paints —
   the ADR's promise holds, by a different route). Catches the demo and
   the honest mistake; misses data-dependent runaways.
2. An injected iteration cap on the GPU tier (a naga transform adding a
   counter and `break` to every loop, mirroring the fuel meter's
   back-edge unit) — the general fix; costs a transform and a policy
   for what the cap is.
3. A bounded device wait on the product read-back paths (the filetest
   `timeout` for everyone) — turns a system hang into a slot error, but
   only after the watchdog has already fired.

**Rule until fixed** — never run a shader you have not bounded through
`--engine gpu-f32`, and never author an unbounded loop into anything the
gallery embeds. The GPU is not a sandbox.
