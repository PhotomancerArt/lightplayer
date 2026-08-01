---
status: carried
since: 2026-08-01
logged: 2026-08-01
area: lp-core/lpc-engine/src/nodes/fixture (brightness → gamma ordering)
related:
  - docs/design/brightness-gamma-dithering.md
  - docs/defects/2026-08-01-gamma-8bit-choke.md
  - lp-core/lpc-engine/src/nodes/fixture/power_limit.rs
---
# Brightness is applied before gamma, starving the 8-bit wire at dim settings

**Shape** — `fixture_node.rs` applies fixture brightness in the *perceptual*
domain, before the γ=2.8 encode. Because `(s·c)^γ = s^γ·c^γ`, the slider value
is effectively raised to the 2.8 power on its way to the wire: brightness
38/255 delivers 0.48 % duty, compressing the entire image into **1.24 of the
wire's 256 codes**. Contrast is unaffected (the scale factors out exactly);
what is lost is output resolution — 30× fewer usable codes at that setting
than a linear-domain brightness would keep. Measured on the classic-ESP32
bench 2026-08-01: gamma-on at brightness 38 lights only pixels above 72 %
content, as visible sparkle (device refresh ~20 fps puts sub-code dithering
below flicker fusion). Full derivation and stage-by-stage domain table:
`docs/design/brightness-gamma-dithering.md`.

**Why it is acceptable now** — the semantics is *coherent*: the slider is
perceptually linear (15 % slider ≈ 15 % perceived), and every project authored
so far was tuned against this behavior. The measured workaround in the wild is
projects shipping `gamma_correction: false` (all classic test projects do),
which sidesteps the starvation by dropping the encode entirely. Changing the
order changes visible output for every gamma-on fixture on every chip, so it
is a deliberate product decision, not a bug fix to slip in.

**Exit criteria** — a decision, then a small change:

1. **Decide the slider semantics.** The recommended direction (matches WLED's
   advised configuration and FastLED's `setBrightness`): brightness becomes a
   linear multiply in the post-gamma u16 domain, composed with the power-limit
   scale that already lives there for the same physical reason. Slider 38
   would then read as ~51 % perceived instead of ~15 %, in exchange for 38
   usable codes instead of 1.24.
2. Apply it in `fixture_node.rs` (move the multiply below `apply_gamma16`,
   compose with `power.channel`), keeping the load-bearing
   `gamma → power scale` order intact.
3. If a perceptual slider *feel* is still wanted, add a UI-side curve
   decoupled from the LED encode — explicitly not γ=2.8 mapped into linear,
   which is numerically identical to today and would recreate the condition.
4. Re-run the classic bench case (`projects/test/quad-gamma-v3`, brightness
   38, gamma on): full-white content should land near code 38, and mid-tones
   should be visible.

**Log**

- 2026-08-01 — condition identified on the DOM-Z-102 bench ("very dim, just a
  few blue ones lit" at brightness 38 with the fresh 16-bit gamma); verified
  as ordering rather than the gamma table by flipping `gamma_correction` on
  identical firmware (4 B heap / ~1 fps delta). WLED/FastLED conventions
  verified from their documentation the same day.
