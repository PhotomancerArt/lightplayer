# ADR: Fixture power model — model kinds, estimated presets, demand-based limiting

- **Status:** Accepted
- **Date:** 2026-07-30
- **Deciders:** Photomancer
- **Supersedes:** The output node's `brightness` driver option, removed
  outright by this work.
- **Superseded by:** None

## Context

A project whose lamps demanded more current than the supply could deliver
would light up at power-on, brown the board out, reboot, auto-load the same
project, and loop. Nothing in the system had any idea how much current a
frame was about to ask for.

`lp-recovery` already carries a boot-loop ladder that enters safe mode after
repeated incomplete boots, but it does not catch this: `mark_boot_complete()`
fires on the first successful frame, so a board that renders one bright frame
before dying resets the counter every reboot, and `ResetCause::Brownout` is
excluded from `blames_code()` besides. Repairing that ladder, and the
device-level safe clamp it would drive, are a **separate follow-up plan**; this
one addresses the root cause instead of the symptom.

Two brightness scalars also sat in series at different layers and fought each
other: `FixtureDef.brightness` (u8 0–255, the fixture card's front-panel fader)
and `OutputDriverOptionsConfig.brightness` (a 0..1 ratio baked into the display
pipeline at construction).

## Decision

### Brightness belongs to the fixture, not the output

`OutputDriverOptionsConfig.brightness` is removed. An output drives
already-rendered bytes; brightness is a property of the lamps. Per the
wire-compat policy in `AGENTS.md` the old form was deleted outright with no
aliases or dual-decode paths, and `WIRE_PROTO_VERSION` went to 4.

### Lamp types carry a power model kind, not a milliamp number

`PowerModel` has two variants because 5V and 12V parts differ structurally, not
merely numerically:

- `LinearPerChannel` — one driver channel per colour die; draw scales with duty
  plus a per-LED quiescent term. 5V WS2812-family, and 12V per-pixel parts whose
  constant-current drivers simply run lower.
- `SeriesGroup` — one channel feeds several LEDs in series, so the channel's
  current covers the whole group. 12V WS2811 strips run three LEDs per chip;
  treating them as three independent pixels over-estimates draw about threefold.

Both carry a quiescent term. It is colour-independent, so it **dominates at low
brightness** — exactly where installations run — and a duty-only formula omits
it entirely.

### Presets are code, and are honest about being estimates

Lamp behaviour lives in a `const` table keyed by `LampType`, not in the project
file. A project stores only the lamp's name, so corrected numbers reach existing
projects without touching them, and no project can author a bogus power model.

Every preset is tagged `PowerProvenance::Estimated` — assembled from datasheets
and community figures, never measured here. A test asserts that nothing claims
`Measured`. All user-facing copy says "estimated", and the readout leads with
`≈`. The intended path to real numbers is an on-device `test_power` harness
alongside the existing `test_gpio_calibrate` / `test_dither` harnesses; the
provenance field exists so that upgrade is additive.

### Limiting is cap-only, demand-based, and applied after gamma

Three ordering decisions, each of which fails *quietly* if reversed:

**The scale is applied after gamma correction.** Gamma is nonlinear, so scaling
its input by `s` changes emitted duty by roughly `s^2.2` — a scale meant to shed
20% would shed nearly half, and the limiter would still look like it worked.
This is also why the scale could not simply fold into the existing brightness
multiply, which sits before gamma.

**Demand is accumulated pre-scale.** Summing what was actually emitted closes a
feedback loop: scale down, sum falls, scale rises, brighten, repeat. It presents
as the fixture pumping once per frame and reads as a slew-rate bug. Demand is a
function of content alone, so the scale converges.

**The quiescent floor comes out of the budget before the scale is derived.**
Only the duty term responds to scaling; dividing the whole budget by the whole
estimate counts the floor twice and settles permanently *above* budget. A
fixture whose lamps idle over budget sheds all light rather than pretending.

The scale is slew-limited — down instantly, up over roughly two seconds — and
applies on the frame *after* the demand that produced it. That trailing frame
is a real trade-off: a single frame can exceed budget, which suits a supply with
capacitance and does not suit a hard current limit.

### The limiter lives in the fixture node

Not in `DisplayPipeline`, which was the first design. Gamma and brightness are
applied in the fixture node; the pipeline's LUT is white-point only. Placing the
limiter in the node puts the budget, the estimate, the scale, and the runtime
slots reporting them in one place, and covers every path that runs the engine —
sim, emu, browser, and GPU preview alike.

The cost is that the estimate does not see the white-point LUT. Its default
scales down, so the estimate over-states draw and limits slightly early — the
safe direction.

### A fixture-wide budget is the simple case of a richer model

`FixturePower { lamp_type, budget_ma }` is a struct rather than a bare
`budget_ma: u32` so that per-group power domains — groups of LEDs assigned to
their own supplies, owned by the fixture mapper — arrive as an added field
rather than a replacement. Absent `power` means no limiting; there is no default
budget, because a wrong guess is worse than none in both directions.

## Consequences

- Projects authored before this change keep working untouched: no `power` slot
  means no limiting and no per-channel cost.
- Every project carrying an output-node `brightness` had to drop it in the same
  change (13 example and test project files); unknown fields are rejected, not
  ignored.
- Measured at dome scale (30k lamps, min-of-25 on host), the per-lamp
  channel-write loop goes 0.0466 → 0.0568 ms/frame under budget and 0.0665
  ms/frame while shedding: 0.010–0.020 ms of added work per frame, under 1% of a
  60 Hz frame. ESP32-C6 image grew 2,048 B against 273 KB of headroom. Limiting
  therefore stays on whenever a budget is set — a safety feature that defaults
  off protects nobody.
- `next_scale_q16` is a pure function of `(estimate, budget, previous scale,
  dt)`. The follow-up plan that adds a device-level safe clamp can compose an
  outer ceiling over it without restructuring anything here.
- The preset numbers are the weakest part of this and are known to be. They are
  good enough to keep a board from browning out and not good enough to size a
  supply to the last milliamp.

## Alternatives considered

- **Limiter in `DisplayPipeline`.** Rejected: see above. It would also have
  needed a driver→engine back-channel to display a scale computed downstream of
  the slots that report it.
- **A flat mA-per-channel figure per lamp type.** Rejected: it cannot express
  the 12V series-group topology, and silently under-estimates those parts.
- **A default budget.** Rejected: too low throttles a working installation, too
  high gives false confidence. Absence is the honest state.
- **Shipping presets untagged.** Rejected: without provenance there is no way to
  tell a measured number from a guess later, and the UI would have no basis for
  saying "estimated".
