# ADR: Shader probe/experiment API — numeric probes on the LPIR f32 oracle

- **Status:** Accepted
- **Date:** 2026-07-25
- **Deciders:** Photomancer
- **Supersedes:** None (builds on the conformance-oracle stance in
  `lp-shader/lps-filetests` and pairs with
  `2026-07-25-studio-shader-agent-architecture.md`)
- **Superseded by:** None

## Context

The shader agent (see the companion architecture ADR) needs a feedback
channel: after editing GLSL it must be able to ask "what does this shader
actually compute?" without a GPU, without the not-yet-built M6 capture
seam, deterministically, and from inside the Studio wasm app. The repo
already contained the pattern in test clothing: the `lps-filetests`
conformance oracle appends probe functions to a GLSL translation unit,
compiles the whole unit via `lps_frontend` (naga), and interprets
functions by name on the LPIR interpreter — the project's defined f32
oracle. What did not exist was a product-grade, `no_std`, sans-IO crate
exposing that pattern as an API with caps, structured results, and
serde-stable types.

## Decision

### The probe model

`lp-shader/lps-probe` (`no_std` + `alloc`, sans-IO, wasm-capable)
exposes `run_experiment(source, &ExperimentSpec) -> ExperimentResult`.

- A **probe** is a GLSL expression + a **domain** + an optional
  **reduce**. The expression is compiled as an appended wrapper function
  (`<ty> __probe_<id>(vec2 pos) { return (<expr>); }`) in the *same
  translation unit* as the user shader, so it can call `render()`, user
  helpers, and canonical `lpfn_*` builtins.
- Domains: `point`/`points`, `grid` (cell centers of a rect), `line`
  (endpoint-inclusive), `leds` (caller-supplied fixture sample points),
  `sweep` (scalar parameter instead of a position — "plot my helper").
  Coordinates are normalized [0,1]²; the harness maps to the shader's
  pixel-space `pos` contract via the experiment `size`.
- Reductions: `none` (raw rows, tightly capped), `stats` (per-component
  min/max/mean + NaN/Inf counts), `histogram { bins }`.
- **Evaluation = the LPIR f32 interpreter** via the naga frontend
  (`lps-frontend`) with host-side libm imports — the same oracle
  semantics as the conformance harness, and the same frontend as the
  browser sim, so probe diagnostics match what the sim reports.

### Per-probe compile isolation

The bare shader compiles first (its diagnostics are the user-meaningful
ones, in user-source coordinates). Each probe then compiles as shader +
that one wrapper: a broken probe expression fails only itself, remapped
to `probe '<id>' expr:<line>:<col>`. Unknown bindings are warnings,
never errors.

### Always-on health; diff via cached modules

Whenever the shader compiles, a **health report** evaluates `render`
over a coarse 16×16 grid plus all LED points: NaN/Inf counts,
near-black fraction, mean luminance, clipping — the renders-black
class of silent failure is caught with zero probes specified.
`ExperimentResult` keeps the compiled module (`#[serde(skip)]`), so a
session can cache it and call `diff_experiments(&prev, &cur, &spec)`
to evaluate the *previous* module at the same sites: max/mean delta +
changed region, answering "did my edit change anything, and where".

### Caps and determinism

Caps live in one place (`experiment.rs`): `MAX_PROBES = 8`,
`MAX_EVALS_PER_PROBE = 4096` (|domain| × |vary|), `MAX_RAW_VALUES = 64`
(reduce `none` only), `MAX_TOTAL_EVALS = 16 384`. Every `Skipped`
reason states what to change (add a reduce, shrink the domain).

Evaluation order is fully deterministic and documented: one interpreter
instance per probe compilation; `__shader_init` once; bindings in map
order; **`vary` values in given order as the outer loop**, domain sites
in domain order inner. Module-scope variable writes persist across
calls within the instance, so ordered `vary` steps approximate
frame-sequential behavior for stateful shaders.

### Duplicated from filetests, deliberately not shared

