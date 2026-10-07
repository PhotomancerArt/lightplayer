---
status: fixed
found: 2026-10-07      # how: emulator walk (try-and-recover loads, PR B #989)
fixed: this change
area: lp-emu-esp32c6 `Esp32C6::restart` × `SocBus::restore_regions`
class: assumed-context
related:
  - docs/adr/2026-10-07-project-loads-are-tried-and-recovered.md
  - docs/defects/2026-10-06-a-wifi-joined-c6-refuses-every-project-switch.md
---
# The emulated C6 cleared RTC fast memory on every reset

**Symptom** — an emulated C6 run with `--reboot-on-reset` ran out of
memory, printed `[RECOVERY] OOM committed to the RTC ledger`, and reset.
The next boot reported no crash (`level=green`, no "last run crashed"
line), and a project load that had reset the board was never known.
Silicon reports both.

**Root cause** — `restart()` restored every RAM region from the power-on
snapshot, LP SRAM included, even for an HP-domain reset. Peripherals were
restored by domain (`a_reboot_keeps_the_lp_domain_and_a_power_cycle_clears_it`
pinned `LP_AON`), but RAM was not. On silicon an HP reset does not reach
the LP island's SRAM, which is where the firmware's recovery region lives
(RTC fast memory, `#[ram(rtc_fast, persistent)]`). So every emulated reset
looked like a power cycle to recovery.

**Fix** — an HP-domain reset restores every region except `lp-sram`
(`SocBus::restore_regions_keeping`). A power cycle still clears it. The
test above now pins both.

**Lesson** — the reset domains have to cover memory too, not only
registers. Any emulated claim about crash reporting across a reset made
before this change was untested.
