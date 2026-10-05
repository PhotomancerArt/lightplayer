---
status: fixed
found: 2026-09-29      # how: live-debugging, the emulator-driven hardware walk's `--request reboot`
fixed: OTA update protocol Part A, P1 (2026-10-04)
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

**Fix** — `LP_AON` is no longer a bare accept block:
`lp-emu/esp/lp-emu-esp32c6/src/periph/lp_aon.rs` wraps the same `RegFile`
and performs the one bit in it that is not a memory. A store with
`sys_cfg.hpsys_sw_reset` (bit 31 of `+0x034`) set raises
`MachineRequest::Reset` with a new `ResetSource::Software`
(`lp-emu-esp-common`), and ends the slice (`yield_to_machine`), so the guest
runs no instruction after the store. The C6 maps it to
`ResetCause::LpSwHpSys`, `rst:0x3 (LP_SW_HPSYS)`. Evidence for the code, two
sources that agree: the vendored mask ROM's reset-reason name table (index 3
is `LP_SW_HPSYS`, the table already transcribed in `loader.rs`), and esp-hal
1.1.1's `SocResetReason::CoreSw = 0x03` for this chip, which the firmware's
`reset_cause_map` reads as `SoftwareReset`. The ESP32-C6 TRM's reset-source
table was not at hand; it is not cited. With `--reboot-on-reset` the machine
reboots at once (HP domain restored, LP domain kept, the watchdog's path);
without it the run ends with `Outcome::Reset`. The bit is stored cleared — a
strobe — because `LP_AON` is LP-domain and survives the reset, and a kept bit
would be written back by the next read-modify-write of `sys_cfg`.

Before / after, `lp-cli link capture --request reboot --request hello`
against `emu run --reboot-on-reset` (the shipped image): before, the restart
came ~8 s of emulated time later as `rst:0x10 (LP_WDT_SYS)` and the run could
not use `--strict-bus`; after, under `--strict-bus`, the capture reads `up (session 0) at 0.035 s`,
`reset (PeerRestarted) at 0.110 s` and `up (session 1) at 0.113 s` (host
wall-clock of the capture, not emulated time).

`.rtc_fast.persistent` across the reset is unchanged: it is the same reboot
path as the watchdog's, and
`2026-09-22-emulated-reset-restores-rtc-fast-persistent.md` stays open.

**Regression coverage** — `tests/rom_reset_reason.rs`
(`a_software_reset_reboots_at_once_and_the_rom_reads_lp_sw_hpsys`,
`without_reboot_on_reset_a_software_reset_ends_the_run_as_a_reset`: the
ROM's own `software_reset` run on the machine, and the ROM's
`rtc_get_reset_reason` reading `3` after it), `periph/lp_aon.rs`'s unit tests
(the request, the strobe), and `lp-cli/tests/link_capture.rs`'s
`a_reboot_request_restarts_the_board_and_the_next_request_goes_to_the_new_session`
back under `--strict-bus`.

**Lesson** — an accept block is a claim that writes there have no effect
worth modelling, and a reset bit is the opposite of that. A `-> !` function
in the guest turns an unmodelled side effect into execution of whatever
happens to follow it in flash, which is why this reads as a wild write and
a watchdog rather than as "the reset did nothing".
