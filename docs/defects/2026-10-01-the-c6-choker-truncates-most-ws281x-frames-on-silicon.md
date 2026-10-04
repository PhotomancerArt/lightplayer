---
status: open
found: 2026-10-01      # how: hardware-walk (the C6 link-thread plan's silicon ws281x_telemetry check)
area: lp-fw/fw-esp32c6 output/rmt (lp-ws281x refill on the C6), lp-emu-esp32c6 RMT model
class: fidelity
related:
  - lp2025/2026-10-01-1756-c6-link-io-thread
  - docs/debt/c6-on-legacy-ws281x-driver.md
  - docs/adr/2026-08-05-ws281x-transmission-on-app-core.md
  - docs/defects/2026-09-02-c6-ws281x-first-three-leds-then-stale.md
  - docs/defects/2026-10-01-the-emulated-c6-charges-a-cold-code-path-10x-less-than-silicon.md
  - third_party/esp-hal/README-LP.md
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

**Root cause** — partly established (2026-10-03); the desk run that
confirms it is still owed, so this stays open. The silicon reading is the
true one, and the emulator's whole-frame reading was a `t2` artefact: at
`t3`, the grade that charges flash-cache fills
(`docs/defects/2026-10-01-the-emulated-c6-charges-a-cold-code-path-10x-less-than-silicon.md`),
the emulator truncates frames too. The cause it points at is the one the
2026-09-02 entry carried as its residual
(`2026-09-02-c6-ws281x-first-three-leds-then-stale.md`, "moving esp-hal's
dispatch into RAM"): the refill's own code is in RAM (`rmt_isr` at
`0x40800938`, `fill_half` at `0x40800644`, no flash call on either but cold
panic paths), but esp-hal's RISC-V dispatcher, entered from the RAM
`handle_interrupts`, called into flash on every interrupt:

```text
ws281x_telemetry image, base 113493d0b (rust-nm -C -n, rust-objdump -d)
4080105c handle_interrupts                                   RAM
  408010a6 jalr <esp_hal::interrupt::riscv::change_current_runlevel>        → 0x42098462 FLASH
  408010c2 jalr <esp_hal::interrupt::riscv::rt::handle_interrupts::{closure#0}> → 0x42098a04 FLASH
  4080112c jalr <esp_hal::interrupt::riscv::change_current_runlevel>        → 0x42098462 FLASH
42098a04 handle_interrupts::{closure#0}                      FLASH
  42098a0a jalr t0 <OUTLINED_FUNCTION_119>                   → 0x4223461a FLASH
  42098a86 jalr a0                                           → rmt_isr 0x40800938 RAM
  42098a8e jalr t0 <OUTLINED_FUNCTION_115>                   → 0x422345b4 FLASH
```

That is about a dozen 32-byte flash-cache lines — ≈25 µs at the 338-cycle
fill `t3` charges, ≈20 of the 24 RMT words a refill has — paid whenever the
render has evicted them. Inferred, not measured on silicon: that is the
first refill after a render, and `trip_at` wandering is that refill landing
at different points of a frame.

**Fix** — esp-hal's fourth fork diff
(`third_party/esp-hal/README-LP.md`): `change_current_runlevel` is
`#[ram]`, and the per-source closure is a named `#[ram] fn dispatch`.
After it, nothing between `Trap15` and `rmt_isr` is in flash except panic
paths:

```text
the same image, the fix applied
408010be handle_interrupts                                   RAM
  4080110a jalr <esp_hal::interrupt::riscv::change_current_runlevel>  → 0x40800fd6 RAM
  40801120 jalr <esp_hal::interrupt::riscv::rt::dispatch>             → 0x408011e0 RAM  (nested path)
  4080113e jalr <esp_hal::interrupt::riscv::rt::dispatch>             → 0x408011e0 RAM  (Priority::max path)
408011e0 dispatch                                            RAM
  40801226 jalr <InterruptStatusIterator::next>              → 0x40801038 RAM
  4080123a jalr <esp_hal::interrupt::mapped_to_raw>          → 0x40800d22 RAM
  40801262 jalr a0                                           → rmt_isr 0x40800938 RAM
```

`.rwtext` +272 B, `.text` −266 B; `just fw-esp32c6-size-check` headroom
175,184 B.

**The `t3` evidence** — `lp-cli emu run --elf <ws281x_telemetry ELF>
--host-link --upload catalog/projects/playful-choker --time-grade t3
--timeout 32s`, `configuration=lp-emu:esp32c6:t3`, lp-emu `72de0d294`, the
ELFs built from base `113493d0b` without and with the fix; the third
`[WS281X]` line of each run, and the frames the emulator decoded off pad 18
(`--dump-frames`):

```text
before: [WS281X] t_ms=30085 ch=0 half=24 frames=1056 complete=1028 trips=28 skips=29 errors=0 refills=75295 wanted=77088 lag_avg=4.9 lag_max=6 over_half=0 hist=1028:72200:2067:0:0:0:0:0:0 entry_max=34 entry_hist=74059:105:86:5:13:574:125:72:256 trip_at=72
after:  [WS281X] t_ms=30082 ch=0 half=24 frames=1067 complete=1059 trips=8 skips=0 errors=0 refills=77618 wanted=77891 lag_avg=4.9 lag_max=6 over_half=0 hist=1059:76245:314:0:0:0:0:0:0 entry_max=19 entry_hist=77079:187:98:57:59:80:58:0:0 trip_at=648
```

| `t3`, 32 s | before | after |
|---|---:|---:|
| trips / frames | 28 / 1,056 (2.7 %) | 8 / 1,067 (0.75 %) |
| skips | 29 | 0 |
| refills / wanted | 97.7 % | 99.6 % |
| `entry_max` (words of 24) | 34 | 19 |
| `entry_hist` buckets 6 / 7 / 8 (≥18, ≥21, ≥24 words late) | 125 / 72 / 256 | 58 / 0 / 0 |
| decoded frames of exactly 72 bits (3 LEDs) | 20 | 0 |
| decoded frames short of 1,752 bits (73 LEDs), excluding empty | 28 | 8 |

A second `t3` run of each image read the same (before: trips 28,
`entry_max` 34; after: trips 8, `entry_max` 19). An earlier ticket run on
main `e51c5cbc3` read trips 56/1,084, `entry_max` 38, 32 frames of 72 bits.

**What is left.** Eight trips in 30 s at `t3`, at scattered bit positions
(240–1,632), with the worst entry 19 words late — not the dispatcher's
signature (a quantised 72-bit frame from a late first refill). The
remaining entry delay is whatever runs with interrupts masked or at a
higher priority when the refill is due; it is not named here.

**`t3` is the gate's grade for this class.** `t2` decodes every frame
whole on the before image too; the next walk that checks the RMT should
read `trips` at `t3`.

**Desk check owed (Yona)** — the fix image on the XIAO C6 rendering the
Choker under `lp-cli link rtt`, 90 s of `ws281x_telemetry`: pass is
`trips` ≈ 0 and `refills` ≈ `wanted`. This entry closes on that run.

**Regression coverage** — none committed. The emulator at `t3` now
catches it (above); at `t1`/`t2`, the grades the walks pin, it still
reports every frame whole.

**Lesson** — the emulator walk's RMT check (0 errors, 0 incomplete) is not
evidence about refill deadlines on silicon until this disagreement is
explained; a `ws281x_telemetry` run on the board is the cheap check, and
it should be read for `trips` and `refills/wanted`, not just `errors`.
