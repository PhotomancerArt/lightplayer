# Brightness, gamma, white point, and dithering — how light actually flows

How a shader sample becomes photons, which numeric domain each stage lives
in, and why the order of operations is the whole ballgame. Written 2026-08-01
after the classic-ESP32 bring-up measured all of it on silicon
(`docs/defects/2026-08-01-gamma-8bit-choke.md`, PR #252, and the bench session
recorded in `docs/debt/brightness-applied-before-gamma.md`).

## The wire's reality

A WS281x pixel accepts **8 bits per channel of duty cycle**, and duty is
(approximately) linear light: code 128 emits half the photons of code 255.
There is no getting more resolution out of the protocol itself; everything
above the wire exists to spend those 256 codes well.

On device, the LED refresh rate is currently **locked to the engine frame
rate** — `Esp32OutputProvider::write` runs once per engine frame and each call
transmits one frame (`lp-fw/fw-esp32-common/src/output/provider.rs`). On the
classic ESP32 that is ~20 fps today. This matters for dithering; see below.

## Two languages, one translator

- **The LED speaks linear light** (duty cycle, photons).
- **The eye speaks ratios** — it compresses, roughly `perceived ≈ light^(1/γ)`.
  Half the photons reads as ~78 % as bright, not 50 %.
- **Gamma is the translator**: `linear = perceptual^γ`, with **γ = 2.8** here
  (inherited from the Adafruit LED convention via the legacy `GAMMA8` table —
  steeper than sRGB's ~2.2; kept for visual continuity when the 16-bit table
  replaced it, see `lp-core/lpc-engine/src/nodes/fixture/gamma.rs`).

The graphics-industry lesson (sRGB) applies verbatim: **do math in linear
space, apply the encode once, at the boundary.** Scales, blends,
interpolation, and power limiting all belong on the linear side; the
perceptual encode is a wire format, not a place to compute.

## The pipeline, stage by stage

| stage | where | domain | notes |
|---|---|---|---|
| shader output | engine / JIT | u16, **perceptual** | what the author sees in Studio previews |
| fixture sampling | `fixture_node.rs` | u16 perceptual | mapping only, no value math |
| **brightness** | `fixture_node.rs` (`apply_brightness_unorm16`) | u16 **perceptual** ⚠️ | applied **before** gamma — see "the ordering question" |
| **gamma** (`gamma_correction: true`) | `gamma.rs` (`apply_gamma16`) | perceptual → **linear** | the encode. 16-bit in/out since PR #252; `[u32; 513]` const table, max error 0.75 counts in 65,535 |
| power limit | `power_limit.rs` | u16 linear | **after gamma, load-bearing**: power ∝ duty ∝ linear, so a scale derived from duty sums must land on linear values |
| color order | `fixture_node.rs` | u16 linear | byte shuffle |
| control product | wire between nodes | u16 linear | `Unorm16` |
| frame interpolation | `DisplayPipeline` | u16 linear | correct domain — blending light levels is a linear-space operation |
| white point (`lut_enabled: true`) | `DisplayPipeline` | u16 linear | Q16.16 multiply per channel (the 3 KB/channel table it replaced computed exactly this — see the defect) |
| **temporal dithering** | `DisplayPipeline` (`dither_step`) | u16 → u8 | error-carry across frames; expresses fractional codes |
| wire | RMT / driver | **u8 duty** | 256 codes, hard stop |

Everything downstream of the gamma step is coherently linear. The one
domain oddity in the pipeline is brightness sitting on the perceptual side.

## The ordering question

Because gamma is a pure power law, moving a constant scale across it is an
exact identity:

```
(s · c)^γ  =  s^γ · c^γ
```

Two consequences, one of which is easy to get wrong:

1. **The image does not change.** The scale factors out, so pixel-to-pixel
   contrast ratios are *identical* whether brightness is applied before or
   after gamma. Ordering is not an artistic choice.
