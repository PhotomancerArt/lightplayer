---
status: fixed
found: 2026-09-06      # how: G1 walk of the catalog branch (PR #536) — the landing hung in Brave after the posters loaded; Firefox took the machine down; a native wgpu harness run of the same entry glitched the whole macOS desktop
fixed: this change     # branch claude/gpu-unbounded-shader-safety-974c30; content eviction landed separately in PR #536
class: contract-gap
area: lp-gfx-wgpu (GPU tiers: browser WebGPU preview, native wgpu harness) + catalog content
related:
  - docs/adr/2026-09-06-gpu-tier-loop-bounds.md
  - docs/adr/2026-09-02-fault-is-never-black.md
  - docs/adr/2026-09-06-catalog-content-tree.md
  - docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md
  - lp-gfx/lp-gfx-wgpu/src/loop_bound_pass.rs
  - lp-gfx/lp-gfx-wgpu/src/read_back.rs
---
# A GPU tier executes an unbounded shader with nothing to stop it

**Symptom** — on the catalog branch, the Studio landing page hung in
Brave ~30 s after the gallery posters came in, a neighbouring card
(Plasma Duo) showed a frame of striped garbage, and in Firefox the whole
machine locked up and had to be rebooted. Running the same entry through
`lp-gfx-harness --engine gpu-f32` natively on the M2 Max glitched every
window on the desktop and crashed the Claude harness UI.

**Root cause** — `fault-demo`'s shader is `while (true) { acc += 0.001; }`
on purpose: the never-black ADR's deterministic runtime fault, which
relies on the LPVM's per-invocation fuel meter (`DEFAULT_INVOCATION_FUEL`
= 100,000 back-edges) to trap it. Every LPVM backend meters fuel; **no
GPU tier did**. `lp-gfx-wgpu` compiled the same GLSL through naga and
dispatched it; the only thing that could stop a kernel that never returns
was the driver's watchdog, which resets the device — corrupting every
other surface in flight (the striped frame), poisoning the preview
worker, and on some driver/OS combinations taking the compositor or the
kernel with it. `read_back.rs` already said so in a comment ("corpus
shaders may not terminate … the GPU has none — an unbounded wait hangs
the process") and bounded the *filetest* wait; the product paths passed
`None`.

The gallery never ran it before because the card was the fourteenth and
last on main's landing and a poster is captured only when its card
scrolls into view; the grouped landing put it second in the Patterns
section, so every visit captured it.

**Fix** — two layers in `lp-gfx-wgpu`, plus a bound on the waits
(`docs/adr/2026-09-06-gpu-tier-loop-bounds.md`):

1. **Static refusal at GPU compile time.** `loop_bound_pass::
   refuse_loops_without_exit` runs on the naga IR after `glsl-in`: a
   loop whose body has no `break`, `return` or `discard` on any path is
   a `GfxError::Compile` naming the function and the assembled-source
   line. Constant `if` conditions are folded first, because glsl-in
   lowers `while (true)` to a loop whose body opens with
   `if (!true) { break; }` — the exit is present in the IR and dead.
   `fault-demo` now fails to compile on the GPU tier; the shader node
   reports the diagnostic as an authoring `Error` (the tier refuses the
   program), and no device is ever asked to run it.
2. **An injected per-invocation back-edge budget.**
   `loop_bound_pass::bound_loop_iterations` adds one `var<private>`
   counter per module and, in every loop's `continuing` block, charges
   it one unit and `break if`s once it exceeds `DEFAULT_INVOCATION_FUEL`
   — the same unit (the loop back-edge) and the same tank as the LPVM's
   fuel meter, which is what content is authored against. This catches
   the data-dependent runaway the static check cannot. The GPU cannot
   trap, so an exhausted budget exits the loop and the invocation
   completes with whatever it has: bounded, not faulted (recorded as the
   accepted gap in the ADR).
3. **Bounded product read-back waits.** The three native product
   read-back paths (`read_back`, the raw-float probe, the sample pass)
   now wait `PRODUCT_READ_BACK_WAIT` (10 s) instead of forever; a device
   the driver has already reset surfaces as `GfxError::Backend` rather
   than a hung host.

The content side landed first in PR #536: `fault-demo` moved from the
catalog to `projects/test/`, so no gallery card previews it; it stays the
engine/server tests' subject and an `lp-cli dev` rig.

**Regression coverage** — host-side only; nothing dispatches an unbounded
kernel:

- `wgsl_compile::tests::fault_demo_is_refused_at_gpu_compile_time` reads
  the rig's shader from disk (`projects/test/fault-demo` or, until #536
  merges, `examples/fault-demo`) and asserts the `Compile` refusal names
  `render_2d` and the `while (true)` line.
- `loop_bound_pass::tests` pin the refusal shapes (`while (true)`,
  `for (;;)`, `do … while (true)`, `while (!false)`, a break that only
  reaches a nested switch or loop, an infinite inner loop in a bounded
  outer one, a loop in a helper) and the accepted ones (counted `for`,
  data-dependent `while`, break/return/discard on some path), and the
  budget's shape on the IR and the emitted WGSL (charge last in
  `continuing`, `break if` on the budget, an existing `break if` OR-ed,
  one shared global, loop-free modules untouched).
- `tests/wgsl_corpus.rs::corpus_loops_are_bounded_by_the_invocation_budget`
  holds every corpus loop to a `break if` and rocaille's two loops to
  exactly two.

**Lesson** — a guard that one tier enforces and content relies on is part
of the interface, whether or not the interface says so. The fuel meter was
documented as an LPVM feature; the shaders written to lean on it (and the
never-black ADR that made leaning on it a feature) treated it as a
property of *shaders*, and the GPU tier inherited the content without the
guarantee. When a second implementation of a tier arrives, walk the
guarantees the first one's content depends on — not just the outputs it
produces — and either enforce each on the new tier or refuse the input
that needs it. The comment in `read_back.rs` had named this gap for
months; a comment that names a gap is a defect entry that has not been
filed.
