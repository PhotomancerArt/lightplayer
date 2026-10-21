# Drawing on the emulated C6: fluid-draw with scripted strokes

2026-10-20. Plan `lp2025/2026-10-20-0019-drawing-on-the-picture`, phase P4.
No board was flashed or used: every number here is emulated.

**The drawing pattern runs on a C6 at 26–27 fps while one or two fingers
draw, 30 fps idle, with no heap growth and no link errors.** Getting there
took three fixes the measurement found, all in this PR (below).

## How it was run

- **Image:** CI's own split image for this PR (`ESP32C6_SERVER_RADIO_SPLIT/merged.bin`,
  run 39552916324, built from `0475217b7`; `just fetch-ci-images` confirmed its
  firmware sources match the checkout). ROM-up through our own second stage.
- **Configuration:** `lp-emu:esp32c6:t2`, `net=lan` engaged softly by default,
  no performance seams; `lp-emu` at `0475217b7`.
- **Host:** `lp-cli` (release) from the same commit, hosting the USB link in
  process. The stroke driver is new in this PR: `emu run --strokes line|two`
  writes touch sets the way Studio does, as momentary map panel writes on
  `touches`, at most 30 a second, one in flight, the newest sample winning,
  ttl 400 ms, in emulated time, ending with an empty set.
- **Runs**, 40 s each, `--upload` then (for the strokes) 5 s, then 20 s of drawing:

  ```
  lp-cli emu run --merged <merged.bin> --host-link --upload catalog/patterns/fluid-draw \
    --time-grade t2 --timeout 40s [--strokes line|two --strokes-after 5s --strokes-for 20s] \
    --console … --dump-frames …
  ```

  `idle` (no strokes), `line` (one finger sweeping left and right across the
  middle, 2 s each way), `two` (two fingers circling). `projects/test/fluid`
  ran the same way as the baseline.

## Results

Frame rates are the firmware's own 5 s `[perf]` windows (the heartbeat's
`fps` is a running average since boot).

| | idle | line (1 finger) | two (2 fingers) | `projects/test/fluid` |
|---|---:|---:|---:|---:|
| fps, steady, while drawing | — | **26** | **26** | — |
| fps, steady, not drawing | **30** | 27 (after) | 26 (after) | **39** |
| frame time p50 / p99, drawing | — | ≤ 50 / ≤ 50 ms | ≤ 50 / ≤ 50 ms | — |
| frame time p50 / p99, idle | ≤ 33 / ≤ 50 ms | | | ≤ 25 / ≤ 33 ms |
| worst frame, steady | 41 ms | 93 ms (first stroke window) | 89 ms (first stroke window) | 33 ms |
| touch writes sent / accepted / collapsed | | 531 / 531 / 77 | 527 / 527 / 81 | |
| heap used, idle → drawing → after | 284,544 → 284,540 | → 289,260 → 285,064 | → 298,344 → 285,064 | 237,468 → 237,464 |
| largest free block, lowest | 52,528 B | 45,664 B | **32,888 B** | 65,528 B |
| link resends / rx no-room / errors | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| lit lamps (> 30 of 765), mid-run | ~215 | **~45** (drawing) | ~110 (drawing) | ~165 |

What it says:

- **It draws where the finger is.** While one finger sweeps the middle line,
  about 45 of the 241 lamps hold dye, where the idle friends light about
  215. The friends fade out on the first touch and are back two seconds
  after the last one. The two-finger run lights about 110.
- **Drawing costs about 4 fps against idle.** Some of that is the board
  answering a touch write in most frames (`responses=1`; ~1 ms each now),
  and some is the extra emitters stamped and the dye they leave.
- **It costs about 9 fps against `projects/test/fluid`.** That baseline has
  one compute shader in pixels mode; `fluid-draw` has two, in pattern space,
  and its draw shader is bigger.
- **Heap.** About 4.7 KB is held while one finger draws and 13.8 KB with
  two (the touch map, its materialized slots, and the stroke emitters). It
  is released when the stroke ends. About 520 B stays after the first
  stroke and is flat from then on: retained capacity, not growth.
- **The write rate.** The driver keeps one write in flight, as Studio does,
  so a 30 Hz stroke landed at about 26 writes a second, and 13 % of the
  samples were collapsed into a newer one. Every write was accepted.

**Against the plan's bars** (fps ≥ 25 while drawing, largest free block
≥ 32 KB, no heap growth): inside all three, **narrowly on two**. Drawing
holds 26 fps, one over the bar, and the two-finger run's largest free block
dips to 32,888 B, 120 B over. A desk sitting should look at both first.

## What the measurement found, fixed in this PR

1. **Every touch write rebuilt the whole binding graph** (~40 ms on the C6),
   so drawing ran at **13 fps**. Fixed: a re-write of an engaged panel writer
   reads its value from the store and changes no structure. Knob turns gain
   the same.
   (`docs/defects/2026-10-20-every-panel-write-rebuilt-the-whole-graph.md`)
2. **A fluid without the new `coords` key hung the C6** in its first frame
   (`projects/test/fluid` never lit; LP_WDT reset). Worked around by reading
   the absent key the way the fixture reads its absent options; why the
   view's `get_or` hangs on rv32 and not on the host is open.
   (`docs/defects/2026-10-20-an-absent-option-read-through-get-or-hung-the-emulated-c6.md`)
3. **The draw shader did not fit a board.** Its definition read back as one
   19.7 KB event, over a read frame's 16 KiB
   (`docs/defects/2026-10-20-a-node-def-over-a-read-frame-cannot-be-read.md`),
   and compiling it peaked at 140 KB against the 80.9 KB compute ceiling.
   It is now two shaders, `draw` (76.6 KB) and `friends` (49.5 KB), with
   the brush and idle time as constants.

One tuning change came from the numbers: **`step_hz` 25 → 20.** At 25 Hz a
frame of 40 ms or more stepped the solver every frame, which kept every
frame at 40 ms: drawing locked at 23–24 fps and stayed there after the
finger lifted. At 20 Hz it holds 26–27, and the solver cannot step faster
than about 20 Hz at these frame rates anyway (the planning grounding run's
note).

## Trust

`lp-emu:esp32c6:t2` grades time `modeled` (`validate.toml`): fps and frame
times are the emulator's model, not a desk measurement, and gate nothing.
Memory figures transfer to silicon. The radio frame-rate budget is not
exercised: the link here is USB, and there is no Wi-Fi traffic and no
Studio session. A desk sitting with a board drawn on from Studio is the
next evidence.
