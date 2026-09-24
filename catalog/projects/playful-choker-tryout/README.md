# PLAYFUL Choker Tryout

The PLAYFUL choker's rig (same `fixture.json` at 20 % brightness, same
`output.json` on `D10`, same `playful.map2d.json`) playing five candidate
patterns from the first pattern-library set, so they can be worn and judged.
The original `playful-choker` project is untouched.

## What it cycles

A playlist, 30 s per pattern with a 1.5 s cross-fade:

| entry | pattern | family | what to look for on the word |
|---|---|---|---|
| 1 | Soft Noise (`noise-soft`) | Fields | slow glows drifting along the letters, dark valleys between |
| 2 | Aurora (`aurora`) | Fields | ribbons swaying across the word's height |
| 3 | Twinkle (`twinkle`) | Points | single lamps blooming on a dim blue ground |
| 4 | Scanner (`scanner`) | Fronts | a bar sweeping P→L and back with a fading tail |
| 5 | Fireflies (`fireflies`) | Points | five soft glows wandering over near-black |

Each module under `modules/` is a verbatim copy of that pattern's exported
`effect/` folder (`catalog/patterns/<slug>/effect/`), the way importing a
pattern copies it; the folder names use `_` because node names cannot
contain `-`.

## How to switch

The playlist cannot start its own tour: a LightPlayer playlist never leaves
its **idle entry** (entry 1, Soft Noise) on a timer; only entries after it
advance on their `duration`. So:

- **Start the tour:** in Studio, connected to the choker, open the
  `playlist` node and click the **Aurora** chip in its entry strip. From
  there it advances on its own, 30 s each: Aurora → Twinkle → Scanner →
  Fireflies → back to Soft Noise, where it rests.
- **Jump to one:** click that pattern's chip. A non-idle entry plays 30 s
  and then continues the tour; to hold one, run the tour to Soft Noise, or
  come back and click again.

There is no button on the choker to do this (the playlist's `trigger`
input is left unbound).

## Why five, not more

Every imported pattern module costs heap on the ESP32-C6 as soon as the
project loads, whether or not it is playing. Measured on the emulated C6
(`lp-emu:esp32c6:t1`, direct boot, `lp-emu` at `14ef539d7`; emulated, not a
hardware figure):

| entries | free after load | free after the first shader compiles |
|---|---|---|
| 4 | 156 KB | 103 KB (largest block 64 KB) |
| 5 (this project) | 141 KB | 88 KB (largest block 64 KB) |
| 8 | 77 KB | 24 KB (largest block 10.8 KB): `lp-cli upload`'s post-deploy read is refused |

Only the idle entry was exercised there: each later entry compiles when the
tour reaches it (about 6 KB and 3 KB of code each in the runs above), and a
full tour has **not** been run in emulation, since starting it needs Studio's
activate command.

## What it needs

`"coords": "pattern"` is new (the pattern-space rule). Firmware built before
that rule does not know it and will not render these shaders in pattern
space, so flash a build of the branch this
project ships on, e.g. `just flash-fw-esp32c6 <port>`, then push the project
from Studio. Brightness stays at 0.2, as on the original.
