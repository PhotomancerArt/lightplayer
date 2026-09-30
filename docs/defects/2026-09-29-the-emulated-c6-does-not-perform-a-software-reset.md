---
status: open
found: 2026-09-29      # how: live-debugging, the emulator-driven hardware walk's `--request reboot`
area: lp-emu/esp/lp-emu-esp32c6 periph/accept.rs (LP_AON stored, not acted on)
class: fidelity
related:
  - lp2025/2026-09-26-2215-lp-link-comms-layer
  - docs/defects/2026-09-22-emulated-reset-restores-rtc-fast-persistent.md
---
# The emulated C6 does not perform a software reset: `software_reset()` returns, and the watchdog reboots the chip seconds later

**Symptom** — `lp-cli link capture tcp://… --request reboot` against
`lp-cli emu run --link … --reboot-on-reset` (the shipped `fw-esp32c6`, and
its `frame-dump` build): the board answers `M!{"id":1000000,"msg":"reboot"}`
at once, then the emulator logs

```text
UNMAPPED write1 at 0x00000000 from pc=0x4207d188
UNMAPPED write1 at 0x00000001 from pc=0x4207d18c
…
machine: LP_WDT stage 0 (ResetSystem) at cycle 17797971233 — rebooting into strap app, rst:0x10 (LP_WDT_SYS)
```

and the rebooted firmware's recovery ledger reads
`[RECOVERY] boot: cause=watchdog-reset`. The restart comes ~8 s of emulated
time after the request (the RWDT's runtime stage), not microseconds after
it. Under `--strict-bus` the first unmapped write stops the run instead, and
the board never restarts at all.

**Root cause** — `esp_hal::system::software_reset()` calls the ROM's
`software_reset` (`0x4001973c`, through the `0x4000_0090` trampoline), which
is five instructions: set bit 31 of `0x600B_1034`
(`LP_AON.sys_cfg.hpsys_sw_reset`) and `ret`. On silicon the chip is in reset
before the `ret` matters. In the emulator `LP_AON` is an accept block
(`periph/accept.rs`): the write is stored and nothing happens, so the ROM
returns into esp-hal's `-> !` function, which has no code after the call and
falls through into the next symbol in flash
(`core::char::methods::encode_utf8_raw_unchecked`, pc `0x4207d188`), writing
through whatever its registers hold — address 0 here — until the LP
watchdog, which the firmware arms at 8 s of runtime, fires a system reset.
Every software reset the firmware performs is affected: the `Reboot`
request, `PowerButton`'s soft power-off path (`hardware/power.rs`), the
recovery backend and the panic path.

**Fix** — none yet. The shape: `LP_AON`'s `sys_cfg` write with bit 31 set
raises the same reset request the LP_WDT's `ResetSystem` stage does, with
the cause silicon reports for it (`rst:0x3`, `RTC_SW_SYS_RESET`, if the
ROM's reset-reason table agrees), so `--reboot-on-reset` reboots at once
and a run without it ends with `Outcome::Reset` as it does for the
watchdog.

**Regression coverage** — none yet. When fixed,
`lp-cli/tests/link_capture.rs`'s
`a_reboot_request_restarts_the_board_and_the_next_request_goes_to_the_new_session`
can take `--strict-bus` back (it runs without it because of this), and a
test beside the RWDT's should assert the reset cause the firmware reads.

**Lesson** — an accept block is a claim that writes there have no effect
worth modelling, and a reset bit is the opposite of that. A `-> !` function
in the guest turns an unmodelled side effect into execution of whatever
happens to follow it in flash, which is why this reads as a wild write and
a watchdog rather than as "the reset did nothing".
