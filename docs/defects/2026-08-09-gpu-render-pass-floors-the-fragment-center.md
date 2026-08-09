---
status: open
found: 2026-08-09      # how: report (code reading during the
                       # visual-parameterization vision session; verified
                       # against post-#403 main)
area: lp-gfx-wgpu/src/assembly.rs (generated fragment main) vs
  lp-shader/synth/render_texture.rs (pixel walk)
class: backend-contract-divergence
related:
  - 2026-07-30-q32-native-vs-wasmtime-last-bit.md
  - 2026-07-27-story-check-tolerance-ignores-amplitude.md
  - ../adr/2026-07-09-gpu-forks-at-glsl.md
---
# The GPU render pass floors the fragment center, so the two tiers evaluate every pixel half a pixel apart

**Symptom** — the CPU and GPU tiers hand the same authored `render_2d` /
`render_1d` entry *different coordinates for the same pixel*. The CPU
synth's render-texture loop walks pixels in Q16.16 seeded at `Q_HALF` and
stepped by `Q_ONE` (`lp-shader/lp-shader/src/synth/render_texture.rs:150`–
`217`), so the entry receives pixel **centers**: `(x + 0.5, y + 0.5)`. The
GPU tier's generated fragment `main` splices
`render_2d(floor(gl_FragCoord.xy))`
(`lp-gfx/lp-gfx-wgpu/src/assembly.rs:72`–`75`), so the entry receives the
integer **corner** `(x, y)`. Every GPU evaluation therefore samples the
pattern half a pixel away from where the CPU tier samples it — a
systematic, whole-frame offset, not noise. No error is raised anywhere;
the divergence surfaces only as part of the cross-tier diff that the
render-parity suite measures and tolerates.

Compounding it, the module doc at `assembly.rs` (item 5, formerly lines
29–32) asserted the **opposite of reality**: that the CPU loop "passes
integer pixel coordinates without a half-pixel offset", presenting the
`floor` as convention-matching. The comment is corrected in the same
change that files this entry; the code is deliberately left as-is (see
Fix).

The sample-point pass is **not** affected: both tiers pass caller-provided
coordinates through raw (`assemble_sample_fragment_glsl` applies no
`floor`, matching the CPU `__render_samples_rgba16` loop), so only the
full-frame render pass carries the offset.

**Root cause** — the coordinate convention at the tier seam was asserted
in a comment instead of read from the synth or pinned by a test, and the
assertion was wrong. `gl_FragCoord` already carries exactly the CPU
convention — fragment centers at `x + 0.5` (default sample position,
non-multisampled) — so the `floor` discards the correct value to
construct the wrong one in the name of matching it.

Two guards existed and neither could falsify it:

1. `lp-gfx-wgpu/tests/render_parity.rs` is a **hold-or-beat bound on mean
   divergence** (per-shader means of 2.0–21.0 in 8-bit units) derived
   from the m3 spike report — measurements taken *with this offset
   already present*. A tolerance calibrated on a defective baseline
   blesses the defect by construction: the half-pixel shift is inside
   the definition of "parity" the test enforces.
2. CI's gated Validate GFX job runs `cargo test -p lp-gfx-wgpu`, but the
   adapter-gated tests (all the parity suites) **skip cleanly on runners
   without a GPU** — so parity is only ever measured on a developer
   machine via `just test-gfx`, and nothing in any gate would have
   flagged a convention change in either direction.

**Fix** — none yet; the entry is filed open, and the false comment is
corrected to describe reality. The likely fix is one line: drop the
`floor`, i.e. `render_2d(gl_FragCoord.xy)` / `render_1d(gl_FragCoord.x)`,
which reproduces the CPU convention exactly (pixel centers). It is parked
rather than done because it **moves every GPU-rendered frame** by half a
pixel: the render-parity bounds must be re-derived (expect improvement —
part of the current measured divergence *is* this offset), and any
GPU-derived image baselines churn. CPU `+0.5` is the defensible
convention to converge on — pixel centers are what `gl_FragCoord` itself
means — so the CPU tier, the device tier, and the wasm preview (all fed
by the same synth) stay untouched and the GPU tier moves to them.

**Regression coverage** — none, and the gap has a specific shape: a
statistical cross-tier diff cannot pin a coordinate convention, because a
systematic sub-pixel shift under smooth shaders yields small means that
fit inside any tolerance with driver headroom. The convention pin the fix
should add is an **exact-value identity test**: a corpus shader that
returns its own `pos` (e.g. `pos / vec2(width, height)` into channels),
rendered on both tiers, asserting the known expected values — `x + 0.5`
at every pixel — rather than a bound on their difference.

**Lesson** — when a stand-in tier's equivalence is guarded by a
tolerance, the tolerance must be derived from a baseline known to be
convention-clean, or every systematic error present at calibration time
becomes part of the contract. A parity suite can hold-or-beat forever
while the two tiers disagree about where a pixel *is*. Separately: a
splice-site comment that asserts what *another tier's* code does is a
claim falsifiable by reading twenty lines of that code — and here it was
not only unverified but load-bearing, since the `floor` exists *because
of* the claim. Cross-tier convention claims belong next to an exact test,
not in prose.
