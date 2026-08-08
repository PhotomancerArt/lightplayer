# lps-probe

Experiment/probe evaluation for GLSL shaders on the LPIR f32 interpreter.
This is the pure core of the shader agent's `iterate` tool: given user GLSL
and an `ExperimentSpec`, it compiles the shader plus per-probe wrapper
functions (naga frontend → LPIR) and evaluates them on the interpreter,
returning structured diagnostics, an always-on health report, and per-probe
results.

`no_std` + `alloc`, sans-IO (no clocks, no randomness, no executors), and it
compiles for `wasm32-unknown-unknown` — it runs inside the Studio wasm app.

## API

```rust
let result = lps_probe::run_experiment(source, &spec);       // ExperimentResult
let report = lps_probe::diff_experiments(&prev, &cur, &spec); // DiffReport
```

- `ExperimentSpec { size, bindings, probes, led_points }` — all fields have
  serde defaults; `{}` is a valid health-only experiment. `size` (default
  256×256) maps normalized [0,1]² coordinates to pixel-space `pos` and is
  written to the `outputSize` uniform iff the shader declares it.
- `ProbeSpec { id, ty, expr, domain, vary, reduce }` — `expr` is a GLSL
  expression (it may call `render()` and user functions) wrapped as
  `<ty> __probe_<id>(vec2 pos) { return (<expr>); }` (or `float <var>` for
  `Sweep` domains). Ids must match `[a-z0-9_]+`.
- Domains: `Point`, `Points`, `Grid` (cell centers of a rect, default the
  unit square), `Line` (inclusive endpoints), `Leds` (caller-supplied
  points), `Sweep` (scalar parameter instead of a position).
- Reductions: `none` (raw rows, tightly capped), `stats` (per-component
  min/max/mean + NaN/Inf counts per vary step), `histogram { bins }`
  (all components pooled, per vary step).
- `ExperimentResult { shader, compiled, health, probes, warnings }` — the
  compiled module is kept (`compiled`, serde-skipped) so a session can cache
  it and later call `diff_experiments` against a newer compile.
- Spec and result types are serde round-trippable; `ExperimentResult::rounded()`
  / `DiffReport::rounded()` round result floats to 4 significant digits for
  transport (specs are never rounded). `int` probe values are reported as f32.

## Entry contract

User shaders define `vec4 render_2d(vec2 pos)` with `pos` in shader pixel
space; `outputSize`/`time` are ordinary uniforms by convention. A shader
without a conforming `render_2d` is reported as a compile outcome error.

## Semantics: f32 oracle

Evaluation uses the LPIR **f32 interpreter** with host-side libm imports —
the same oracle semantics as the `lps-filetests` conformance harness. It is
*not* the Q32 device path: expect bit-exact f32 math, not device-identical
output. Canonical `lpfn_*` builtins are compiled from their canonical GLSL
sources (with the oracle's `lpfn_` → `lpo_` rename), so probes and shaders
may call them freely.

## Determinism contract

Evaluation order is fully deterministic:

1. One interpreter instance per probe compilation; `__shader_init` runs once
   at instantiation; then `outputSize` (if declared) and `spec.bindings` are
   applied in map order.
2. For each `vary` value **in the given order** (outer loop): write the
   varied binding, then evaluate every site in domain order (inner loop).
3. Global (module-scope) variable writes persist across calls within the
   instance, so ordered `vary` steps approximate frame-sequential behavior
   for shaders with global state.
4. Health evaluates `render` over a 16×16 grid (cell centers) plus all
   `led_points`, under `spec.bindings`, no vary — whenever the shader
   compiles, even with zero probes. Diff reuses the same sites.

Grid sites are cell centers (`(i + 0.5)/n`), row-major (y outer); `Line` and
`Sweep` are endpoint-inclusive with `n` evenly spaced samples.

## Diagnostics

The bare shader is compiled first: its diagnostics are the user-meaningful
ones, with positions in user-source coordinates. Each probe is then compiled
as shader + that one wrapper (compile isolation: a broken probe fails only
itself). Probe diagnostics at or after the wrapper line are remapped to
`probe '<id>' expr:<line>:<col>` (expression-relative). Bindings that don't
match a declared uniform are warnings, never errors.

## Caps

One place: `experiment.rs`.

| Cap                   | Value     | On violation                        |
|-----------------------|-----------|-------------------------------------|
| `MAX_PROBES`          | 8         | later probes `Skipped`              |
| `MAX_EVALS_PER_PROBE` | 4096      | probe `Skipped` (\|domain\|×\|vary\|) |
| `MAX_RAW_VALUES`      | 64        | probe `Skipped` (reduce `none` only) |
| `MAX_TOTAL_EVALS`     | 16 384    | probe `Skipped` (probes + health)   |
| `MAX_OPS_PER_EVAL`    | 1 048 576 | interpreter errors mid-call         |

Every `Skipped` reason states what to change (add a reduce, shrink the
domain, remove probes).

`MAX_OPS_PER_EVAL` is the interpreter op budget ("fuel") for a single
evaluation — the guard that turns an infinite-loop shader into an
actionable error instead of a hung Studio tab (the LPIR interpreter has no
other termination bound; see `lpir::InterpLimits`). An exhausted evaluation
skips its probe (with the "infinite or excessively long loop" reason), and
health evaluation bails after a few consecutive failures, so a
non-terminating shader costs a handful of bounded calls per experiment.

## Provenance

The interpreter harness (`interp_harness.rs`) and canonical-unit assembly
(`canonical_unit.rs`) were extracted from `lps-filetests`
(`src/test_run/interp.rs`, `src/conformance/oracle.rs`) and adapted:
no-`anyhow` string diagnostics, no texture specs, and persistent VMContext
state across calls (the filetests copies remain authoritative for filetest
semantics; see the breadcrumb comments there).
