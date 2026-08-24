# ADR: Two fidelity tiers — Q32 authoritative, f32 GPU for preview and non-embedded scale

- **Status:** Accepted, **partially superseded 2026-08-01** (user decisions
  2026-07-09, GPU-preview roadmap M4). Decision 1's "single authoritative
  semantics" clause and its scope clause about ESP32 devices no longer hold;
  everything else stands. See **Superseded by**.
- **Date:** 2026-07-09
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** `2026-08-01-float-mode-as-a-compiler-parameter.md`, in
  part only. That ADR retires (a) "Q32 remains the single authoritative
  semantics" — there are now two, and which applies is a property of the
  compiled module — and (b) the clause scoping ESP32 devices to Q32, because
  the ESP32-S3 has an FPU and now executes IEEE f32 natively. It explicitly
  does **not** retire the preview-fidelity framing, the GPU tier's documented
  latitude and divergence bounds, decision 4's no-silent-fallback rule (which
  it extends), decision 3's non-embedded Q32 parity mode, or Q32's status as
  the default everywhere and the only option on boards without an FPU.
- **Related:** `2026-07-08-glsl-canonical-builtins.md`,
  `2026-07-09-gpu-path-forks-at-glsl.md`, `docs/design/q32.md`,
  `docs/design/float.md`. **Amended 2026-08-05** by
  `2026-08-05-browser-sample-readback-is-async.md`: this ADR predates
  LED-output sampling, whose blocking readback was native-only and left
  browser GPU-tier runtimes unable to render fixture-bearing projects
  (carried as `docs/debt/gpu-tier-cannot-sample-led-output.md`, now
  retired); the browser GPU tier now samples via an async readback with
  one frame of latency. **Amended 2026-08-08** by
  `2026-08-08-float-semantics-per-target-representation.md`: it retires nothing
  further here, but it **generalizes decision 2's GPU-tier latitude** (an
  authored `Fixed` rendering IEEE f32 is a documented product decision, not a
  dropped request) from one tier into the general rule — float is the product's
  one authored semantics, and numeric representation is a per-target execution
  detail. Decision 4 is load-bearing in that ADR and is carried forward
  unweakened; see the note under it.

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

   > **Superseded 2026-08-01** by
   > `2026-08-01-float-mode-as-a-compiler-parameter.md`. There are now two
   > authoritative numeric semantics — Q32 (`docs/design/q32.md`) and IEEE f32
   > (`docs/design/float.md`) — and which one applies is a property of the
   > compiled module, disclosed by `FloatImpl`. The ESP32-S3 has an FPU and
   > executes native f32 on silicon as of M7 P5. Q32 stays the **default**
   > everywhere and the only option on boards without an FPU, which is still
   > most of them. Points 2–4 below are unaffected.
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

   > **Note, 2026-08-08 — scope clarified, not narrowed.**
   > `2026-08-08-float-semantics-per-target-representation.md` generalizes
   > decision 2's numeric latitude across targets, and the two rules are easy
   > to confuse from outside because they look alike. The distinction that
   > ADR fixes: **latitude** is a target executing a request in the
   > representation it carries (documented per target); **fallback** is a
   > target silently answering a request it cannot serve. This decision is
   > about the second, and it stands unchanged — a board whose image linked no
   > f32 backend still *errors* on a pinned Float rather than quietly
   > compiling Q32.

   > **Note, 2026-08-24 — visibility surface chosen: issue-only badges on
   > browsing surfaces.** This decision lists three visibility channels
   > (badge/log/wire-queryable); the first implementation badged the granted
   > tier on every preview card. Product decision (PR #444, chrome/UX polish
   > pass): **browsing surfaces — the landing hero, gallery cards, docs
   > heroes — badge failures only.** A visitor reading "GPU" on a thumbnail
   > learns nothing actionable; the *normal* state needs no announcement.
   > The rule itself is not weakened: tier selection stays explicit in logs
   > and wire-queryable status, diagnostic surfaces may still show it, and a
   > preview that *fails* keeps a visible badge everywhere with its reason.
   > Context that shrank the stakes: primary targets have shifted from
   > Q32-class boards to f32-class (ESP32/S3 with FPUs, RV32F direction), so
   > the tier gap a badge once hinted at — quantization and i16 integer-part
   > range — no longer separates preview from device on the boards most
   > users hold. The power-masking rationale is unchanged and is served by
   > the log/wire channels.

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
