# ADR: Two fidelity tiers — Q32 authoritative, f32 GPU for preview and non-embedded scale

- **Status:** Accepted (user decisions 2026-07-09, GPU-preview roadmap M4)
- **Date:** 2026-07-09
- **Deciders:** Photomancer
- **Related:** `2026-07-08-glsl-canonical-builtins.md`,
  `2026-07-09-gpu-path-forks-at-glsl.md`, `docs/design/q32.md`

## Context

LightPlayer's normative shader semantics are Q16.16 fixed point
(`docs/design/q32.md`), executed by the on-device JIT and the browser-sim
wasm backend. Two product needs want GPU execution: live project preview
cards in Studio (~20+ at once; battery matters) and future **non-embedded
lp-servers** on desktop/RPi driving installations beyond what an ESP32 can
(a stated strategic direction). Simulating Q32 on GPU would mean
reimplementing every builtin's normative semantics in integer WGSL and
would surrender most of the GPU's advantage.

Measured evidence (GPU-preview roadmap, PoCs M1–M3, reports in the
planning workspace): f32 GPU rendering of real authored shaders through
naga is visually indistinguishable from the Q32 pipeline on ordinary
content (mean divergence ≤3/255); divergence concentrates where Q32
approximation error is amplified (hue wheels) or where shaders rely on Q32
saturation by design.

## Decision

1. **Q32 remains the single authoritative semantics.** ESP32 devices, the
   browser-sim editor session, and conformance oracles keep it.
2. **f32-on-GPU is the preview and large-scale tier**: Studio gallery
   cards, and the *default* engine for non-embedded lp-servers.
3. **Non-embedded lp-servers offer a Q32 CPU parity mode**, selectable per
   deployment, for bit-parity with embedded devices (debugging, mixed
   installs). Q32-on-GPU is explicitly not built now.
4. **Tier selection is always explicit and visible — never a silent
   fallback.** A runtime that cannot use the GPU tier (no WebGPU, adapter
   failure, GPU compile failure, device lost) surfaces the CPU selection
   as user-visible state (badge/log/wire-queryable). Rationale: a silent
   downgrade can mask a regression that looks correct while consuming an
   order of magnitude more power.

## Consequences

- GPU shader assembly must close known f32/GPU-specific gaps (bounded-tanh
  rewrite for Metal fast-math NaN) rather than chase bit parity.
- Frontend semantic bugs become tier-divergence bugs (e.g. the eager
  `&&`/`||` lowering found 2026-07-09) and must be fixed at the frontend —
  the GPU tier must not replicate CPU-frontend bugs for parity.
- Conformance: the GPU backend joins the filetest harness as a target;
  GPU-f32 vs interpreter-f32 isolates GPU defects from Q32 approximation.
- Saturation-reliant art previews differently by design; authoring docs
  may eventually note this.

## Alternatives considered

- **Q32-on-GPU (integer WGSL transforms):** bit-faithful but reimplements
  normative semantics a fourth time and loses GPU throughput; deferred
  until real evidence that the parity mode is insufficient.
- **f32-only for everything non-embedded:** simpler but gives up
  bit-parity debugging against fielded devices.
- **Silent CPU fallback:** rejected — regression-masking (power, perf).
