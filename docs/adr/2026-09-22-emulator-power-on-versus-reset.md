# ADR: Power-on versus reset in the emulator: LP-domain state survives a reset

- **Status:** Accepted
- **Date:** 2026-09-22
- **Deciders:** Photomancer (Yona) — director-run (`yona-direct`), hands-off;
  the ADR itself is the director's call (plan `notes.md` D8)
- **Supersedes:** None
- **Superseded by:** None

Plan: `lp2025/2026-09-22-0015-c6-lp-domain-reset` (P1
[#774](https://github.com/PhotomancerArt/lightplayer/pull/774), P2
[#775](https://github.com/PhotomancerArt/lightplayer/pull/775), P3
[#776](https://github.com/PhotomancerArt/lightplayer/pull/776), P4 this PR).

## Context

The ESP32-C6 emulator was built in part to reproduce one specific defect:
`docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`
and its bench diagnosis,
`docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md`. A board
whose LP peripheral clocks have the analog I2C master's clock gated
(`LPPERI_CLK_EN` bit 29 clear — a previous firmware's own runtime write, or
the same word simply never having been set) hangs the ESP-IDF second-stage
bootloader spinning on `LP_I2C_ANA_MST`'s busy bit, before the bootloader's
first console line. A watchdog eventually resets the chip
(`rst:0x7 (TG0_WDT_HPSYS)`), and the next boot lands in the identical hang —
for ever — because the reset that fired was an HP-only one and the wedge
lives in the LP (low-power/RTC) domain, which that reset never touches.
Only removing power, or a flasher's specific three-register cure
(`LPPERI_CLK_EN` bit 29 set, `LPPERI_RESET_EN` bit 29 pulsed), clears it.

**Before this plan, the emulator could not reproduce the loop at all,
because every `reboot()` it performed was already a whole-snapshot
restore** — memory and every peripheral's `save_state`, indiscriminately.
That is a power cycle by any other name. A model in which every reset is a
power cycle has no way to express "a reset that leaves some state standing"
— which is exactly the property the defect's whole loop depends on. P1
modelled the LP analog master and its gate (AC1); P2 gave MWDT0 a real
expiry and traced the watchdog's own arming to a reset value, not the ROM
(`TIMG0.wdtconfig0`'s power-on value, bit 14 `flashboot_mod_en` — see the
wording fix landing beside this ADR: it is reset-armed and
bootloader-disarmed, never "ROM-armed"); P2's own loop closed, but by
accident — the induced board's seeded gate word happened to equal its own
power-on value, so a whole-restore reboot put the wedge back without the
model proving anything about *domains*. P3 built the actual domain model
this ADR records; P4 (this phase) closes two gaps P3 left open, writes this
record, and files the fidelity gap the domain model does not yet reach.

The archived Seeed factory firmware that produced the real-world incident
(`~/.photomancer/firmware-archive/seeed-xiao-esp32c6/2026-09-21-factory-10bda3b0a52c/`)
is **not** in this repository, for licence reasons — see the memory note
`c6-factory-firmware-archive` — and stays reference material only. Nothing
in this plan copies its bytes into the tree; the induced-board fixture below
is built from one register value the bench read off a wedged board, not
from the factory image.

## Decision

**A block declares its own power domain; a reset restores the high-power
(HP) domain and leaves the low-power (LP) domain standing; only a power
cycle clears the LP domain.**

- **`Peripheral::domain() -> Domain::{Hp, Lp}`** (`lp-emu-esp-common`),
  defaulting to `Hp`. The declaration lives on the block, not as a list in
  `machine.rs` — see Alternatives Considered.
- **`Esp32C6Machine::reboot(strap, cause)`** restores HP-domain peripherals
  from the power-on snapshot via `SocBus::restore_peripherals_in`, and
  leaves LP-domain peripherals' state untouched.
  **`Esp32C6Machine::power_cycle(strap)`** is the old whole-restore,
  peripherals and everything else, under the reading "the supply came
  back."
- **`ResetSource::PowerOn`** is the one cause nothing *inside* the chip can
  ask for — the run loop dispatches a `PowerOn` cause to `power_cycle`,
  every other cause to `reboot`, in one place
  (`Esp32C6Machine::run_until`'s reset arm), so the two kinds of restart
  cannot drift apart. On the C6, `power-cycle` is a control verb over the
  `emu serve` door, beside `reset` — nothing inside the modelled chip
  produces `PowerOn` on its own, which matches silicon: no watchdog stage
  action clears the LP domain either.
- **The LP set, ten blocks, each with its own evidence** (pinned by
  `machine.rs::the_lp_domain_is_exactly_these_ten_blocks`, in registration
  order): `LP_APM`, `LP_APM0`, `LP_AON`, `PMU`, `LP_I2C_ANA_MST`,
  `LP_TIMER`, `LP_TEE`, `LP_IO`, `RNG` (the `LP_PERI` window — the register
  the whole plan is about), `LP_ANA`. `LP_APM0` and `LP_ANA` moved from `Hp`
  to `Lp` in this phase (DD11, below) — P3 held them out only because
  nothing reads a status bit out of either, which the director ruled is not
  a reason to call a genuinely LP-island block `Hp`.
- **Three blocks are deliberately kept `Hp`-restored**, each with a named
  reason rather than left unmentioned (`periph/accept.rs`'s "the reset
  domains" section): `LP_CLKRST` (its one interesting register,
  `reset_cause`, is an *input to the run* that the reboot path re-pokes
  with the new cause anyway — a block whose state is rewritten after every
  restore gains nothing from a domain); `LP_WDT` (a live RWDT carried
  across a reboot could fire spuriously mid-boot, and the ROM re-arms it
  regardless — deferred conservatively, D4); `EFUSE` (constant for the life
  of a chip, so restoring it and keeping it are the same statement).
- **`ASSIST_DEBUG` stays `Domain::Hp`**, and the reset path pokes its
  `Saved PC` register back *after* the HP restore, from the reset event
  itself (DD9). The value is produced by the act of resetting, not carried
  across it by a block — giving the block an LP domain would get the right
  value under the wrong theory, and would carry its other fifty registers
  along as a side effect nobody asked for.
- **Two fixtures, because the bench has two real shapes.** `--lpperi-clk-en`
  seeds the gate word *into the power-on snapshot* (a board whose previous
  firmware permanently re-gates the clock every boot — AC4a); poking the
  same register *after* a clean boot models a board that was running fine
  and only got wedged at runtime (silicon's own story for
  `docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md` — AC4b,
  this phase). A `power_cycle()` hands the first fixture straight back
  wedged (its power-on value *is* the wedge) and cleans the second (its
  power-on value never was) — both are real, and DD10 is the ruling that
  they are not in tension: "only power removal clears the LP domain" says
  nothing about what a board's power-on value happens to be.

## Consequences

- **What `reset` means through the `emu serve` door changes for every
  consumer** — Studio-in-a-tab, a device scenario, a hand-driven walk. A
  `reset` (the door's existing verb) now genuinely means "the kind of reset
  a host's DTR/RTS dance or a watchdog performs," which leaves LP state
  standing; a caller that wants a guaranteed-clean board asks for
  `power-cycle` instead. Nothing that only ever ran clean boards notices —
  the LP set's registers all read the same values on both paths when
  nothing has wedged them — but a scenario that seeds `--lpperi-clk-en` and
  expects `reset` to clean the board would now be wrong to expect that.
- **The induced board is a first-class, committed test fixture** —
  `--lpperi-clk-en <hex>` on `lp-emu-esp32c6`'s binary and on `lp-cli emu
  run`/`serve` — rather than something that could only be reproduced by
  hand on a physical board that happened to be in that state.
- **The archived factory image stays outside the repository.** This ADR
  changes nothing about that; it is named here because the induced-board
  fixture is this plan's substitute for running that image, and a reader
  should not go looking for it in the tree.
- **Two gaps are now named rather than silently absent**, both filed as
  follow-ups below and one as a defect: memory regions and the scheduler
  restore wholesale on every reset regardless of domain
  (`docs/defects/2026-09-22-emulated-reset-restores-rtc-fast-persistent.md`,
  F10), and `ResetScope::System`'s doc claimed a behaviour
  (`lp-emu-esp-common::periph::ResetScope::System`, "everything, LP domain
  included") that the dispatch never implements — reworded in this phase to
  state what the code does today (F9).
- **The invariant this whole plan runs under holds**: every committed
  `lp-emu-esp32c6-t1` transcript and every `rom_up_boot`/`boot_idle` line
  stays byte-identical, because no committed transcript-graded run ever
  reboots. The domain model only changes behaviour on a path
  (`reboot()`/`power_cycle()`) nothing byte-graded exercises yet.

## Alternatives Considered

- **Keep the whole-snapshot restore as the only reset** (the status quo
  before this plan). Rejected as the behaviour of `reboot()`: it is exactly
  what makes the emulator unable to reproduce the one defect it exists to
  reproduce — a reset that clears the LP domain cannot boot-loop a wedged
  board, and cannot distinguish a reset from a power cycle at all.
- **A list of LP block names living in `machine.rs`.** Smaller diff, one
  place to read the whole answer at a glance. Rejected (D5): a list drifts
  — a new LP block is added by someone editing `periph/mod.rs`, who has no
  particular reason to also go edit a list in a different file — where a
  `domain()` override puts the claim next to the evidence for it, and the
  pinning test (`the_lp_domain_is_exactly_these_ten_blocks`) gives back the
  single place to read the whole answer that the list would have had,
  without the drift risk.
- **`Domain::Lp` for `ASSIST_DEBUG`.** Would have produced the right `Saved
  PC` with no explicit poke in the reset path. Rejected (DD9): the value is
  produced by the reset *event*, not carried by the block across it, and a
  domain would carry the block's other fifty registers along for a reason
  that has nothing to do with why `Saved PC` needs to survive. A third
  `Domain` value ("reset by a narrower signal than the watchdog itself
  asserts") was named and rejected too — a concept with exactly one member
  and one register behind it.
- **`LP_WDT` and `EFUSE` given `Domain::Lp`.** Both are real LP-island
  blocks by the same address-prefix argument that put ten other blocks in
  the set. Deliberately left `Hp`-restored instead (D4): a live RWDT
  carried across a reboot could fire spuriously mid-boot on a build the ROM
  itself re-arms regardless, and eFuse is constant for the life of a chip —
  restoring it and keeping it are the same statement, so there is nothing
  to gain and one more untested behaviour to carry if it were wrong. Marked
  *deferred*, not *decided*: nothing here rules out revisiting either.
- **`LP_APM0` and `LP_ANA` left `Hp`-restored** (P3's original cut).
  Reasoned then as "nothing reads a status bit out of either, so carrying
  their state changes no decision any image makes." The director reversed
  this in P4 (DD11): the domain model's whole premise (D5) is that a block
  declares what it *is*, and both are genuinely LP-island blocks by the
  same base-address evidence every other member of the set relies on —
  leaving them `Hp` because nobody currently reads them back makes the
  rule "the block declares what it is" quietly mean "the block declares
  what it is, when that happens to be observable," which is a different
  and weaker rule. Both are `modeled`: no image reboots between a write to
  either block and a read of it today, so the claim is asserted rather than
  measured, same as every other domain claim here.

## Follow-ups

- **F5** (bench, needs hands): hold a C6 in ROM download mode over USB,
  read `TIMG0.wdtconfig0` bit 14 and whether it resets — settles whether
  flash-boot watchdog protection really counts only on a flash-boot strap
  (P2's `Timg::set_flash_boot`, `modeled`).
- **F6**: widen `Saved PC` to every `chip_rst` reset, matching silicon
  (today it is printed for watchdog causes only) — needs a
  committed-transcript decision, since widening it changes a console line
  on every reboot transcript.
- **F7**: `lp-emu-esp32s3`/`lp-emu-esp32v3`'s `reboot(strap)` still
  hard-code their own reset cause; sweep when either chip next gets reset
  work.
- **F8**: capture the reset PC in `on_event` so `Saved PC` is exactly
  `0x4086ed7a` on every console path (today it lands on `…7c`/`…7e` over
  USB-Serial-JTAG, a slice-boundary quantisation artefact, not a modelling
  error — see `tests/bootloader_hang.rs`'s own note on it).
- **F9**: `ResetScope::System` is declared but never dispatched on; an RWDT
  `ResetSystem` reset should clear the LP domain and today performs a
  `reboot()` that keeps it. The doc now states this; the dispatch does not
  yet honour it.
- **F10** (filed as a defect,
  `docs/defects/2026-09-22-emulated-reset-restores-rtc-fast-persistent.md`):
  memory regions and the scheduler restore wholesale on every reset,
  regardless of domain — `.rtc_fast.persistent` does not survive an
  emulated reset although the ROM's own `__pre_init` zeroing it only on
  `POWERON` proves silicon keeps it.
- **F11**: `power-cycle` is reachable from `emu serve`'s door and the
  workshop binary's `--control`/`--usb-script`, but not yet from `lp-cli
  emu run` (which has no control channel — only `--link` bytes).
- **F12**: `--lpperi-clk-en` on `emu serve` is serve-wide, not
  per-`--board`; a walk that wants one wedged board beside one clean board
  in a single `serve` needs a second serve today.