2. **The slider's meaning and the wire's resolution change enormously.**
   - Brightness **before** gamma (today): the slider is *perceptual*.
     Slider `s` asks for `s` of the perceived brightness, which is `s^2.8` of
     the photons — and `s^2.8` of the wire codes.
   - Brightness **after** gamma: the slider is *linear*. Slider `s` gives
     `s` of the photons and `s` of the codes; perceived brightness moves as
     `s^(1/2.8)` (slider 15 % reads as ~51 %).

Measured consequence at the top of the range (`255·s^2.8` vs `255·s` codes):

| slider | codes today (perceptual) | codes if linear |
|---|---|---|
| 255 | 255 | 255 |
| 127 | 32 | 127 |
| 64 | 5.3 | 64 |
| 38 | **1.24** | 38 |

At brightness 38 — the value the classic bring-up's test projects ship —
**the entire image fits in 1¼ wire codes**. Only content above 72 % clears
code 1; below ~19 % nothing reaches the wire at all. On the bench this looks
like "mostly dark, a few of the brightest pixels dimly lit", and it is the
pipeline faithfully executing a request the wire cannot carry. It is also,
almost certainly, why those projects ship `gamma_correction: false`: gamma
was unusable at dim brightness, and the ordering — not gamma itself — was the
cause.

Verified on silicon 2026-08-01: identical firmware, identical project,
`gamma_correction` flipped — 4 bytes of heap and ~1 fps of difference. The
16-bit gamma itself is free; the ordering is what starves the wire.

## What other systems do

- **WLED** ships gamma for *color* and gamma for *brightness* as separate
  toggles ([kno.wled.ge/features/settings](https://kno.wled.ge/features/settings/)):
  color gamma — *"Will correct colors to match those on a monitor. Strongly
  advised to keep on."* Brightness gamma — *"Will correct brightness changes
  to make it appear more linear. Advised to leave off."*
  Our current behavior is equivalent to WLED with brightness-gamma **on** —
  the configuration WLED advises against.
- **FastLED**: `setBrightness()` is a linear whole-animation scale applied at
  show time, with temporal dithering recovering sub-code resolution
  ([FastLED temporal-dithering wiki](https://github.com/FastLED/FastLED/wiki/FastLED-Temporal-Dithering)).
  Their docs warn that at low refresh rates and low brightness *"you may see
  the dithered pixel output as flickering"* — see below.
- **Displays**: OS brightness moves the backlight (linear light); the pixel
  encode is untouched.
- **Stage lighting**: dimmer curves (linear / square / S-curve) are an
  explicit per-fixture configuration, because the semantics genuinely is a
  choice — the sin is making it implicitly, which is where we are today.

## Dithering: what it can and cannot rescue

`dither_step` carries the 16→8 quantization error across frames, so code 0.66
becomes "on 2 frames of every 3". Two hard limits:

1. **It needs refresh well above flicker fusion** (~100 Hz+; FastLED aims for
   hundreds). Our device refresh is the engine frame rate — ~20 fps on the
   classic today — so sub-code dithering there renders as visible *sparkle*,
   not smooth dimness. Higher-fps hosts (sim, S3) fuse much better.
2. **It cannot restore information destroyed upstream.** The 8-bit gamma
   choke (fixed in PR #252) quantized before dithering ever ran; the
   brightness ordering compresses the signal into ~1 code before dithering
   sees it. Dithering is a last-inch tool: it spends fractional codes, it does
   not mint new ones.

## The open decision

Whether to move brightness to the linear side (composing it with the power
scale that already lives there), matching the WLED/FastLED convention, is
tracked — with the measured costs of both options — in
`docs/debt/brightness-applied-before-gamma.md`. Any slider-*feel* curve, if
wanted after that change, belongs in the UI layer, decoupled from the LED
encode: mapping the slider through γ = 2.8 and applying it in linear space is
numerically identical to today's behavior and would recreate the problem.