The interpreter harness and canonical-unit assembly were **copied and
adapted** from `lps-filetests` (`test_run/interp.rs`,
`conformance/oracle.rs`), with breadcrumb comments on both sides — not
extracted into a shared crate. Three reasons: `lps-filetests` is a test
monster (wgpu, wasmtime, cranelift, file IO, git deps) that must never
enter a product dependency tree; the filetests copies keep `anyhow`
signatures and lpfn-rename lookup semantics the product crate dropped;
and lps-probe's `call` deliberately persists VMContext state across
calls for the `vary` contract — a semantic change the filetests must
not inherit.

### Implementation deviations from the original spec (recorded)

- Specs carry a serializable `BindingValue` enum (not `LpsValueF32`,
  which has no serde); values coerce via the declared uniform leaf type.
- `diff_experiments` returns `Result<DiffReport, String>`.
- Results gained `warnings: Vec<String>` (home for unknown-binding
  warnings); runtime probe failures reuse `Skipped { reason }`.
- Result floats round to 4 significant digits via explicit `rounded()`
  (specs are never rounded); `int` probes report as f32.

## Consequences

- **Probe values are oracle semantics, not GPU pixels.** This is the
  project's existing conformance stance (GLSL = canonical semantics,
  LPIR interp = f32 oracle) extended to a product surface; the agent
  system prompt discloses it. Device Q32 divergence is out of scope.
- **The probe vocabulary doubles as the eval assertion language** — and
  this worked as designed: the P6 eval corpus (5 tasks, 23 probe-based
  assertions) passed **5/5 on the first live run with zero prompt or
  contract tuning**, including a deterministic time-`vary` sine sample
  and a monotonic line probe. The probes are expressive enough to grade
  the agent that uses them.
- Perf (measured): ~0.6 ms/eval in debug wasm-class code; a max-size
  experiment (4096 render evals + health + two compiles) ≈ 2.6 s. Fine
  on the main thread for modest experiments; worker offload is
  advisable-not-existential (follow-up).
- **The interpreter has no fuel/loop cap** (the 2026-07-20 fuel ADR
  covered rv32/wasmtime/browser-wasm, and its follow-ups flagged the
  interp gap "if interp leaves opt-in oracle duty"). lps-probe is
  exactly that departure: an infinite-loop shader evaluated by a probe
  hangs the Studio main thread until the tab is killed. Known, open,
  and now attached to a product surface (follow-up below).
- Dialect watch-item: the oracle's naga frontend requires
  `layout(binding=N)` on uniforms; the engine's device frontend accepts
  bare uniforms. A shader that renders on-engine can fail the oracle's
  first health call. In practice the model fixes declarations when
  editing; watch at live gates.

## Alternatives Considered

- **GPU readback probes**: rejected — nondeterministic across preview
  fidelity tiers, requires the not-yet-built M6 capture seam, and ties
  probe availability to a GPU context. Capture stays a *reserved* tool
  field until M6.
- **lps-glsl frontend for probes**: rejected for v1 — the interp
  harness and the browser sim are both naga-built, so naga keeps probe
  diagnostics/semantics consistent with what the sim renders. lps-glsl
  has nicer spans; revisit as a pure *linter* pass layered on top.
- **Separate pixel/stats/diff tool calls**: rejected — each hypothesis
  should cost one round-trip; a composite experiment spec batches
  everything and lets caps be reasoned about per call.
- **Sharing harness code with lps-filetests**: rejected — see the
  duplication decision above; a shared crate would couple product
  builds to the filetests' dependency surface or force the filetests
  through a product API they don't want.

## Follow-ups

- **Interpreter fuel/loop cap** (with `2026-07-20-lpvm-native-fuel`):
  now that the interp runs in product wasm via lps-probe, bound it —
  either interp-level fuel or worker offload with terminate-on-timeout
  (below) — before hostile/accidental infinite loops meet real users.
- **Worker offload for probe evaluation**: move `run_experiment` off
  the Studio main thread; also the pragmatic mitigation for the fuel
  gap (a hung worker is killable). Trigger: live walks show jank, or
  the fuel follow-up lands here first.
- **lps-glsl as a linter pass**: better spans on the same unit,
  additive to the naga oracle.
- **Dialect gap**: align bare-uniform acceptance between engine
  frontends and the oracle, or lint it in the agent path.
