# TimeProduct M2 — P5 migration sweep ledger

Every `bus:time`-consuming artifact in the tree, what it became, and what the
oracle measured. Produced by `scratch/time-sweep/` (disposable; deleted in P9).

- **31** shader/compute defs consumed time before the break. **26** converted
  here; **5** are `node:..#entry_time`-bound and stay f32 (settled Q3).
- **18** unique GLSL bodies behind those 26 defs. 3 keep byte-identical GLSL
  (the conversion is def-only); the 3 multi-file bodies stay byte-identical
  across their copies (asserted by `sweep.py apply`).
- **6** authored `"time": 0` value lines deleted (P4's red list).
- `examples/shader-oracle` and `projects/test/quad-wire-oracle` are
  time-invariant by design and were **not touched**.

## How to re-run

```bash
python3 scratch/time-sweep/sweep.py apply    # rewrite glsl + defs from the table
python3 scratch/time-sweep/sweep.py cases    # emit the oracle cases
cd scratch/time-sweep/oracle
cargo run --release --bin time-sweep-oracle  # lps-probe A/B  -> oracle-results.json
cargo run --release --bin render_check       # engine @ t=5 s -> render-check.json
python3 scratch/time-sweep/precision_witness.py
```

## The oracle

For each unique body, `lps_probe::run_experiment` on the LPIR **f32
interpreter**, `render(pos)` over an 8×8 grid (64 sites × 4 components),
`reduce: none`, at `t ∈ {0, 0.7, 3.333, 17.9, 100.1}`. Side A is the committed
body with `time = t`; side B is the converted body with each phasor uniform
set to `fract(t / period + offset)` and each seconds uniform set to `t`. Every
non-timebase uniform is pinned to its authored default on both sides, so the
measurement isolates the rewrite. Threshold: max abs diff ≤ 1e-5.

Two guards ride along:

- **self-variation** — how far side A moves between `t=0` and each later `t`.
  A body that does not move is not exercising the timebase, so its agreement
  would prove nothing. Every case moves ≥ 0.24 (most ≥ 0.5) on a 0–1 scale.
- **lit fraction** on BOTH sides. No case went black where the original was
  lit, at any `t`.

Compute bodies have no `render()`. `fluid/compute.glsl` is oracled through two
`render()` shims carrying its seven emitter terms verbatim
(`shims/fluid-{a,b}-{orig,conv}.glsl`); `meteor/sim` and `events/event_{a,b}`
keep byte-identical GLSL, so their A/B is the identity.

## Per-body ledger

`worst` = max abs diff over the whole t-grid. `lit` and `motion` come from the
**real engine** (project load → ClockNode → timebase store → phasor slot
evaluation → Q16.16 lpvm shader), ticked 200 × 25 ms: `lit` is the fraction of
a 16×16 primary-visual frame above black at t = 5 s, `motion` is the mean
per-channel |Δ| (16-bit units, full scale 65535) between the t = 1 s and
t = 5 s frames. A still frame would prove nothing about a timebase, so both
are recorded; every project animates.

| body (files) | class | uniforms after | worst | at t | lit @5s | motion 1s→5s |
|---|---|---|---|---|---|---|
| `fast` (1) | periodic | `phase`=1 s | 1.53e-6 | 100.1 | 100.0% | 410 |
| `fiber-headband` (1) | periodic | `phase`=8.333333 s | 3.70e-6 | 100.1 | 100.0% | 29757 |
| `rocaille` (1) | periodic | `cycle`=20 s | 5.96e-8 | 100.1 | 100.0% | 2772 |
| `quad-strips` (**7**) | periodic | `phase`=20 s | **1.01e-5** | 100.1 | 100.0% | 3175 |
| `penta-strands` (1) | periodic | `phase`=20 s | 7.75e-6 | 100.1 | 100.0% | 5969 |
| `plasma` (1) | periodic (driven) | `phase`=100 s ← `bus:speed` | 4.92e-6 | 100.1 | 100.0% | 21366 |
| `smoke-project` (1) | periodic | `wavePhaseA`=2.991993 s, `wavePhaseB`=3.695991 s, `crossPhase`=4.83322 s, `huePhase`=12.5 s | **2.62e-5** | 100.1 | 100.0% | 18861 |
| `basic2` (1) | periodic | `panPhase`=20.94395 s, `scalePhase`=8.975979 s, `huePhase`=6.283185 s | 5.25e-6 | 100.1 | 100.0% | 16407 |
| `basic` (1) | both | `time`=seconds, `palettePhase01`=25 s, `panPhase`=20.94395 s, `scalePhase`=8.975979 s | **0** | — | 100.0% | 13412 |
| `perf` (**2**) | both | `time`=seconds, `palettePhase01`=25 s, `panPhase`=20.94395 s, `scalePhase`=8.975979 s | **0** | — | 100.0% | 13844 |
| `button-idle` (**2**) | both | `time`=seconds, `wavePhase`=17.95196 s, `palettePhase`=25 s | 8.79e-7 | 100.1 | 100.0% | 6684 |
| `fyeah-attract` (1) | periodic (driven) | `wheelPhase`=4.347826 s, `paletteCycle`=27.27273 s | **1.53e-5** | 100.1 | 50.4% | 9226 |
| `fyeah-idle` (1) | both (driven) | `time`=seconds, `zoomPhase`=19.63495 s, `driftPhase`=34.90659 s, `bandPhase`=7.391983 s, `breathPhase`=8.377581 s, `paletteCycle`=18 s | 5.04e-6 | 17.9 | 100.0% | 4596 |
| `fyeah-idle-plain` (1) | both | same five phasors + `time`=seconds | 5.04e-6 | 17.9 | 100.0% | 4596 |
| `fluid-compute` (1) | periodic | `wave_a`=20.26834 s, `wave_a2`=27.76485 s, `wave_b2`=33.72617 s off 0.270723, `wave_b`=27.3182 s off 0.334225, `wave_c`=33.0694 s off 0.668451, `wave_c2`=49.35731 s off 0.447862, `wave_breathe`=34.90659 s | 1.97e-6 | 100.1 | 98.8% | 7500 |
| `meteor-sim` (1) | unbounded | `time`=seconds (GLSL unchanged) | identity | — | 33.6% | 2453 |
| `events-a` (1) | unbounded | `time`=seconds (GLSL unchanged) | identity | — | 100.0% | 307 |
| `events-b` (1) | unbounded | `time`=seconds (GLSL unchanged) | identity | — | (shares the events project) | |

Sparse frames are the pattern, not a defect: `meteor` is four dots on a dark
field, `fyeah-attract` is a rim wheel with two dark wedges, `fluid` fades at
one corner. `fast`'s small motion number is also correct — its period is 1 s,
so t = 1 s and t = 5 s land on nearly the same phase by construction.

## GPU corpus

`lp-gfx-wgpu`'s parity corpus (`tests/util/corpus.rs`) embeds four of these
bodies verbatim and binds `outputSize`/`time` only. Since binding is by name
and `apply_uniform_fields` errors on a **missing** declared uniform, the
corpus gained a `phasors: &[(name, period)]` field and the harness evaluates
each as `fract(time / period)` per timestamp. `fyeah_idle` also gained the
`glow` binding it was already missing before this sweep. `cargo test -p
lp-gfx-wgpu` is green on a real Metal adapter, `corpus_parity_holds_or_beats_m3`
included — so the converted bodies also compile through **naga** and hold the
m3 parity bounds.

## Conversion notes, body by body

**`fast`** — `mod(time, 1.0)` *is* a phasor; the uniform is read straight out.

**`fiber-headband`** — `fract(time*0.12 + led*0.5)` → `fract(phase + led*0.5)`.

**`rocaille`** — `mod(time*0.05*TAU, TAU)` ≡ `TAU*fract(time*0.05)`. Exact for
*any* TAU literal, because the shader's own constant is both the scale and the
modulus; the 6.28318 spelling is left alone.

**`quad-strips` / `penta-strands`** — per-band rates (0.25…0.65 Hz) are whole
multiples of 0.05 Hz, so one 20 s phasor carries all bands:
`fract(phase*(5 + 2*band) + band*k)`. Whole multiples are what makes it exact —
the cycles the wrap skips are a whole number of the band's own cycles.

**`plasma`** — the five field rates (0.13/0.09/0.11/0.15/0.05 Hz) are whole
multiples of 0.01 Hz → **one** base phasor, each field riding `phase*13`,
`phase*9`, `phase*11`, `phase*15`, `phase*5`. The `speed` uniform is retired
and the phasor's `default_bind` points at the retyped `bus:speed` **config
channel** (period-driven, D3). Old-feel mapping: `speed` had range 0…4 with
default 1, and `speed=1` is `period = 100 s`; the whole old slider is
`period ∈ [25 s, ∞)` with 0 = frozen. See deviations for the 8 s question.

**`smoke-project`** — four incommensurate rates, four phasors. The file's TAU
literal was tightened 6.28318 → 6.2831853: 6.28318 is 5.1e-6 off 2π, and this
body runs 33 cycles by t=100 s, so the "whole cycles" the wrap skips would not
have been whole. The tightening also shifts the palette by ~3.3e-6 at every t
(visible as the t=0 floor in this row).

**`basic2`** — Worley is sampled at a time-independent coordinate, so nothing
is unbounded: three phasors, no seconds. `worley_demo` moved above `render` —
the committed order is call-before-declaration, which the naga front end
rejects outright ("Unknown function"). The oracle baseline carries the same
reorder so the A/B stays like-for-like.

**`basic`, `perf`** — split. Palette walk, pan and zoom → phasors; `prsd_demo`
keeps raw seconds, because psrdnoise's `alpha` rotation and a `mod(…,1)` hue
walk are tangled in one argument. `perf`'s `mod(time,5)` and `mod(time*0.2,5)`
both fold onto the single 25 s phasor. **Both are bit-exact** (max diff 0.0
across the whole grid): the phasor-fed `sin` values round to the same f32 as
the originals, and the seconds half is untouched.

**`button-idle`** — the fbm coordinate scroll stays seconds; the wave and the
palette walk become phasors.

**`fyeah-attract`** — `speed` was a **local** knob here (no bus binding), so
its default 2.0 is baked into the two periods and the uniform is retired. The
two rates share no useful base (69:11 over a 600 s cycle), so they get one
phasor each.

**`fyeah-idle`** — five wrapped terms → five phasors; the psrdnoise scroll
keeps seconds. `speed` retired at its default 1.0 (see deviations).

**`fyeah-idle-plain`** — the knob-free sibling in `projects/test/`; same split.

**`fluid-compute`** — seven standalone `sin(time*k + c)` terms → seven phasors,
each additive constant folded into `phase_offset` (that is exactly what the
field is for). Compute body, so it is oracled through render shims.

**`meteor-sim`** — the sanctioned integrator: `dt = time - prev_time`. Stays
unbounded seconds; `speed` stays a plain f32 that scales `dt` (a phasor would
rewind `dt` once per cycle and stall the meteors). GLSL byte-identical; only
the slot kind and the now-redundant `bus:time` binding changed. Pinned by
`meteor_sim_keeps_an_unbounded_seconds_uniform`.

**`events-a` / `events-b`** — `uint(time*2)` is a monotone counter feeding an
event sequence number; a wrapped phase would replay ids. Seconds, GLSL
byte-identical.

## The three over-threshold rows

`quad-strips` (1.01e-5), `fyeah-attract` (1.53e-5) and `smoke-project`
(2.62e-5) exceed 1e-5 — all of them **only at t = 100.1**, and all within 3×
the gate. The per-t curve is the tell (diff grows monotonically from exactly
0 at t=0):

| body | t=0 | t=0.7 | t=3.333 | t=17.9 | t=100.1 |
|---|---|---|---|---|---|
| `quad-strips` | 0 | 0 | 1.3e-6 | 8.3e-6 | 1.01e-5 |
| `fyeah-attract` | 0 | 0 | 0 | 5.2e-6 | 1.53e-5 |
| `smoke-project` | 3.3e-6 | 3.3e-6 | 5.1e-6 | 7.2e-6 | 2.62e-5 |

The algebra did **not** change; f32 resolution did. `precision_witness.py`
recomputes the one timebase-carrying term of each body in f64 and measures
three candidates against it at t=100.1:

```
quad-strips band 2 (0.45 Hz)   orig=1.83e-6  conv=6.99e-7  conv-ideal=1.67e-8
fyeah-attract wheel phase      orig=7.17e-7  conv=8.76e-8  conv-ideal=6.89e-8
smoke-project wave A           orig=1.11e-6  conv=2.19e-6  conv-ideal=9.35e-7
```

`conv-ideal` is the converted expression fed an f64-accurate phase, and it is
20–250× closer to exact than either measured side. So:

1. The rewrite itself is exact — feed it an accurate phase and the answer is
   right to ~1e-8.
2. The original loses precision because `time*k` grows without bound
   (`100.1*2.1 = 210.2` has an f32 ulp of 1.5e-5 — larger than the whole
   threshold). This is the migration's own argument, measured.
3. The oracle's phase model, `fract(t/period)` in f32, has its own
   cancellation at large `t`. **The runtime does not use it**: the timebase
   store *integrates* (`advance(state, rate_hz * delta)`), so its phase never
   evaluates `t/period` and never suffers this loss.

A steep `smoothstep` edge (0.12 wide in `quad-strips`, 0.08 in
`fyeah-attract`) multiplies a ~1e-6 phase error into ~1e-5 of colour on one
grid cell. Nothing is visible: the engine render check is 100% lit for all
three, and the `basic`/`perf` rows show that where the arithmetic can be
bit-exact, it is.

Recorded as an explained exception rather than widened — the gate stays 1e-5.

## Deviations from the phase file

1. **plasma's authored period is 100 s, not 8 s.** The phase file suggests an
   8 s default. plasma's fastest field rides `phase*15`, so an 8 s base period
   runs the whole pattern **12.5× faster** than the committed `speed=1` — the
   opposite of "map old feel". 100 s reproduces the current look exactly. The
   period is panel-exposed and channel-driven, so G2 can dial it in one
   gesture; 8 s is one keystroke away if Yona wants it.
2. **Only plasma gets a config channel.** The notes name four "driven-speed"
   shaders, but only three actually bind `bus:speed`
   (plasma, fyeah-sign/idle, meteor/sim) — `fyeah-button/attract`'s `speed` is
   a local knob with no binding. Of the three, meteor keeps a plain f32
   `speed` by doctrine (it scales `dt`), and fyeah-sign/idle cannot use one: a
   config channel drives **one period**, and that body has five incommensurate
   wrapped rates plus a seconds term that `speed` also multiplied. Its `speed`
   uniform is retired at its default 1.0. plasma — the G2 demo, and the one
   body with a genuine single base rate — is the driven case.
3. **`fyeah-button/attract` and `fyeah-sign/idle` lose their Speed knobs.**
   Migration-visible UX change, sanctioned by the phase file's "retire the
   speed multiplier uniform". Their motion is now per-phasor periods (panel
   exposes period in v1), which is a units inversion: bigger = slower.
4. **`smoke-project`'s TAU literal changed** (6.28318 → 6.2831853). Required
   for correctness, not cosmetic — see above.
5. **`basic2`'s helper order changed.** Also a fix, not a preference: the
   committed order does not compile under naga at all.
6. **The interactive dev-server eyeball was skipped** per the phase prompt;
   the engine render check at t=5 s (lit + 1s→5s motion) stands in, and G2
   covers visual verification with Yona.
7. **`lps-glsl`'s `fluid_compute_example_stays_compact_at_lpir`** synthesizes
   its own compute header around the example body; its `uniform float time`
   became the seven `wave_*` uniforms. The LPIR op counts (34 stores, 0
   selects, < 180 ops) are unchanged.
8. **The lpa-link device trace was version-skipped**, not regenerated.
   `s3-current-fw-valid-project.jsonl` embeds a **format 2** project — already
   several bumps stale before this break — and nothing parses those payloads:
   `trace_replay.rs` feeds only `rx` lines to the boot-line classifier.
   Regeneration needs a board (`just device-scenario run s3`), and the phase
   file forbids hand-editing recorded frames. Documented in that directory's
   README instead.
