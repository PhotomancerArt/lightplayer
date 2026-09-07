# RTOS tick probe — what the scheduler's PRO-core interrupts cost

The measurement behind `docs/debt/classic-iram-handlers-reach-flash.md`'s
esp-rtos rows, kept as patches rather than as code because the answer was
"not worth carrying a fork": esp-rtos's `timer_tick_handler` and
`cross_core_yield_handler` read `CCOUNT` on entry and exit, the firmware
prints the counters every 10 s, and `summarize.py` turns the lines into
microseconds and a PRO-core percentage.

## Reproduce

1. Vendor esp-rtos 0.3.0 (the version `Cargo.lock` pins) the way
   `third_party/esp-alloc/README-LP.md` describes, into `third_party/esp-rtos`,
   and add `esp-rtos = { path = "third_party/esp-rtos" }` to the root
   `[patch.crates-io]`.
2. `git apply probes/rtos-tick/esp-rtos-0.3.0-lp-tick-probe.patch` (adds the
   `lp-tick-probe` feature: counters in `lp_probe`, CCOUNT reads in the two
   handlers — inline asm, because `xtensa_lx::timer::get_cycle_count` is only
   `#[inline]` and opt-level z outlines it into flash).
3. `git apply probes/rtos-tick/fw-esp32v3-rtos_tick_probe.patch` (adds the
   `rtos_tick_probe` feature and `src/rtos_tick_probe.rs`, which prints one
   `[RTOS-TICK]` line per 10 s from the frame-write path, next to the ws281x
   telemetry tap). Both patches are against main `5f20c4b47`; re-fit by hand
   if either side moved.
4. `cd lp-fw/fw-esp32v3 && touch src/main.rs && cargo build --profile
   release-esp32v3 --features rtos_tick_probe`, flash with `--monitor
   --monitor-baud 921600` and a render-heavy project resident
   (`projects/test/zook-dome-1500`; the line only prints while frames are
   being written), capture 60 s or more, then
   `python3 probes/rtos-tick/summarize.py <capture.log>`.

Detach the monitor with a SIGINT scoped to the port
(`pkill -INT -f "espflash flash.*--port $PORT"`), never a bare pkill: other
boards share the desk bus. `timeout -s INT` in front of espflash did not end
it here; the scoped pkill did, every time.

## Results, 2026-09-07 (DOM-Z-102, zook dome 1500 lamps on 4 wires, 240 MHz)

`captures-2026-09-07/` holds the `[RTOS-TICK]` and `[perf]` lines of every
run; `summarize.py` on them reproduces the table. CPU clock 240 MHz, so
`cycles / 240` is microseconds.

| image | tick: calls/s, mean, max | tick % PRO | yield: calls/s, mean, max | yield % PRO | frame (`[perf] tick=`) |
|---|---|---|---|---|---|
| `before` (probe only; handlers' callees in flash) | 18.0/s, 5.8 µs, 6.3 µs | 0.010 % | 36.4/s, 10.4 µs, 40.8 µs | 0.038 % | 54.0 ms |
| `before2` (repeat) | 18.0/s, 5.8 µs, 6.3 µs | 0.010 % | 36.4/s, 10.4 µs, 40.8 µs | 0.038 % | 54.0 ms |
| `after` (`now`, `arm_next_wakeup`, `set_state`, `ensure_no_stack_overflow` `#[ram]`, `Priority::new`/`P::from_usize` `#[inline(always)]`; +880 B `.rwtext`) | 19.3/s, 6.2 µs, 6.7 µs | 0.012 % | 39.1/s, 4.6 µs, 5.5 µs | 0.018 % | 50.3 ms |
| `after2` (repeat) | 19.3/s, 6.2 µs, 6.8 µs | 0.012 % | 39.1/s, 4.6 µs, 5.5 µs | 0.018 % | 50.3 ms |
| `hotpair` (`now` + `arm_next_wakeup` only) | 18.5/s, 6.1 µs, 6.8 µs | 0.011 % | 37.3/s, 12.6 µs, 21.5 µs | 0.047 % | 52.6 ms |
| `coldmove` (layout control: 590 B of *cold* esp-rtos code moved to RAM, hot path untouched but shifted) | 18.9/s, 5.8 µs, 6.3 µs | 0.011 % | 38.2/s, 14.4 µs, 56.8 µs | 0.055 % | 51.2 ms |
| `pad780` (layout control: 780 B of dead text at the end of `.text`) | 18.0/s, 5.8 µs, 6.3 µs | 0.010 % | 36.4/s, 10.4 µs, 40.7 µs | 0.038 % | 53.9 ms |
| `control` (pristine main, no probe) | — | — | — | — | 56.0 ms |

Reading it:

- **The tick handler is register-bound, not fetch-bound.** 5.8 µs per call
  with `now` + `arm_next_wakeup` in flash, 6.2 µs with them in RAM: the time
  is the LACT update-and-poll read and the TIMG alarm reprogramming (which
  still runs esp-hal's `timg::Timer::load_value` from flash either way),
  not instruction fetch. At 18 calls/s — the embassy alarm cadence at this
  frame rate, not the 100 Hz `tick_rate_hz`, which only arms a timeslice
  when two ready tasks compete — it is 0.010 % of the PRO core.
- **The only real effect of the RAM move is yield latency**: 41 µs worst
  case to 5.5 µs, mean 10.4 µs to 4.6 µs, at 36 calls/s (0.038 % to
  0.018 %). Nothing on the PRO core has a deadline that 35 µs of context-
  switch jitter threatens.
- **The frame-time column is a layout artefact, not the handlers.** The
  zook frame moved 54.0 → 50.3 ms between `before` and `after`, but
  `coldmove` — which only *shifts* the hot path by moving cold code out of
  the same flash region — gives 51.2 ms with the handlers unchanged, and
  pristine main gives 56.0 ms. Flash code placement alone swings this
  project's frame time across a 10 % band (50–56 ms). Any before/after
  fps claim on the classic needs a layout control.
