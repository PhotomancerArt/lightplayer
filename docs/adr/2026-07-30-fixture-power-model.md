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

**The trailing frame is also a startup transient.** Frame 0 has no prior demand
to work from, so it renders unlimited and frame 1 is the first one clamped. At
frame rate this is invisible and harmless — a supply's capacitance covers 16 ms
comfortably — but it means output is *not* frame-invariant while the scale
settles. Golden-frame comparisons must therefore opt out: `examples/shader-oracle`
exists to prove the JIT renders identically on host and device, and sets
`budget_ma: 0` so that current limiting cannot vary what it is measuring.

### The limiter lives in the fixture node

Not in `DisplayPipeline`, which was the first design. Gamma and brightness are
applied in the fixture node; the pipeline's LUT is white-point only. Placing the
limiter in the node puts the budget, the estimate, the scale, and the runtime
slots reporting them in one place, and covers every path that runs the engine —
sim, emu, browser, and GPU preview alike.

The cost is that the estimate does not see the white-point LUT. Its default
scales down, so the estimate over-states draw and limits slightly early — the
safe direction.

### Absent means protected, at 1000 mA, with an explicit opt-out

A fixture that states no budget gets WS2812B at 1000 mA rather than no limiting.
Every project written before the slot existed therefore gains a guard without
being edited — which is the point, since the author most in need of a current
limit is the one who has never heard of the setting. A budget of **zero** means
unlimited, for someone whose supply is genuinely larger than any default.

The failure modes are not symmetric, and that is the whole argument. A budget
set too low dims the show, says so on the fixture card, and is corrected in
seconds. No budget at all lets a board brown out in a reboot loop, silently,
needing bootloader recovery to escape — the failure that prompted this work, and
one that took a code-reading session to diagnose.

**1000 mA** sits between the two available precedents. WLED ships its limiter on
by default at 850 mA, and that default being *too low* is item 3 on its own
top-five mistakes list: dim or dark strips with nothing on screen explaining
why. FastLED ships no default at all and protects only those who already know
the feature exists. We can afford to sit near WLED's conservative end precisely
because the fixture card reports the limiting — a limiter that cannot say what
it is doing is indistinguishable from a broken renderer, which is exactly WLED's
reported symptom.

Note the budget is **per fixture** where WLED's is per device, so a project with
several fixtures can demand a multiple of it. That is deliberate but worth
knowing: nothing here caps a project's total.

### A fixture-wide budget is the simple case of a richer model

`FixturePower { lamp_type, budget_ma }` is a struct rather than a bare
`budget_ma: u32` so that per-domain power budgets — groups of lamps assigned to
their own supplies, owned by the fixture mapper — arrive as an added field
rather than a replacement.

**A device-level total is not the missing piece and should not be built.** Any
fixture or sub-fixture can have its own supply; a single strip with power
injected every few metres is already several domains. Domains cut across
fixtures and inside them, so lamps→supply is the only unit that models reality.
A fixture-wide budget is the degenerate one-domain case of that.

This slice is a **guardrail, not a power model**. It stops the common mistake;
it is not accurate enough to size a supply and does not try to be.

## Consequences

- **Every existing project starts limiting at 1000 mA per fixture.** No file
  needs editing, which is the intent, but it is a behaviour change to output
  everywhere: any fixture whose content exceeds one amp is now scaled down, and
  the fixture card is what explains it. Installations with a genuinely large
  supply must state their budget (or zero) to get their old brightness back.
  This also moves rendered output in previews and story baselines.
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
