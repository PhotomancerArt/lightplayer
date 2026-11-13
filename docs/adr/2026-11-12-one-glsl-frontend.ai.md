# ADR: One GLSL frontend — lps-glsl everywhere, lps-frontend deleted

- **Status:** Accepted (Yona: "fewer concepts to own"; plan
  `lp2025/2026-11-12-0917-one-glsl-frontend`)
- **Date:** 2026-11-12
- **Deciders:** Photomancer
- **Supersedes:** the "Naga-backed path can remain a reference target"
  paragraph in [`docs/design/lps-glsl-native-frontend.ai.md`](../design/lps-glsl-native-frontend.ai.md)
  ("Keep Naga Out Of The Default Firmware Path"), and the parity-work
  arrangement it describes (two filetest targets per backend, `rv32n` on
  naga beside `rv32lpn` on lps-glsl).
- **Superseded by:** None
- **Related:** [2026-08-01-float-mode-as-a-compiler-parameter](2026-08-01-float-mode-as-a-compiler-parameter.ai.md),
  [2026-08-05-sampler2d-authored-surface](2026-08-05-sampler2d-authored-surface.ai.md),
  [2026-09-06-gpu-tier-loop-bounds](2026-09-06-gpu-tier-loop-bounds.ai.md)
  (the GPU tier, which keeps naga). Planning: `lp2025/2026-11-12-0917-one-glsl-frontend`.

## Context

Two GLSL → LPIR frontends lived in the tree. `lps-glsl` is LightPlayer's own
(`no_std`, resumable, the one every board ships:
`DEVICE_SHADER_FRONTEND = LpsGlsl`). `lps-frontend` lowered naga's `glsl-in`
output to LPIR (18,744 lines) behind a `naga` Cargo feature that ran from
`lp-shader` through `lpc-engine` and `lpa-server` to `fw-emu`, `fw-esp32c6`
and `fw-browser`, and behind a `ShaderFrontend` choice threaded through
compile descriptors, graphics options, the server and the filetests' target
names. Every GLSL feature had to land twice, and the frontend boards run had
the thinnest default test coverage: five of the six default filetest targets
compiled through naga. Studio's in-browser CPU tier (`fw-browser`) also ran
naga, so the simulator did not compile what a board compiles.

The gap was measured before anything was deleted (planning dir
`~/.photomancer/planning/lp2025/2026-11-12-0917-one-glsl-frontend/measurements/`,
sweep method in its `notes.md`):

- On `rv32n.q32` naga passed 6321 of 6536 `// run:` directives and lps-glsl
  6370. Naga-only passes: **46** (45 are `global-future` in/out stage globals,
  a by-design omission, plus `intBitsToFloat(0)`); lps-glsl-only passes:
  **95**. `rv32c.q32` gave the same 46 / 95.
- `wasm.q32` had 53 naga-only passes (the 46, plus `radians`, `roundEven` and
  an overload case). `wasm.f32` failed 20 files on lps-glsl and `interp.f32`
  41, among them 18 that failed on a single missing lexer feature.
- Seven distinct gaps (G1–G7 in the plan) accounted for everything else, and
  **none was visible in the corpus**: no filetest and no catalog shader used a
  bitwise operator, so lps-glsl shipped without them.

