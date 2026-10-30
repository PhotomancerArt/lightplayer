# The S3's Wi‑Fi goes on by default (FS3 at its usual distance)

2026-10-29. Agent-run; Yona put FS3 back at its usual spot beforehand.

- **Board:** FS3 `fixture-s3`, a Seeed XIAO ESP32-S3 Plus (16 MB, Puya flash
  `85 20 18`, D8:3B:DA:47:29:70), antenna attached, about 1 m from the desk's
  `lp-bench-ap`. Powered through the switchable hub on a long USB cable, with
  five strips of 8 WS281x LEDs on its 5 V pin (D5, D7–D10). Startup project
  "Skirt 620 five" (5 × 124, brightness 0.25).
- **Control:** XS1 `xiao-s3-1`, the plain XIAO ESP32-S3, already walked
  (docs/reports/2026-10-29-plain-xiao-s3-bring-up-walk.md): 10/10 at 20 dBm
  and 10/10 at the 15 dBm cap, idle −4.7 %, LAN busy −9.7 %.
- **Images:** main `3dc8a3527` built as the split image with `wifi` added
  (the 15 dBm cap, #1302, on), written to the app region so the filesystem
  and the saved network stayed. A 20 dBm control (the cap removed, never
  committed) for one test. FS3's known-good before and after: releases
  2026.10.25-1 and 2026.10.29-6.

Every number below is FS3's own console over USB (`lp-cli link capture`),
except where it says `link rtt` (the host's measurement over the LAN).

## Verdicts

| check | result |
|---|---|
| Joins at 1 m, 15 dBm cap, Skirt lit | **10/10**, each on the first attempt; associated in 715–747 ms; address 1.7–7.0 s after boot; `lp-bench-ap` at −27 dBm |
| Joins at 1 m, 15 dBm cap, no project running | 10/10, first attempt, 711–855 ms (the first run, before the project was upgraded; see below) |
| Relay | registered with lightplayer.app by itself (`relay: connected`): FS3 holds an account key |
| Radio frame-rate budget, Skirt 620 five | idle −4.8 %, LAN busy −10.8 % mean, p99 ≤ 75 ms (below) |
| Radio frame-rate budget, 2,048 LEDs | idle −5.7 %, LAN busy ~−20 % mean, p99 ≤ 500 ms (below) |
| Heap, Wi‑Fi + BLE + ESP-NOW + PSRAM, LAN link busy | Skirt 620: ≥ 99,932 B internal free, largest block 73,736 B; 2,048 LEDs: 122,700 B at every heartbeat; no reset, no "memory busy" |
| 20 dBm, LEDs dark vs lit | **0/10 dark, 0/10 lit**: not LED-load supply sag (below) |
| QIO on FS3's Puya part | 20/20 reboots, core hash 20/20, an upload under QIO; Skirt 55.6 → 65.8 fps (#1332) |

## Flash on 8 MB

`just fw-esp32s3-size-check` with `wifi` in the default features: `app.bin`
3,489,792 B of the 6 MiB factory partition; the gate (smallest of steady and
update headroom) 2,805,359 B against its 64 KiB floor. The radio's network
lives in the core, so the core grew: it is now **6,672 B from its next 64 KiB
page** (28.8 KB before, with #1329). Not a gate on this chip; the next ~6 KB of
core growth costs a flash page.

## The project had to be upgraded first

FS3's "Skirt 620 five" was project format 11; main reads format 12, so the
first image refused it (`unsupported project format 11 (expected 12)`) and the
first ten joins ran with no project rendering. The board's filesystem was
saved first (`lp-cli hardware lpfs save`, kept locally), the project upgraded
on the host (`lp-cli project upgrade --apply`, 11 → 12) and written back over
USB, as a firmware update does. "Core1 8x256 (2048)" was upgraded the same way
for the 2,048-LED checks. Both now load on main and on release 2026.10.29-6;
the release 2026.10.25-1 FS3 came with would refuse them.

## Radio frame-rate budget

Same image, Skirt 620 five, means of 5 s windows (`[perf]`), first two windows
dropped:

| state | fps | change | p99 | longest frame |
|---|---:|---:|---:|---:|
| Wi‑Fi off | 53.9 | — | ≤ 25 ms | 26 ms |
| joined, idle, relay connected, 2 min | 51.3 | −4.8 % | ≤ 33 ms | 34 ms |
| `lp-cli link rtt lan:` | 48.1, worst window 38.3 | −10.8 % mean | ≤ 75 ms | 343 ms |

LAN request RTT p50 87 ms, p99 203 ms; board → host 17.5 KiB/s.

The 2,048-LED project ("Core1 8x256 (2048)", 8 × 256), same image:

| state | fps | change | p99 | longest frame |
|---|---:|---:|---:|---:|
| Wi‑Fi off | 19.3 | — | ≤ 75 ms | 63 ms |
| joined, idle | ~18.2 | −5.7 % | ≤ 75 ms | 76 ms |
| `lp-cli link rtt lan:` (seven windows) | ~15.4, worst window 12.5 | ~−20 % | ≤ 500 ms | 323 ms |

All inside the ceilings (idle ≤ 10 % and p99 ≤ 100 ms; editing ≤ 50 % and
p99 ≤ 1 s; docs/adr/2026-10-06-radio-frame-rate-budget.md).

## Heap with every radio on

Wi‑Fi, Bluetooth, ESP-NOW and PSRAM on, joined, relay registered:

- **Skirt 620 five**, a LAN link busy (`link rtt`): the heartbeat's internal
  free never below 99,932 B, largest block 73,736 B (heartbeats every 5 s over
  the run). Main stack high water 30,720 of 51,532 B.
- **2,048 LEDs**, a LAN link busy: `[MEM] free=122,700 used=84,164
  largest_free=73,744` at every heartbeat; PSRAM 245 KB used of 8 MB.
- The radio's own region (49,152 B) runs at its ceiling (high water 49,036 B)
  and spills to the main heap while the LAN is busy (`overflowed to main`
  counts bytes over the run, 397 KB by the end of the Skirt run); the main
  heap's free figure held through it.

The LAN read floor is 40,960 B free and a 16 KiB block; both projects stay well
above it.

## 20 dBm, LEDs dark and lit (the supply-sag question)

FS3 powers its strips from the XIAO's 5 V pin, through the hub and a long
cable, while XS1 is bare on the Mac. If a 20 dBm burst on top of the LED load
sagged the supply, FS3 would fail lit and join dark. Each trial is a reboot
and up to 50 s for an address; "dark" sends `stopAllProjects` right after the
rebooted board's hello, which closes every output with one all-black frame (in
9 of 10 trials that landed before the first join attempt, and every later
attempt in the 50 s ran dark).

| 20 dBm | joins | how the attempts ended |
|---|---:|---|
| LEDs dark (project stopped, strips blanked) | **0/10** | 4–5 attempts each: "disassociated due to inactivity", some "authentication expired"; AP heard at −28…−33 dBm |
| Skirt 620 five lit | **0/10** | the same, one "association failed" |
| 15 dBm cap, lit (above) | 10/10 | — |

So it is not the LED load. FS3's earlier failures agree: in the 2026-10-22
sittings the startup project crashed the old `wifi` image and the recovery rule
skipped it, so most of those 0-for-N attempts ran with nothing driving the
strips.

What this does not rule out is the board's own supply: the radio's 20 dBm
burst through the hub and the long cable, LEDs or not. A short cable straight
into the Mac (XS1's setup) would test that in one sitting, and it needs hands.
The other lead is still FS3's crystal, +13.5 ppm against the Mac where XS1
reads +7.2. A short cable is the cheaper test; a powered hub would not change
the cable's drop.

One product finding: a 20 dBm toggle trial ended in "4-way handshake timeout",
which the station records as a refused password (`last: wrongPassword`) and
then stops trying that network until the next boot. On a marginal link a
timeout is not a refusal.

## Afterwards

FS3 was left on release 2026.10.29-6 with "Skirt 620 five" (format 12) as its
startup project, rendering at 55.8 fps. Its saved network and Wi‑Fi switch are
unchanged (that release has no station; the next one will).
The format-11 filesystem it came with is saved locally
(`~/.lightplayer/backups/fs3-2026-10-29-format11/`).
