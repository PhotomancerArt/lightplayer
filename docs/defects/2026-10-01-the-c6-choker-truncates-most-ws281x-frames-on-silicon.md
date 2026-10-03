---
status: open
found: 2026-10-01      # how: hardware-walk (the C6 link-thread plan's silicon ws281x_telemetry check)
area: lp-fw/fw-esp32c6 output/rmt (lp-ws281x refill on the C6), lp-emu-esp32c6 RMT model
class: fidelity
related:
  - lp2025/2026-10-01-1756-c6-link-io-thread
  - docs/debt/c6-on-legacy-ws281x-driver.md
  - docs/adr/2026-08-05-ws281x-transmission-on-app-core.md
---
# The C6 truncates most of the PLAYFUL choker's WS281x frames on silicon, and the emulator decodes every one of them whole

**Symptom** — the shipped C6 image built with `ws281x_telemetry`
(`--features ws281x_telemetry` on the defaults), on the desk XIAO C6
(`10:bd:a3:b0:8e:30`) rendering `PLAYFUL Choker (lab rehearsal)` (73 LEDs on
D10, the manifest's two channels: `ch0_blocks=1 ch0_window_words=48
ch0_half_words=24`), under `lp-cli link rtt` load, 2026-10-01:

```text
main 0380c63f8 + telemetry, at 90 s:
[WS281X] t_ms=90110 ch=0 half=24 frames=2795 complete=588 trips=2207 skips=0 errors=0 refills=103666 wanted=204035 lag_avg=5.3 lag_max=7 over_half=0 … entry_max=25 … trip_at=480
branch 636b2c56d's firmware + telemetry, at 80 s:
[WS281X] t_ms=80062 ch=0 half=24 frames=2453 complete=636 trips=1817 skips=2 errors=0 refills=129494 wanted=179069 lag_avg=5.3 lag_max=7 over_half=0 … entry_max=39 … trip_at=1008
```

79 % (main) and 74 % (branch) of frames end on a guard trip — the refill
for a half arrived too late and the transmitter stopped early — and
`refills` is 51 % / 72 % of `wanted`. `trip_at` wanders (288–1,392 bits),
so it is not one deterministic late service. The same project on
`lp-emu:esp32c6:t2` (`lp-cli link rtt emu:…`, emulator code at
`6134f415f`) decodes 4,253 frames on pad 18 with **0 errors and 0
incomplete**, main and branch alike, with refill `entry_max` 20 words of a
24-word half and nothing unanswered.

The C6's own desk smoke at the `lp-ws281x` swap (2026-08-01,
`docs/debt/c6-on-legacy-ws281x-driver.md`) read `trips=0 skips=0 errors=0`
and `refills == wanted` on the 3-strip jig, so this is not how the driver
has always read on silicon.

**Root cause** — not established. The two readings disagree on the same
image and project, so at least one of them is wrong: either the silicon
refill service misses its half-window deadline far more often than the
emulator's model allows (interrupt entry and refill cost on silicon versus
the model), or the telemetry's trip detection misreads this configuration
(two declared channels, one block each). What is established: it is
present on `main`, so the C6 link thread did not introduce it; the link
thread raised the worst refill entry delay from 25 to 39 words (one run
each), with fewer trips overall.

**Fix** — none yet. First step: read the strip (or a logic analyser on D10)
to learn which reading is true, then bisect `trips` on silicon from the
2026-08-01 swap forward.

**Regression coverage** — none: the emulator, the gate that should catch
it, reports every frame whole.

**Lesson** — the emulator walk's RMT check (0 errors, 0 incomplete) is not
evidence about refill deadlines on silicon until this disagreement is
explained; a `ws281x_telemetry` run on the board is the cheap check, and
it should be read for `trips` and `refills/wanted`, not just `errors`.