Switching surfaced defects that boards already had, because the corpus had
only ever been green on the other frontend. Each was fixed in the switch
(PR #1340):

- no bitwise operators (`&`, `|`, `~`, `<<`, `>>` and their compound
  assignments) — [defect](../defects/2026-11-12-lps-glsl-has-no-bitwise-operators.ai.md);
- no hex or octal literals —
  [defect](../defects/2026-11-12-lps-glsl-has-no-hex-or-octal-literals.ai.md);
- a ternary's reject arm swallowed the arguments after it —
  [defect](../defects/2026-11-12-lps-glsl-ternary-swallows-following-arguments.ai.md);
- an empty global initializer panicked the compiler —
  [defect](../defects/2026-11-12-lps-glsl-panics-on-an-empty-global-initializer.ai.md);
- no function overloading at all (bodies were keyed by name), const writes and
  non-constant const initializers accepted, and an implicit return after an
  explicit one (a Cranelift panic);
- found in the backends, not lps-glsl: `lpvm-wasm` truncated Q32 constants and
  rounded `Fnearest` ties away from zero (now native's behavior), passed
  aggregate parameters by pointer against lps-glsl's by-value IR, and
  `texture` imports took the wrong coordinate type on `wasm.f32`; the
  interpreter harness sized `sret` from the GLSL type.

## Decision

1. **`lps-glsl` is the only GLSL → LPIR frontend**, on every target: devices
   (C6, S3, classic), Studio's in-browser CPU tier (`fw-browser`), the engine,
   the filetests (all of them, error tests in lps-glsl's wording), `lps-probe`
   (and so the shader agent's `iterate`), `lp-cli`'s shader tools, `web-demo`,
   and the CPU side of the GPU parity suites. `lps-frontend` is deleted.
2. **No frontend choice exists.** `ShaderFrontend`,
   `ShaderCompileOptions.frontend`, `LpGraphics::glsl_frontend`,
   `DEVICE_SHADER_FRONTEND`, the `naga` Cargo feature chain and the filetests'
   `Frontend` axis are gone. Filetest target names carry no frontend:
   `rv32n.*`, `xtn.*` (formerly `rv32lpn.*`, `xtlpn.*`); the default targets
   are `rv32n.q32`, `rv32c.q32`, `wasm.q32`, `interp.f32`, `wasm.f32`.
   `StdMathHandler`, which only needed `lpir` and `libm`, moved into `lpir`.
3. **The vendored naga fork (`third_party/naga`) stays only for the GPU tier**:
   `lp-gfx-wgpu` translates GLSL with `glsl-in` → `wgsl-out` and never went
   through `lps-frontend`. A naga mention outside the GPU tier is now a bug.
4. **A feature lands once.** A new GLSL feature is implemented in lps-glsl and
   given a filetest that exercises it; there is no second frontend to keep in
   step and no reference target to diff against. A shape the corpus does not
   cover is still invisible, as bitwise operators were, so a language feature
   gets its own filetests when it is added.

## Consequences

- About 19.8K lines gone net (`lps-frontend` 18,744; the deletion PR
  removes ~20.4K and adds ~0.6K over ~250 files). One frontend to learn,
  test and fix.
- **Studio's simulator compiles exactly what boards compile.** The CPU tier in
  `fw-browser` no longer links naga's lowering; the shader agent and
  `lps-probe` use the same frontend a board does.
- **GPU-vs-CPU parity is proven on Metal only.** CI has no GPU adapter
  (`lp-gfx-wgpu` adapter-gated tests skip), so the `lp-gfx-wgpu` suites with
  the CPU side on lps-glsl were run on the Mac, and every bound held (the
  largest move: the basic pattern-space mean at t=2.5, 0.479 → 0.721, inside
  its bound). Re-run `cargo test -p lp-gfx-wgpu` on a machine with an adapter
  when touching the CPU or GPU compile paths.
- **Sizes.** The C6 engine image grew about **6.5 KB** in the switch PR
  (+6,462 B CI to CI, of which lps-glsl +4,404 B; core +320 B): the cost of
  the language features the corpus and the interpreter demanded (bitwise
  operators, overloads, const checking, literals), not of the deletion.
  Headroom stayed above the gate. `fw_browser.wasm` shrank
  9,567,438 → 9,103,696 B (**−463,742 B**). The C6/S3 images never linked
  naga, so removal changed nothing there.
- Behavior gaps with naga that were decisions, not defects, stay as they are:
  `global-future` stage in/out globals, and `intBitsToFloat`.
- A tie in `round()` is target-defined; the corpus expects 3.0 for
  `round(2.5)` on lps-glsl.
- The naga reference for behavior is gone. Disagreements are now settled
  against the GLSL specification and the GPU tier (`wgpu.f32` filetests, the
  parity suites), which is the reference that matters for authored shaders.

## Alternatives Considered

- **Keep naga as an opt-in reference target.** The previous design note's
  position. Rejected: every feature still landed twice, five of six default
  targets kept testing the frontend nobody ships, and on devices the opt-in
  was reachable only through the `demo-esp32c6-*-naga` recipes.
- **Keep naga for Studio's simulator only.** Rejected: the simulator exists to
  show what the board will do; a different frontend there is the wrong
  answer by construction, and it cost the browser bundle ~460 KB.
- **Delete the naga fork too.** Not possible: the GPU tier needs `glsl-in` →
  `wgsl-out`.
- **Fix lps-glsl's gaps lazily and keep both until then.** Rejected: the
  measured gap was small (46 directives that were stage globals and one
  `intBitsToFloat`) and the bigger risk was the defects the second frontend
  was hiding, which only a full switch finds.

## Follow-ups

- `sweep -t interp.f32` in `lps-filetests-app` does not terminate on some
  directive (it has no fuel or timeout of its own); the test runner does. The
  sweep bit twice during this work and is the way to re-baseline stale
  annotations, so it wants a fuel limit.
