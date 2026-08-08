# Time is a product: `bus:time` carries a queryable handle, and the clock owns all integration

- Status: accepted
- Date: 2026-08-04
- Context: closes the TimeProduct/phasor design arc (parent plan
  `planning/2026-08-03-2203-timeproduct-phasors`, G1 register D1–D12
  settled 2026-08-03; implemented by
  `planning/_archive/2026-08-04-0003-timeproduct-m2-core`, merged as
  PR #328). The personally-felt defect this kills: plasma's speed
  slider janked because every consumer multiplied raw seconds by a
  live-editable rate — `fract((t)·k)` jumps when `k` moves. The G2
  gate (2026-08-04, live) confirmed the fix on plasma: period sweeps
  with no phase jump, scrubbing reproduces motion bit-exactly.
- Plan: `planning/_archive/2026-08-04-0003-timeproduct-m2-core`
  (external planning root). Design spike for the follow-on face:
  `spikes/clock-phasor-face/index.html`.

## Decision

### 1. `bus:time` carries a `TimeProduct` — raw seconds never ride the bus

The clock publishes a **product handle** on `bus:time` (the
`visual.out`/`control.out` precedent), not an `f32`. Everything behind
the handle — effective seconds, this tick's delta, every live phasor —
lives in an engine-owned **`TimebaseStore`** keyed by clock `NodeId`
(the `PanelWriterStore` side-store precedent: survives
`apply_project_changes` by construction). Queries are store lookups +
arithmetic on `TickContext` — **no `NodeCall` dispatch, no `Executing`
guard** — because uniform fill is the hottest per-frame path and the
frame-cost budget is led by resolver/endpoint machinery already.

An `f32` slot bound to `bus:time` is a **loud** kind-mismatch: the
conversion failure lands the authored default plus a card `Warn`
(the PR #316 pattern) — never silent black (D12: fails loud, keeps
running). Project format bumped 4→5; format-4 artifacts refuse per the
alpha posture (version-and-refuse, never migrate silently).

### 2. Two declared timebase shapes, evaluated at uniform fill

`ShaderSlotKind` gains **`phasor`** and **`seconds`** (GLSL sees plain
`uniform float` either way — frontends unchanged):

- **`phasor`** declares periodic consumption. The def carries
  `PhasorConfig { period_seconds (0 = frozen), waveform, phase_offset }`.
  The store integrates the **raw ramp** (`φ += Δt/T`, wrap into
  `[0,1)`, cycle count); **shaping is applied by the evaluator, never
  the integrator** — waveform and phase_offset are ALWAYS slot-local,
  so one shared integrator can serve many differently-shaped readings.
  A binding on a phasor slot names a **config channel** whose value is
  a `PhasorConfig`, of which a driven channel supplies
  **`period_seconds` only** (same reason).
- **`seconds`** declares genuinely unbounded consumption (noise
  advance, dt integration). It takes no bindings: it always reads the
  scope's time product.

Phase is continuous under period edits by construction (the integrator
never re-derives `φ` from `t/T`), which is the entire point.

### 3. Identity is the resolved config's provenance — no id field

A phasor's identity is **where its config came from**, computed at
evaluation, never authored: authored-local (or unwritten-channel
fallback) ⇒ `Private(node, slot)`; resolved from a channel writer ⇒
`Shared(scope, channel)` — one integrator for every reader of the
channel, phase-locked automatically (D3). The private↔shared
transition resets φ; that is correct, not a defect ("**grabbing the
reins**"). Lifecycle is demand-driven: materialize on first query,
despawn after a swept horizon of silence; nothing persists.

### 4. Scrub-exactness via a breakpoint log — hosts only, and it changes the live path

Studio/sim tiers keep an event-sparse per-phasor log of
`(t_eff, φ, cycle, rate)` breakpoints (feature `scrub-log`, default-on
for hosts, OFF for every `fw-esp32*` — zero log symbols in the shipped
ELF, pinned). A query behind the live edge answers by closed-form
segment lookup; punch-in writes while scrubbed truncate the
provisional future; a 30 s window + entry cap bound memory.

The load-bearing implementation fact: **with the log on, the LIVE path
evaluates the same closed form** — bit-exact reconstruction against a
running float integrator is impossible (different summation order), so
hosts do segment evaluation everywhere. Firmware keeps the plain
forward integrator byte-for-byte; the two tiers differ by last-bit
phase, and both behaviors are pinned in both feature configurations.
Devices handle a backward scrub as a negative delta (monotone
integration downward) — accepted, logless.

### 5. The break was atomic; the panel speaks speed, unit-aware

Removing seconds from the bus breaks every consumer, so the bind swap,
the full gallery migration (26 defs over 18 bodies, lps-probe A/B
oracle, ledger in the archived plan), and the shader-agent doctrine
(periodic → phasor, unbounded → declared seconds; never
`fract(time/period)`) landed as one unit. `entry_time` stays `f32`
(entry-relative, node-ref bound — cannot rot; settled Q3).

The panel control for a phasor slot is a **Speed knob** (period only,
D11 v1): drag axis up = faster, local echo + throttled writes, readout
**auto-denominated** — `2/s → 3/min → 15/hr`, the smallest time unit
keeping the count ≥ 1 (frozen = `0/s`). That display rule is a
product-wide principle (unit-awareness), first shipped here.

## Consequences

- Multi-device phase-locked sync stays reachable: rates change at
  discrete events and φ is piecewise-linear, so timebase sync + event
  distribution is a transport problem, not a redesign (the Pixelblaze
  purity-vs-continuity dichotomy is dissolved rather than chosen).
- `seconds()` consumers inherit the precision debt raw time always
  had (`docs/debt/bus-time-precision.md`: f32 ulp rot ~8 ms/day,
  Q16.16 ceiling ±32768 s); phasors dodge the wrap but host phase is
  still a function of f32 effective seconds.
- The clock card is becoming the phasor observability surface: the
  probe streams per-integrator rows (wire proto 10), growing
  per-reading shaping en route to trace cards
  (`planning/2026-08-04-1440-clock-face-v2`, PR #335, proto 11).
- The transport UI was still owed at the time this ADR was written — the
  engine half (this log) existed; the Debug scrub slider was the only
  driver. **Closed 2026-08-07**: `docs/debt/clock-transport-has-no
  -transport-ui.md`'s exit criteria are met — a tape transport instrument
  on the clock card (plan `2026-08-04-2355-clock-tape-hero` P3–P5) and on
  the module panel (P6/P8), per
  `docs/adr/2026-08-07-clock-transport-is-a-panel-instrument.md`.
- Wanting waveform/offset on a panel is answered by a future **LFO
  node**, not by widening the panel contract (modules.md §10).
