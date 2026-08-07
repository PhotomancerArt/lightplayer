# Welcome to LightPlayer!

```embed sim-canvas sim=main view=product
```

```embed panel sim=main mode=interactive
```

LightPlayer is a work-in-progress app that helps you build light art at any scale,
from glowing scarves to art cars.

It works by running small graphics programs called
[shaders](#/docs/what-is-a-shader) for each lamp in your art, allowing for
dynamic patterns driven by the environment, or pre-programmed playlists.

LightPlayer runs today on ESP32-based controllers and boards, including the
[QuinLED](https://quinled.info) family and others.

Our goal is to create a friendly experience that gets you glowing in minutes and then
lets you dive as deep as you want to go. Everything is customizable and configurable,
all the way down to
[LightPlayer's own code](https://github.com/PhotomancerArt/lightplayer),
which is fully open-source and free.

You can [get started](/devices) right away using our browser-based simulator,
no hardware required!

## Simple on the surface, deep everywhere

LightPlayer is built like a game engine for interactive light art: projects
are made up of components called **nodes**. Each kind of node serves a
different role in making your art work. They all come with good settings out
of the box, and each is highly customizable.

- The **Clock** node owns the timing of your project. It lets you change the
  speed and scrub through time to find any problems and preview your patterns.
- [**Shader** nodes](#/docs/what-is-a-shader) generate the patterns that are
  played on your lights. Each one is a small program you can open — read it,
  tweak a number, or ask an AI to rework it into something new.
- **Fixture** nodes tell LightPlayer how many and what kind of lights you
  have, and how they are arranged. This lets your lights look like they are
  part of a cohesive whole.
- **Output** nodes represent the actual hardware — which pins on which
  controller are connected to which lights.

## Try it before you build it

Ideas usually show up before the hardware does. LightPlayer runs a
first-class build of its firmware right in your browser, so from any
computer, anywhere, you can play with a pattern and see what it's going to
look like on your art — without needing the hardware, or even having built
it yet. When the board shows up, the same project uploads onto it.

## Where it is today

LightPlayer is in alpha. Today it runs real installations of around 1,500
LEDs on a common ESP32 controller, edited over USB — the scale of a scarf, a
nightstand light, a desk piece, a small dome. That's the scale we're
polishing hardest right now: plugging in a board and getting it glowing
should take a minute, not an evening.

The architecture was designed from the start for much more — host-class
machines, GPUs, installations of 100,000 LEDs — and that's where we're
headed. No promises with dates on them; we'd rather show you working light.

Interactivity is on the same path: buttons, audio, and sensors flowing
through the same graph as your patterns, with tools to record what came in
and play it back until the behavior is right.

## Why we're building it

LightPlayer is open source, and that matters to its mission. The tools
people use to create and express themselves are best when they're open —
when you can see how they work and change them.

The goal is simple: more interactive light art in the world. Wanting your
LEDs to do something should feel fun and friendly — grab a controller, plug
it in, and see your vision glowing in half an hour. And at 2 a.m. at Burning
Man, when some of your LEDs have stopped doing the right thing, figuring out
*why* should feel friendly too.

## Where next

- [What's a shader?](#/docs/what-is-a-shader) — the heart of LightPlayer,
  live: drag real knobs, meet the code behind them, and watch one shader run
  on two different shapes at once.
- [Brightness, gamma, and smooth fades](#/docs/brightness-and-smooth-fades)
  — what the brightness slider actually controls, and why gamma correction
  should stay on.

*Hacking on LightPlayer itself rather than making art with it? Start at
`docs/architecture.md` in the repository.*
