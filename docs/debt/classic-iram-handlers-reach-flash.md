---
status: carried
since: 2026-08-25
logged: 2026-09-05
area: fw-esp32v3 interrupt paths (esp-hal 1.1.1 Xtensa dispatch, esp-rtos 0.3.0)
related:
  - docs/reports/2026-09-04-classic-ram-budget.md (lever 3, "side finding")
  - docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md
  - scripts/fw-iram-flash-refs.py
  - lp-fw/fw-esp32v3/src/output/rmt/shared_driver.rs (`with_app_core_stalled`)
---
# The classic's `#[ram]` interrupt paths still fetch from flash — in esp-hal and esp-rtos, not in ours

**Shape** — the RAM budget report's lever-3 side finding counted 63
flash-resident constants referenced from IRAM functions in the default
`release-esp32v3` build. Verified 2026-09-05 on main `0c2c06fd3` with
`scripts/fw-iram-flash-refs.py` (every `l32r` in `.rwtext`/`.vectors`
resolved and classified; 82 `l32r` instructions, 70 literal slots, 63 unique
values — the report's number). Read per branch, the picture splits three
ways:

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
   | esp-hal `__level_1/2/3_interrupt` | both | `panic_bounds_check` Location; level 3 also an 11-entry jump table (`.rodata.__level_3_interrupt`) for CPU-internal sources 6..16 — never taken here (nothing uses the CPU Timer/Software/Profiling lines) | `InterruptStatus::current` (80 B, level-triggered path), `InterruptStatusIterator::next` (127 B), `mapped_to_raw` (81 B, via `should_handle`) — **288 B run from flash on every peripheral interrupt, including the RMT refill interrupt on the APP core** |
   | esp-rtos `timer_tick_handler` | PRO, every tick | 24 — `debug!`/`trace!` pieces behind a `MAX_LOG_LEVEL_FILTER` check (dead at Info) and `unwrap_failed` tails | `esp_rtos::now` (77 B) and `TimeDriver::arm_next_wakeup` (575 B), unconditional; `u64 Display::fmt` log-gated |
   | esp-rtos `cross_core_yield_handler` (bound to FROM_CPU_INTR0 on the PRO core; the APP core never runs esp-rtos here) | PRO | 21 — log-gated, an `assert!(addr.is_multiple_of(16))` tail, `unwrap_failed` | `Task::ensure_no_stack_overflow` (77 B, whenever a task is current), `esp_rtos::now`, `arm_next_wakeup`; `delete_marked_tasks` is `#[cold]` and conditional |
   | esp-rtos `RunQueue::pop` / `mark_task_ready` / `resume_task` | PRO | 6 / 3 / 1 — log-gated + `unwrap_failed` | `Priority::new` — a **5-byte `const fn` outlined at `opt-level = "z"`** — on pop's unmark path; `TaskExt::set_state` (80 B) unconditionally at `mark_task_ready`'s entry |
   | esp-rtos embassy `__pender` | PRO | 4-entry jump table `.rodata.__pender` on the executed path (match on the executor id) + `BorrowMutError` tail | — |
   | esp-hal default GPIO handler | — | 40-entry jump table + panic tails; never fires (no GPIO interrupt is listened for) | `AnyPin::steal` |
   | esp-hal UART0/1/2 irq trampolines | PRO | the `PERIPHERAL` info pointer; UART0's fires for the async host link, UART1/2 unused | the whole esp-hal UART handler is flash anyway |
   | `__user_exception`, `__default_*_exception` | — | crash-path `Debug` formatting only | — |

   The jump tables escape esp-hal's `place-switch-tables-in-ram` (default
   on) because that option's `rwdata.x` patterns match `.rodata..Lswitch.table.*`
   (LLVM's switch *lookup* tables) and `.rodata.*_esp_hal_internal_handler*`;
   a `#[ram]` function's *jump* table is emitted as `.rodata.<function>`
   (`.rodata.__level_3_interrupt`, `.rodata.__pender`,
   `.rodata.*default_gpio_interrupt_handler` in the linker map) and falls
   through to flash `.rodata`.

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

**Carrying cost** — latency, unmeasured. On the APP core the private 32 KB
cache holds its whole flash working set (~1.6 KB: the 288 B of dispatch plus
the pusher path) after warm-up, so refill-interrupt entry pays a miss only
after eviction, which nothing on that core causes. On the PRO core the
render loop thrashes the cache, so each tick's `now` + `arm_next_wakeup`
(~650 B, ~20 lines) and each yield's `set_state`/`ensure_no_stack_overflow`
can miss; bounded by tens of microseconds per tick at 40 MHz flash, a
low-single-digit percent of the PRO core at worst. Worth one measurement
before spending RAM on it — the classic's heap is the scarcer resource
(docs/reports/2026-09-04-classic-ram-budget.md).

**Workarounds** —
- Re-verify after any esp-hal / esp-rtos bump or a change to a `#[ram]`
  path: `python3 scripts/fw-iram-flash-refs.py target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3`
  (Xtensa binutils on PATH), then `--dump <fn>` and read the branch each hit
  sits on. A hit is a defect only if it is on the executed path; panic tails
  and `MAX_LOG_LEVEL_FILTER`-gated `debug!` bodies are not.
- Never raise the runtime log level to Debug/Trace on a running classic: it
  turns the 24 + 21 gated reads in the tick and yield handlers into live
  flash reads *and* serial traffic from interrupt context (see the memory
  note on the Debug level crashing the lab tab).

**Paying down (in order of return)** —
1. esp-hal upstream (or a local patch under `[patch.crates-io]`): `#[ram]` on
   `InterruptStatus::current`, `InterruptStatusIterator::next` and
   `mapped_to_raw` (288 B), and an `rwdata.x` pattern for `.rodata.__level_*_interrupt`
   / `.rodata.__pender`. Locally, the report's lever-3 `rwdata_hook.x` can
   route those three input sections into `.data` for ~0xd0 bytes.
2. esp-rtos upstream: `#[ram]` on `now`, `arm_next_wakeup`, `set_state`,
   `ensure_no_stack_overflow`, and `#[inline(always)]` on `Priority::new`
   (~900 B if all moved).
3. Firmware: `#[esp_hal::ram]` on `io_pacer_isr` is free but only moves the
   trampoline; its callees are embassy's. Moving the pusher path costs
   ~1.3 KB of IRAM for no deadline it currently misses — do it only if a
   measurement shows APP-core misses delaying refills.
