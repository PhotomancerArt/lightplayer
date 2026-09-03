---
status: fixed
fixed: this change     # bench-verified 2026-09-02, see Fix
found: 2026-09-02      # how: report (Yona, 8-LED strip on a XIAO C6 running an example)
area: lp-fw/fw-esp32c6 output/rmt + lp-fw/lp-ws281x (refill path placement)
class: deadline-margin-by-accident
related:
  - docs/debt/c6-scan-truncation-accepted.md
  - docs/adr/2026-08-04-rmt-isr-on-app-core.md
  - docs/defects/2026-08-31-c6-rmt-ws281x-dark.md
  - plan dir 2026-08-01-1459-rmt-priority-hli (p4-logs, archived in Planning)
---
# ESP32-C6: the first 3 LEDs update every frame, LEDs 4+ only every few seconds

**Symptom** — XIAO ESP32-C6 on the shipped 2-channel manifest (one 48-word
RMT block per TX channel, 24-word ping-pong halves), 8-LED WS2812 strip,
an example project rendering at ~26 fps: LEDs 1-3 follow the animation,
LEDs 4-8 hold stale colours and refresh only once every few seconds. No
error in the log; the frame path reports every frame complete.

**Root cause** — refill latency, not logic. 3 LEDs = 72 bits = the
48-word prefill (2 LEDs) plus exactly one serviced refill (LED 3). The
first threshold refill fills the first half and plants the guard STOP at
word 24; the *second* (wrap) refill is then serviced more than 24 words
(~30 µs) late, the transmitter re-reads word 24 and stops there. WS2812
latches on the idle line, so LEDs 1-3 update and LEDs 4-8 keep their
registers. A frame gets through whole only when that refill happens to
be on time.

Why the second refill is late, deterministically: the interrupt service
path ran from **flash**. On the baseline image (llvm-nm, `opt-level =
"z"`) only the `rmt_isr` trampoline was in RAM. `fill_half`, its per-word
store closure, `write_ram` → `ram_word` → two `SharedBlockPlan` lookups
*per word*, `ColorOrder::source_index`, `read_pos` and `set_tx_threshold`
sat in four distant flash regions — a cold cache-miss chain on the first
refills of every frame, after the render loop has evicted them. The
2026-08-01 P4 matrix already showed this shape: with a simple project at
idle, one refill per frame landed at 21-23 words of entry delay against
the 24-word deadline (`entry_hist` bucket 7, `entry_max=21`). The meteor
example's compute load and a larger image moved that refill across the
line. Nothing else changed: esp-hal 1.1.1, esp-rtos 0.3.0, esp-radio
0.18.0 and the C6 register sequence are all identical to August.

**What it was not** — the classic ESP32 backend documents the same
symptom (`guard_trips == frames` on every frame that outgrows the window)
from a *different* cause: its `tx_lim` is a repeating entry count and the
core's alternating threshold had to be clamped. That clamp must **not**
be ported to the C6: the archived P4 logs (cell C, 60-LED frames on
24-word halves, `refills == wanted` = 60 per frame, 1 trip in 5,520
frames) prove the C6's `tx_lim` is a window position, with the same
`c6_rmt.rs` sequence that ships today.

**Fix** — put the whole service path in IRAM and make it cheaper:
- `fw-esp32c6` enables `lp-ws281x/isr-in-ram` (+~1 KB `.rwtext`; main
  stack 72,768 → 71,704 B against a measured 36,936 B high-water).
- `C6Rmt::ram_window` (one window derivation per fill, mirroring the
  classic backend) and `#[inline(always)]` on the backend's ISR-path ops.
- In the core, `#[inline(always)]` on the plan lookups and
  `source_index`, and the per-word store closure replaced by an
  always-inlined `put_word` — a closure is its own codegen item and does
  not inherit the caller's `link_section`, so even under `isr-in-ram` it
  was outlined into flash, one call per word. After the change
  `fill_half` and `rmt_isr` are the only ISR-path symbols and both sit in
  `.rwtext`.
- `[WS281X]` telemetry gains `trip_at=` (bit cursor at the last guard
  trip): a value stuck on 72 frame after frame is this defect's
  signature and separates a deterministically late refill from load.

Bench A/B, 2026-09-02, XIAO C6 `A0:F2:62:87:B4:8C`, storage project
"studio" (meteor, 231 LEDs = 693 bytes on `ws281x0`/D10, 2-channel plan,
24-word halves), 60 s of `[WS281X]` telemetry each, same board and load:

| | baseline (main 9feb434f4) | this change |
|---|---|---|
| frames / complete | 1,414 / 4 | 1,197 / 1,194 |
| guard trips | 1,410 (99.7 %) | 3 (0.25 %) |
| guard skips | 502 | 0 |
| refill work, avg / max (words of a 24-word half) | 15.6 / 22 | 5.7 / 7 |
| entry delay max (words) | 25 | 21 |
| services with entry delay ≥ 24 words | 696 | 0 |

The refill *work* was the larger half of the budget: 15.6 words (~20 µs)
to write a 24-word half from flash with two plan lookups per word, versus
5.7 words from IRAM through the hoisted window. The three residual trips
landed mid-frame (`trip_at=3000`, `864`), the random-load class the
scan-truncation debt entry already carries, at ~0.25 % under meteor.

**Residual exposure (carried, not fixed here)** — one service per frame
still enters at 18-23 words (`entry_hist` buckets 6-7 ≈ frame count,
`entry_max=21`): that is the first interrupt after a render, and its
cost is now outside this crate's code — esp-hal's trap/dispatch path and
whatever the render loop leaves masked. With the refill at 7 words that
leaves a 3-word margin on the first refill of every frame. The levers
left are the block plan (a 192-word window whenever only one output is
opened — 96-word halves, 120 µs deadlines) or moving esp-hal's dispatch
into RAM; both are decisions, tracked in
`docs/debt/c6-scan-truncation-accepted.md`'s reopen path.

**Regression coverage** — none on silicon: no S3/C6 hardware test
transmits an untruncated frame longer than the prefill. The S3 loopback's
truncation test expects 72 bits under both threshold semantics and its
routine frames are 1-2 LEDs; the C6 harness `LedChannel` publishes the
1-channel plan (192-word window), so a ≤8-LED harness strip never
refills. A 2-channel-plan, >2-LED capture (or a `trip_at`/`trips`
assertion under the app path) is the missing test.

**Lesson** — a refill deadline measured in tens of microseconds cannot
have its service path in flash on a single-core chip whose render loop
owns the cache, and `#[inline]` plus `link_section` is not enough to keep
it out: at `opt-level = "z"` the hint is ignored and closures escape the
section. Check placement with `llvm-nm`, not by reading attributes. And
when a symptom is quantised (exactly 3 LEDs, every frame), suspect a
systematic latency at one specific refill before suspecting the
hardware's semantics — the archive already held the measurement that
settled both.
