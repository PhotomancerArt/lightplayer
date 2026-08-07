# Welcome to LightPlayer!

```embed sim-canvas sim=main view=product
```

```embed panel sim=main mode=interactive
```

LightPlayer is a work-in-progress app that helps you build light art at any scale,
from glowing scarves to art cars.

It works by running small graphics programs called [shaders] for each lamp in your
art, allowing for dynamic patterns driven by the environment, or pre-programmed
playlists.

LightPlayer runs today on ESP32-based controllers and boards including the [QuinLED] boards
and others.

Our goal is to create a user friendly envion


You have LEDs and a controller. Plug it into your phone or your
computer, and in a minute or two you have glowing light — a library of
shaders you can play with and control right from your browser. No Wi-Fi
setup, no pairing, nothing between you and the light: plug it in,
upload, play.

That first minute is the surface. Underneath it, LightPlayer is an
engine — like a game engine, but for interactive light art. A game
engine doesn't tell you what game to make; it gives you a place where
your ideas can live, at whatever depth you want to work. LightPlayer
wants to be that for light: an instrument in play, built to magnify
*your* artistic vision, not prescribe one.

## Simple on the surface, deep everywhere

Every piece you meet in that first minute opens up if you want it to.

- **The shader** making the picture is a friendly little program you
  can read, edit, or hand to an AI to rework — "make it feel like
  flying through clouds" is a fine place to start.
  [What's a shader?](#/docs/what-is-a-shader) shows you, live, right
  in the article.
- **The mapping** knows where your LEDs actually sit. A strip wrapped
  around an object only *looks* like a strip until you tell the
  software where each LED landed — then the light belongs to the
  surface, not the string, and the whole piece reads as one thing.
  You draw the shape, the picture follows, live.
- **The wiring** between the pieces — clock to shader, shader to
  fixture, fixture to output — works by default and rewires when your
  project outgrows the defaults.

Go as deep as you want, or don't. Make ten simple pieces in an hour,
or one deep one. Same software.

## It starts in the simulator

Ideas don't wait for hardware. LightPlayer was built simulator-first:
a first-class build of the firmware runs in your browser, so you can
try a shader, sketch a mapping, and watch what your idea would look
like — on hardware you haven't built yet, from any computer, anywhere.
When the hardware shows up, the same project uploads to the board.

## Where it is today

LightPlayer is in alpha, and we'd rather be straight with you than
impressive. Today it's happiest at the scale of a scarf, a nightstand
light, a bar, a desk piece, a small dome — real installations around
1,500 LEDs on a common microcontroller, edited over USB. That's the
scale we're polishing hardest right now, because getting your board
from box to glowing should be the smoothest minute in the hobby.

The architecture reaches further than the polish does: the engine was
designed from the start for host-class machines, GPUs, and
installations in the tens of thousands of LEDs, and it grows in that
direction as real art pulls it there. Interactivity is headed the same
way — buttons, audio, sensors flowing through the same graph as the
light, with tools to record what came in and replay it until the
behavior is right.

## Why we're building it

LightPlayer is open source, and that's load-bearing. The tools people
use to make art and express themselves are best when they're open —
when you can see how they work, argue about them, and change them.

The mission is simple: more interactive light art in the world. The
moment an idea for glowing light hits you, making it real should feel
fun and friendly — grab a controller, plug it in, and see your vision
running in half an hour. And at 2 a.m. at Burning Man, when some of
your LEDs have stopped doing the right thing, finding out *why* should
feel friendly too.

## Where next

- [What's a shader?](#/docs/what-is-a-shader) — the heart of
  LightPlayer, live: drag real knobs, meet the code behind them, and
  watch one shader run on two different shapes at once.
- [Brightness, gamma, and smooth fades](#/docs/brightness-and-smooth-fades)
  — what the brightness slider actually controls, and why gamma
  correction should stay on.

*Hacking on LightPlayer itself rather than making art with it? Start
at `docs/architecture.md` in the repository.*
