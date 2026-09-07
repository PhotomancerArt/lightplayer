---
status: carried
since: 2026-07-10      # best-effort: lp-gfx-wgpu's first parity suite was adapter-gated from day one
logged: 2026-09-07
area: lp-gfx-wgpu tests + CI (Validate GFX runs on ubuntu-24.04-arm, no GPU adapter)
related:
  [
    "../defects/2026-09-07-metal-drops-atomic-guarded-by-loop-exit-sum.md",
    "../adr/2026-09-07-gpu-tier-spent-budget-faults.md",
    "../adr/2026-09-06-gpu-tier-loop-bounds.md",
    "local-gate-misses-what-ci-checks.md",
  ]
---
# GPU-tier tests run only on a desk with an adapter

**Shape** — every `lp-gfx-wgpu` test that touches a device is
adapter-gated: `util::test_graphics()` returns `None` without a wgpu
adapter and the test prints `SKIP` and passes. CI's Validate GFX job
runs `just test-gfx` on `ubuntu-24.04-arm`, which has no adapter, so
the render/sample parity envelopes, the NaN regression, the texture
corpus and the loop-fault guard (`tests/loop_fault.rs`) all *pass* in
CI without running. The only machine that runs them is a developer's
desk (the M2 Max, Metal). This is structural: the IR/WGSL layer can be
pinned host-side, but what the driver's compiler does with a legal
module is visible only on a device, and there is exactly one device in
the loop.

**Carrying cost** — a GPU-tier regression is invisible to CI. The
2026-09-07 Metal miscompile (a guarded `atomicAdd` that validation,
wgsl-out and the IR tests all accepted and the driver dropped) was
caught only because the implementer ran `just test-gfx` locally; the
same PR's Validate GFX job was green before and after the fix. Every
GPU-tier PR must be run on the desk before merge, and a sibling PR
merged in between (as #561 was) re-opens the question. Metal is also
the only backend ever exercised: Vulkan (Linux), DX12 and the browsers'
WGSL compilers are covered by nothing.

**Workarounds** —
- Run `just test-gfx` on the desk before every push of a GPU-tier
  change, and again after merging `origin/main`; CI green is not
  evidence for `lp-gfx-wgpu`.
- Keep any GPU-tier mechanism that must *fire* (not merely translate)
  under a device test, and say so in the PR: the loop-fault guard is
  the model (`tests/loop_fault.rs`, 4×4 frames, milliseconds).
- For a suspected driver difference, the WGSL-variant harness shape in
  the 2026-09-07 defect (hand-written fragment + storage flag through
  raw wgpu) isolates the compiler from the pass in one test file.

**Incident log** —
- 2026-09-07 — PR #560: the fault flag's first shape bounded loops but
  never counted on a single top-level loop on Metal; CI green
  throughout; found by `just test-gfx` on the desk; fixed by a
  store-then-read-back order. Defect filed.

**Exit criteria** — a CI job with a real adapter (a self-hosted macOS
runner, or a Linux runner with lavapipe/SwiftShader accepted by
`wgpu::Instance` as a fallback adapter) runs `just test-gfx` with the
adapter-gated suites *executing*, and the job fails when they skip
(e.g. an env var that turns `SKIP` into a failure). Two backends
(Metal + Vulkan or the browser) would retire the "one device" half.
