# LightPlayer user guide

Articles for LED artists using Studio. Not firmware documentation — if you
are here to hack on LightPlayer itself, start at `docs/architecture.md`
instead.

Each article teaches the *why* behind one part of making LED art look good:
short, concrete, and written for people who think in scenes and fixtures,
not in code. These pages render in-app at `#/docs` (each article needs a
manifest entry in `lp-app/lpa-studio-web/src/app/docs/mod.rs`); richer,
interactive docs are a separate, later initiative.

## Articles

- [Brightness, gamma, and smooth fades](brightness-and-smooth-fades.md) —
  what the brightness slider actually controls, why gamma correction should
  stay on, and why very dim scenes can shimmer on some devices.

## Planned

Topics we intend to write, in no particular order:

- Mapping fixtures in 2D — placing LEDs so shaders land where you expect.
- Buses and bindings — sharing one control across many nodes.
- Picking hardware — what different boards can drive, and how to choose.
