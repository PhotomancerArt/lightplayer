# ADR: The GPU execution path forks at GLSL source via naga, not at LPIR

- **Status:** Accepted (user decisions 2026-07-09, GPU-preview roadmap M4)
- **Date:** 2026-07-09
- **Deciders:** Photomancer
- **Related:** `2026-07-09-preview-fidelity-tiers.md`,
  `2026-07-08-glsl-canonical-builtins.md`, `docs/lpir/`

## Context

LightPlayer's compiler pipeline is GLSL → LPIR → backends (RV32 JIT,
Cranelift, WASM, interpreter). LPIR is deliberately scalarized, decomposed,
and builtin-import-based — the right shape for CPU codegen on embedded
targets, and the wrong starting point for GPU code: re-vectorizing scalar
LPIR into efficient WGSL would fight the compiler's own lowering.

The GPU-preview spike (`spikes/wgpu-preview-poc`, M3 report 2026-07-09)
validated the alternative on real authored shaders: assemble GLSL source
(authored shader + canonical lpfn builtin prelude from
`lps-builtins/glsl/` + generated prototypes + generated fragment `main()`)
and hand it to naga (`glsl-in` → validate → `wgsl-out`) for wgpu. No naga
patches were needed; pipeline builds cost ≤26 ms per shader.

## Decision

GPU execution compiles **from assembled GLSL source through naga to WGSL**,
sharing the frontend parser family (naga `glsl-in` is already the primary
CPU frontend) but bypassing LPIR entirely. Builtins reach the GPU as GLSL
source (the canonical library — see the canonical-builtins ADR), not as
imports. Backend-agnostic semantic fixes (e.g. bounded-tanh) are applied
as naga IR transform passes between parse and WGSL emission.

Compute shaders do not fork: they remain on the CPU/wasm LPIR path
permanently (serial, stateful, tiny outputs; GPU offers nothing). A
GPU-backed `LpGraphics` is therefore a mixed backend.

wgpu is the portability layer (D3, 2026-07-09): one platform-neutral
backend crate serving browser WebGPU (first target), desktop Metal/Vulkan,
and RPi 4/5 Vulkan — the same code intended to power non-embedded
lp-servers.

## Consequences

- Two compilation pipelines must agree semantically; conformance runs
  compare GPU output against the f32 LPIR interpreter (isolates GPU
  defects) and Q32 references (measures tier divergence).
- Frontend features must be exercised against naga `glsl-in` acceptance
  (declaration-before-use requires generated prototypes; `lps-glsl`-only
  leniencies won't reach the GPU tier).
- LPIR stays CPU-shaped; no pressure to make it GPU-representable.
- naga version tracking matters (workspace pins naga 29; `wgsl-out` is a
  feature flag).

## Alternatives considered

- **LPIR → WGSL backend:** single IR, but requires re-vectorization and a
  builtin story LPIR was designed to avoid; rejected.
- **WGSL as authored source:** rejected for now — GLSL is the product
  authoring language on-device; WGSL input remains future work.
- **GLSL passthrough to WebGL2:** legacy API, no native/RPi story;
  naga/wgpu covers a GLES fallback later if ever needed.
