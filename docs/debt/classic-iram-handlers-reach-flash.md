---
status: paying-down
since: 2026-08-25
logged: 2026-09-05
updated: 2026-09-07
area: fw-esp32v3 interrupt paths (esp-hal 1.1.1 Xtensa dispatch, esp-rtos 0.3.0)
related:
  - docs/reports/2026-09-04-classic-ram-budget.md (lever 3, "side finding")
  - docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md
  - scripts/iram-flash-literals.py
  - third_party/esp-hal/README-LP.md (the #[ram] fork, pay-down item 1)
  - lp-fw/fw-esp32v3/rwdata_hook.x (the jump-table globs, pay-down item 1)
  - lp-fw/fw-esp32v3/src/output/rmt/shared_driver.rs (`with_app_core_stalled`)
  - lp-fw/fw-esp32v3/probes/rtos-tick/ (the esp-rtos measurement, 2026-09-07)
---
# The classic's `#[ram]` interrupt paths still fetch from flash — in esp-hal and esp-rtos, not in ours

**Shape** — the RAM budget report's lever-3 side finding counted 63
flash-resident constants referenced from IRAM functions in the default
`release-esp32v3` build. Verified 2026-09-05 on main `0c2c06fd3` with
`scripts/iram-flash-literals.py` (every `l32r` in `.rwtext`/`.vectors`
resolved and classified; 82 `l32r` instructions, 70 literal slots, 63 unique
values — the report's number), and re-verified 2026-09-06 on main
`2cf7301d3` after #521 turned `place-switch-tables-in-ram` off with
`rwdata_hook.x`: the table below is unchanged and `fill_half` still loads
zero flash literals. Read per branch, the picture splits three ways:

1. **Our two handlers are clean on their executed paths.**
   `shared_driver::rmt_isr` (APP core, level 3): its 3 constants are
   `core::panic::Location`s at the function tail, paired with
   `panic_const_rem_by_zero` and two `panic_bounds_check`s, reached only by
   cold branches. Every executed load resolves to IRAM/DRAM; the one indirect
   call is `fill_half` at `0x40080d14` (IRAM), the other `note_wake` (IRAM).
   `wire_pusher::run` (APP core, thread): its 2 constants are
   `slice_index_fail` Locations, panic-only. Nothing to mark `#[ram]`.

2. **The third-party constants are log-gated or panic-only — but the same
   functions call flash *code* on every execution.** That is the actual
   condition this entry carries:

   | IRAM function | core | rodata reads (all cold) | flash code on the executed path |
   |---|---|---|---|
   | esp-hal `__level_1/2/3_interrupt` | both | `panic_bounds_check` Location; level 3 also an 11-entry jump table (`.rodata.__level_3_interrupt`) for CPU-internal sources 6..16 — never taken here (nothing uses the CPU Timer/Software/Profiling lines). **Table in `.data` since 2026-09-07** (`rwdata_hook.x`) | `InterruptStatus::current` (80 B, level-triggered path), `InterruptStatusIterator::next` (127 B), `mapped_to_raw` (81 B, via `should_handle`) — 288 B that **used to** run from flash on every peripheral interrupt, including the RMT refill interrupt on the APP core. **In `.rwtext` since 2026-09-07** via the vendored esp-hal fork (`third_party/esp-hal`, +376 B of IRAM with the entries' literal pools); `mapped_to_raw` keeps two flash literals, both its bounds-check panic tail |
   | esp-rtos `timer_tick_handler` | PRO, every embassy alarm (18/s under zook; the 100 Hz `tick_rate_hz` timeslice only arms when two ready tasks compete) | 24 — `debug!`/`trace!` pieces behind a `MAX_LOG_LEVEL_FILTER` check (dead at Info) and `unwrap_failed` tails | `esp_rtos::now` (77 B) and `TimeDriver::arm_next_wakeup` (575 B), unconditional, and under the latter esp-hal's `timg::Timer::load_value` (286 B) → `apb_clk_config_frequency` (flash) and `__udivdi3` (ROM); `u64 Display::fmt` log-gated. **Measured 2026-09-07: 5.8 µs per call, 6.3 µs max, 0.010 % of the PRO core; 6.2 µs with the esp-rtos callees in RAM** — register-bound (LACT poll + TIMG reprogramming), not fetch-bound |
   | esp-rtos `cross_core_yield_handler` (bound to FROM_CPU_INTR0 on the PRO core; the APP core never runs esp-rtos here) | PRO, 36/s under zook | 21 — log-gated, an `assert!(addr.is_multiple_of(16))` tail, `unwrap_failed` | `Task::ensure_no_stack_overflow` (77 B, whenever a task is current), `esp_rtos::now`, `arm_next_wakeup`; `delete_marked_tasks` is `#[cold]` and conditional. **Measured: 10.4 µs mean, 41 µs max, 0.038 % of the PRO core; 4.6 µs / 5.5 µs / 0.018 % with the callees in RAM** — the one place the flash residue is visible, as worst-case yield latency |
   | esp-rtos `RunQueue::pop` / `mark_task_ready` / `resume_task` | PRO | 6 / 3 / 1 — log-gated + `unwrap_failed` | `Priority::new` — a **5-byte `const fn` outlined at `opt-level = "z"`** — on pop's unmark path (and `P::from_usize` under it: `#[inline(always)]` on `new` alone just outlines the callee instead); `TaskExt::set_state` (80 B) unconditionally at `mark_task_ready`'s entry. Inside the yield figure above |
   | esp-rtos embassy `__pender` | PRO | 4-entry jump table `.rodata.__pender` on the executed path (match on the executor id) — **in `.data` since 2026-09-07** (`rwdata_hook.x`) — + `BorrowMutError` tail | — |
   | esp-hal default GPIO handler | — | 40-entry jump table + panic tails; never fires (no GPIO interrupt is listened for) | `AnyPin::steal` |
   | esp-hal UART0/1/2 irq trampolines | PRO | the `PERIPHERAL` info pointer; UART0's fires for the async host link, UART1/2 unused | the whole esp-hal UART handler is flash anyway |
   | `__user_exception`, `__default_*_exception` | — | crash-path `Debug` formatting only | — |

   The jump tables escape both esp-hal's `place-switch-tables-in-ram`
   patterns and the classic's own `lp-fw/fw-esp32v3/rwdata_hook.x` (#521),
   because both match `.rodata..Lswitch.table.*` (LLVM's switch *lookup*
   tables) and `.rodata.*_esp_hal_internal_handler*`; a `#[ram]` function's
   *jump* table is emitted as `.rodata.<function>`
   (`.rodata.__level_3_interrupt`, `.rodata.__pender`,
   `.rodata.*default_gpio_interrupt_handler` in the linker map) and falls
   through to flash `.rodata`. `rwdata_hook.x` now names the two that are on
   an executed path — `*(.rodata.__level_*_interrupt)` and
   `*(.rodata.__pender)`, 0x2c + 0x10 B, `.data` 12,252 → 12,308 B — and
   leaves the GPIO handler's 0xa0 B table in flash, since nothing listens for
   a GPIO interrupt.

3. **Firmware-side residue of the same rule, priced and accepted.**
   `wire_pusher::run` is `#[ram]` but the work it drives is not:
   `Pusher::dispose_through` (190 B), `Pusher::release_wire_slot` (91 B),
   `Ws281xDriver::start_frame` (383 B), `ChannelState::stats` (146 B),
   `v3_rmt::route_rmt_to_gpio` (284 B) and `park_gpio` (208 B) — about
   1.3 KB of flash code on the APP core's thread path. The module doc used to
   claim "everything on this path is `#[ram]`"; it now says what is and is
   not, and why. `serial::io_task::io_pacer_isr` (PRO core, level 1, 122 B)
   is not in RAM at all and calls the critical-section acquire and the
   embassy waker in flash.

**Why it does not fault** — the write-window hazard the ISR-in-RAM rule
guards against cannot reach these paths on this firmware. The vendored
`esp-storage` 0.9.0 (`critical-section` feature) wraps each ROM flash op in
an `esp_sync::RawMutex` critical section — interrupts masked on the calling
core — and never disables a cache; so the PRO core takes no interrupt while
the flash is in program/erase mode, and the APP core is hardware-stalled by
`with_app_core_stalled` across every littlefs write and erase (littlefs is
the only esp-storage writer). A flash fetch from an interrupt path can
therefore only ever *miss*, never read a mid-program flash.

**Carrying cost** — latency. **Measured 2026-09-07 on DOM-Z-102** (Zook
dome resident, 5 × 300 lamps, ~35 fps, 190–200 s per image, the
`ws281x_telemetry` build before and after pay-down item 1): no starvation
either way — `trips`/`skips`/`errors` 0, `refills` = `wanted` − `frames`
exactly, `lag_avg`/`lag_max` unchanged (10.4 / 15 words) — so the null result
on the refill deadline is the expected one. But the **entry-delay** histogram
(what a service spends between the interrupt and its first word, bucketed in
eighths of the 32-word half, 4 words ≈ 5 µs per bucket) moved on every channel,
clearest on ch 0, the first channel serviced per snapshot and therefore the
one that measures dispatch alone: buckets 0..4 went from
481,675 : 60,563 : 134,668 : 100,974 : 72 to 611,554 : 117,506 : 29,575 :
1,396 : 1 — services delayed 8–16 words (10–20 µs) fell from ~30 % to ~4 %,
and `entry_max` fell 34 → 32 (ch 0), 42 → 39, 45 → 35, 43 → 35 (ch 2/4/6).
That is consistent with the dispatch path missing the APP core's cache on a
large share of refill interrupts, ~10 µs a time against a 40 µs half — which
would make the earlier "boot-only miss" claim wrong. One caveat before
treating it as proven: this was one image pair, and flash code layout alone
moves things on this board (the sibling esp-rtos measurement saw zook frame
time move 50–56 ms between images whose hot path was unchanged), so a
layout-only control — same helpers left in flash, some other cold code moved
— was not run and would be the next measurement if the number ever matters.
Nothing was at the deadline, which is why the counters that matter did not
move; the margin did.
Before that measurement the reasoning here was: on the APP core the private
32 KB cache should hold its whole flash working set (~1.6 KB: the 288 B of
dispatch plus the pusher path) after warm-up, so refill-interrupt entry would
pay a miss only after eviction. On the PRO core the
render loop thrashes the cache, so each tick's `now` + `arm_next_wakeup`
(~650 B, ~20 lines) and each yield's `set_state`/`ensure_no_stack_overflow`
can miss. **Measured 2026-09-07** on the DOM-Z-102 under
`projects/test/zook-dome-1500` (CCOUNT around both handlers, procedure and
captures in `lp-fw/fw-esp32v3/probes/rtos-tick/`): the tick handler costs
5.8 µs per call at 18 calls/s, **0.010 % of the PRO core**, and moving its
esp-rtos callees into RAM does not make it faster (6.2 µs — the time is the
LACT read and the TIMG alarm reprogramming, not fetch); the yield handler
costs 10.4 µs mean / 41 µs max at 36 calls/s, **0.038 %**, and in RAM
4.6 µs / 5.5 µs, 0.018 %. The whole esp-rtos residue is therefore under
0.05 % of the PRO core, and the RAM move (+880 B `.rwtext`, which is free —
16 KB sit between `rwtext_end` and the JIT region, no DRAM involved) buys
35 µs of worst-case yield latency that nothing on that core needs. A
side finding worth more than the measurement: **flash code layout alone
moves this project's frame time across 50–56 ms** (pristine main 56.0,
the probe image 54.0, a control that shifted the same flash region without
touching the hot path 51.2, the RAM-move image 50.3), so a before/after
fps delta on the classic is not evidence of anything without a layout
control. **Diagnosed 2026-09-07** (`lp-fw/fw-esp32v3/probes/flash-layout/`,
the "Layout noise floor" paragraph of
`docs/debt/dome-scale-dev-frames-miss-60fps.md`): the per-core flash cache
is two-way with 32-byte blocks and the per-lamp working set was scattered
over 1.7 MB of `.text`; `hot_text.x` now pins it to the head of `.text`,
and the shift sweep there is the noise floor to quote.

**Workarounds** —
- Re-verify after any esp-hal / esp-rtos bump or a change to a `#[ram]`
  path: `just iram-flash-literals-esp32v3` is the baseline gate (no function
  may gain a literal); `just iram-flash-literals-esp32v3 --dump rmt_isr
  fill_half` prints the annotated disassembly so each hit can be read on the
  branch it sits on. A hit is a defect only if it is on the executed path;
  panic tails and `MAX_LOG_LEVEL_FILTER`-gated `debug!` bodies are not.
- Never raise the runtime log level to Debug/Trace on a running classic: it
  turns the 24 + 21 gated reads in the tick and yield handlers into live
  flash reads *and* serial traffic from interrupt context (see the memory
  note on the Debug level crashing the lab tab).

**Paying down (in order of return)** —
1. **PAID 2026-09-07** (locally): `third_party/esp-hal` is esp-hal 1.1.1 with
   `#[ram]` on `InterruptStatus::current`, `InterruptStatusIterator::next`
   and `mapped_to_raw`, patched in through the root `[patch.crates-io]`
   (`.rwtext` 14,848 → 15,224 B on the classic), and `rwdata_hook.x` routes
   `.rodata.__level_*_interrupt` / `.rodata.__pender` into `.data` (+56 B).
   Still owed: the upstream PR to esp-rs/esp-hal (main is unmarked as of
   2026-09-07) so the fork can be dropped; the fork's README-LP.md says how
   to re-sync until then. The attributes also apply to the RISC-V
   `handle_interrupts` callees on the C6/S3, which is the same rule there.
2. ~~esp-rtos upstream: `#[ram]` on `now`, `arm_next_wakeup`, `set_state`,
   `ensure_no_stack_overflow`, and `#[inline(always)]` on `Priority::new`
   (~900 B if all moved).~~ **Retired 2026-09-07**: measured at 0.010 % of
   the PRO core for the tick and 0.038 % for the yield (above), so not worth
   a vendored fork. The change was built and verified anyway (880 B
   `.rwtext`; `Priority::new` needs `P::from_usize` inlined too, and
   `arm_next_wakeup` still reaches esp-hal's `timg::Timer::load_value` in
   flash); the patches to reproduce the measurement are in
   `lp-fw/fw-esp32v3/probes/rtos-tick/`. Reopen only for a PRO-core
   interrupt-latency need that 41 µs of yield jitter would break.
3. Firmware: `#[esp_hal::ram]` on `io_pacer_isr` is free but only moves the
   trampoline; its callees are embassy's. Moving the pusher path costs
   ~1.3 KB of IRAM for no deadline it currently misses — the 2026-09-07
   measurement above suggests APP-core misses are real, not boot-only (with
   the layout caveat there), so this is a priced option (~1.3 KB IRAM for the
   thread-context path) rather than a closed one; the refill ISR itself is
   already clean.
