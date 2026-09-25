# PLAYFUL Choker Tryout

The PLAYFUL choker's rig (same `fixture.json` at 20 % brightness, same
`output.json` on `D10`, same `playful.map2d.json`) with the whole first
pattern-library set on one playlist, so they can be worn and judged.
The original `playful-choker` project is untouched.

## What is on it

25 entries: all thirteen patterns of the first set, then a second stop for
twelve of them (all but the Linear Gradient).

| entries | pattern | module |
|---|---|---|
| 1, 14 | Soft Noise (`noise-soft`) | `modules/noise_soft/` |
| 2, 15 | Aurora (`aurora`) | `modules/aurora/` |
| 3, 16 | Twinkle (`twinkle`) | `modules/twinkle/` |
| 4, 17 | Scanner (`scanner`) | `modules/scanner/` |
| 5, 18 | Fireflies (`fireflies`) | `modules/fireflies/` |
| 6, 19 | Colour Wipe (`color-wipe`) | `modules/color_wipe/` |
| 7, 20 | Heartbeat (`heartbeat`) | `modules/heartbeat/` |
| 8, 21 | Hard Noise (`noise-hard`) | `modules/noise_hard/` |
| 9, 22 | Ripples (`ripples`) | `modules/ripples/` |
| 10, 23 | Spiral (`spiral`) | `modules/spiral/` |
| 11, 24 | Veins (`veins`) | `modules/veins/` |
| 12, 25 | Radial Gradient (`radial-gradient`) | `modules/radial_gradient/` |
| 13 | Linear Gradient (`linear-gradient`) | `modules/linear_gradient/` |

Each module under `modules/` is a verbatim copy of that pattern's exported
`effect/` folder (`catalog/patterns/<slug>/effect/`), the way importing a
pattern copies it; the folder names use `_` because node names cannot
contain `-`.

**The second stop shares the first stop's module.** Entry 14
(`noise_soft_2`) points at the same `./modules/noise_soft/module.json` as
entry 1, and so on. Only one entry is loaded at a time, so the two never
meet on the device, and a shared file means one copy to edit. A playlist
entry is only a path, so the two stops start from the same knob defaults;
each entry remembers its own knob values once you change them (the
remembered values are per entry, not per module), so the second stop is the
place to keep a second setting of a pattern you like.

## The tour

It tours by default: `"tour": { "kind": "cycle", "step_seconds": 30,
"fade_seconds": 1.5 }` on the playlist. Every entry plays 30 s and hands
over with a 1.5 s fade, in entry order, and wraps. The tour follows the
clock, so the clock's speed and pause move it too.

A switch holds the last frame the choker showed, unloads the pattern that
was playing, loads and compiles the next one, then fades from the held frame
into it. The lamps never go dark.

## How to pick

In Studio, connected to the choker, open **Play**. The **Pattern** row lists
the 25 names:

- **tap a name** to play it now; while touring, the tour carries on from
  there;
- **the tour switch** turns the tour off (hold the current pattern) or back
  on, and its step sets the seconds per pattern;
- **the per-pattern switch** takes a pattern out of the tour (it can still
  be tapped);
- **next / previous** step through the list.

These are remembered on the choker, like the other knobs. The authored
defaults in `playlist.json` (tour on, 30 s, nothing skipped) are what a
fresh upload starts from.

There is no button on the choker (the playlist's `trigger` input is left
unbound).

## Why 25 fits now

Before, every entry's pattern was loaded as soon as the project loaded,
whether it was playing or not: about 17.6 KB of heap per entry, so eight
entries left too little room for the first shader to compile.

Now only the playing entry is loaded. An entry that is not playing costs
about **343 B** of heap: its name, path and settings in the playlist.
Measured on fw-emu (`lp-cli profile --collect alloc --mode startup`), 5
versus 25 entries: the project load retains 44,539 B and 51,399 B.

On the emulated C6 (`lp-emu:esp32c6:t1`, direct boot; emulated, not a
hardware figure), this 25-entry project uploads and passes `lp-cli upload`'s
post-deploy check, and leaves:

| when | free heap | largest free block |
|---|---|---|
| after the first shader compiles | 105,456 B | 76,980 B |
| after 7 tour switches (lowest seen) | 91,284 B | 56,750 B |
| after 7 tour switches (at the end) | 98,400 B | 66,304 B |

The full method, and a heap trace across two full tours, are in
`docs/reports/2026-09-25-dormant-playlist-entries-proof.md`.

## What it needs

Firmware with dormant playlist entries and touring (the
multi-pattern-projects work) and the pattern-space rule (`"coords":
"pattern"`). Older firmware loads every entry at once and will not fit 25.
Flash a current build, e.g. `just flash-fw-esp32c6 <port>`, then push the
project from Studio. Brightness stays at 0.2, as on the original.
