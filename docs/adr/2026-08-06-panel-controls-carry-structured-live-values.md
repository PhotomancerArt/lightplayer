# Panel controls carry structured live values beside the display string

- Status: accepted
- Date: 2026-08-06
- Area: `lpa-studio-core` panel-control DTO (`UiPanelControl`,
  `UiBindingEndpoint`), `lpa-studio-web` panel renderers

## Context

A panel control's live reading has always been **one pre-formatted string**,
`live_value: Option<String>`. That shape is load-bearing for a reason: values
are quantized to ≤2 decimals, and monotonic/time-kind channels excluded,
*before* the string enters any DTO, so a channel that drifts every engine tick
does not dirty the whole-DTO change gate 60 times a second
(`ProjectController::apply_bound_live_values`, `live_channel_value`).

Scalars survive that round trip — `live_numeric()` and `live_bool()` simply
re-parse the string. A `GradientConfig` cannot. So the palette swatch rendered
the **authored** config even while a channel drove the slot, and because the
slot is public (`panel: "show"`, or an authored `bus:palette` binding), every
pick took the panel-write path: it wrote the channel and left the authored
value untouched. The control could therefore never show what it had itself
just written. Picking Lava (16 stops) turned the light red and left the swatch
drawing the default black→white ramp with the readout still reading `2 stops`.

The failure was not cosmetic. The chooser expresses every gesture as a whole
replacement of the config the control is showing, so while that was stale, a
read-modify-write lost data: adding two palettes to a cycle produced a
one-member set, because the second add rebuilt from the same stale value the
first one did.

Three places in the tree recorded the old behaviour as a decision — "text, not
a config the swatch could sample". They were describing a limitation of the
string, not a principle, and are amended alongside this change.

## Decision

A panel control carries the structured live value **alongside** the display
string, and **narrowly** — gradient-shaped (`live_gradient:
Option<GradientConfig>`), not a general `LpValue`.

`UiPanelControl::shown_palette()` is the palette counterpart of
`shown_display()`: the live reading when a channel is driving the slot,
the authored config otherwise. Both panel renderers ask it. The authored value
keeps its home in the control's detail popover, exactly as the authored scalar
does behind `shown_display()`.

The write path is unchanged; only what comes back is.

## Why narrow, and why this is not a special case

Generalising `live_value` to an `LpValue` would reintroduce for **every**
control the per-tick churn the string form exists to prevent. A gradient can
ride structurally at no churn cost because it only moves when someone writes
it.

That is the same argument the tree already makes for `PhasorConfig`, in
`live_channel_value`: *"the value only moves when someone writes it … so the
churn worry behind the instant exclusion does not apply, and the speed knob
riding the channel needs the reading to track its own writes."* The phasor got
a scalar it could round-trip through the string; the palette needed the
structured counterpart of the same idea.

So the rule is: **a control whose value cannot round-trip through display text
gets its own narrow field.** The next non-scalar panel control should add one
rather than widen this into a general value channel.

## Consequences

- `UiBindingEndpoint` and `UiPanelControl` gain one `Option<GradientConfig>`.
  Wire cost is small: a gradient is a compact stops literal since
  `2026-08-05-gradient-stops-string-storage` (~12 B/stop), so even a maximal
  8×24 cycle is ~4.4 KiB — inside the 16 KiB project-read frame budget that
  the stops-string rewrite was designed around.
- `Eq` comes off `UiBindingEndpoint` and the types embedding it: a gradient
  carries `f32`. This matches `UiConfigSlot` and `UiPanelControl`, which have
  always been `PartialEq`-only for the same reason; nothing keyed these in a
  hash or tree set.
- Two gaps surfaced while wiring it, both fixed here: `live_channel_value` had
  no gradient branch at all (so probe truth for a palette channel produced no
  display reading either, and the row fell out of `apply_bound_live_values`
  before reaching any new code), and the seam that actually feeds a **shader
  uniform** is the binding overlay builder — both the synthesized and the
  default-origin path — not that later pass.
- The chooser's "This project" section can now include a **held** palette, not
  only authored ones, which is what makes the gradient editor reachable at all
  in a project that has authored nothing.

## Alternatives considered

- **Generalise `live_value` to `LpValue` and format at render time.** Cleaner
  in principle; reintroduces scalar churn through the change gate, which is
  the thing the current shape was built to avoid.
- **Have the swatch re-read the channel itself.** Forks the derivation: the
  control panel and the slot row would answer "what is playing" from different
  sources.
- **Make the pick a slot edit instead of a panel write.** Would make the
  authored value the truth again, but panel writes are deliberate — a pick is
  a runtime poke, not an authored change, and it must not dirty the project.

## References

- `docs/adr/2026-08-05-gradient-stops-string-storage.md`
- `docs/adr/2026-08-04-palettes-are-values.md`
- `docs/design/panel.md` (P6 — live reading vs authored value on a control)
- Plan: `2026-08-06-1920-palette-panel-feedback`
