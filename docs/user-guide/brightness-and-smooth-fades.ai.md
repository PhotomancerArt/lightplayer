# Brightness, gamma, and smooth fades

Three controls decide how your content becomes light: the fixture's
brightness slider, its gamma correction toggle, and (on the output) temporal
dithering. They work together, and knowing what each one actually does makes
the difference between "dim and smooth" and "dark with random sparkles".

## What the brightness slider does

The brightness slider scales the *light* — the actual photons — in direct
proportion. Half the slider means half the light output.

Here is the catch: your eye does not read light proportionally. Eyes compress,
so half the photons looks like roughly 78% as bright, not 50%. That is not a
bug — it is the same reason a candle looks bright in a dark room. It means
the slider feels "top-heavy": most of the visible dimming happens in the
lower part of its travel. Slider at 15% still reads as about half brightness
to your eye.

Why do it this way? Because scaling the light directly preserves everything
else. The colors keep their relationships, ramps stay even, and — crucially —
the LEDs keep enough resolution to draw your content. LED pixels only have
256 output levels per channel. A proportional brightness scale at 15% still
leaves you 38 of those levels to paint with. A scale that tried to feel
perceptually linear instead would leave you with one.

## Why gamma correction should stay on

Shaders and Studio previews describe color the way screens and eyes do.
LEDs, though, emit light in direct proportion to the value they are sent —
which is *not* how your eye works. Send a smooth 0-to-255 ramp straight to an
LED strip and it looks wrong: it leaps out of black, then spends the entire
top half of the ramp looking nearly the same.

Gamma correction is the translation between those two worlds. With it on, a
fade that looks smooth in the Studio preview also looks smooth on the actual
LEDs: even steps, no sudden jump out of black, highlights that still have
headroom. Leave it on. Turning it off makes mid-tones look washed-out and
fades look lopsided — the only real reason to turn it off is matching content
that was already hand-tuned with it off.

## Why very dim scenes can shimmer

At low brightness there are only a handful of output levels left, so the
device uses a trick called temporal dithering: to show a level between two
steps, it flickers a pixel between them faster than your eye can see, and
the average lands in between. On a fast device this is invisible and buys
back beautifully smooth dim gradients.

But "faster than your eye can see" depends on the device's frame rate. On a
slower device the flicker drops below the speed your eye blends away, and
what should read as steady dimness shows up as shimmer or sparkle in the
darkest parts of a scene. If you see it: raise the brightness slightly,
simplify the scene so the device runs faster, or turn dithering off on that
output and accept slightly steppier dim tones instead.

## Practical guidance

- **Dim with the brightness control, not by darkening your content.** The
  brightness scale is applied at the very end, where it keeps the most
  output resolution. Darkening the content itself throws that resolution
  away before it reaches the LEDs.
- **Keep gamma correction on** for anything new. It is what makes ramps and
  fades on the LEDs match what you designed.
- **Expect "dim but smooth", and treat sparkle as a signal.** A dim scene
  should look like a quieter version of the bright one. If the dark parts
  shimmer, you are seeing dithering below the device's blending speed — nudge
  brightness up or simplify the scene rather than fighting the content.
