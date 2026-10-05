---
status: fixed
found: 2026-10-03   # live-debugging, making the `?emu=` banner's D0 switch-mode power button
fixed: this change
area: lp-emu-esp-common (bus.rs, pins.rs) — shared by lp-emu-esp32c6, lp-emu-esp32s3, lp-emu-esp32v3
class: fidelity
related: []
---
# An emulated C6/S3/classic reset or power cycle dropped levels held on pads from outside the chip

**Symptom** — a bench driver's level on a pad (the `pin <n> <0|1>` control verb,
`--pin-script` lines, or `Fabric::drive_pad` from a host-side switch) disappeared
the instant the emulated chip reset or power-cycled: `pins` reported `drv=-`
where it had read `drv=0`/`drv=1` a moment before. The switch-mode power button
spike for PR #963 worked around it in the Studio page
(`lpa-studio-web/public/lpa-link/virtual_serial.js`, "The page's switches")
by re-sending `pin 0 1` after every observed reboot, rather than the emulator
holding the level itself.

**Root cause** — `SocBus::restore_scalars` (`lp-emu/esp/lp-emu-esp-common/src/bus.rs`),
called by every chip's `reboot()`/`power_cycle()`/`restart()`
(`Esp32C6Machine::restart`, and `Esp32S3Machine::reboot` / `Esp32V3Machine`'s
equivalents, which all restore scalars the same way), ended with
`self.pins = s.pins.clone()` — the whole `Fabric` overwritten from the
power-on snapshot. `Fabric` conflates two different things under one struct:
the chip's own registers (routing, `GPIO.out`, `GPIO.enable`, signal levels —
reset by a chip reset or power cycle) and the board's wiring
(`Fabric::driven`, set by `drive_pad` from outside the chip, and `wire` ties —
neither of which a reset touches on silicon, because they are not inside the
chip). A wholesale restore stood in for a narrower one that should have left
the board side alone — the same shape as
`docs/defects/2026-09-22-emulated-reset-restores-rtc-fast-persistent.md`
(a wholesale *region* restore standing in for one that should skip
`.rtc_fast.persistent`), one layer down in the same machine's restart path.

**Fix** — `Fabric::restore_chip_side` (`lp-emu-esp-common/src/pins.rs`) replaces
the fabric's chip-side fields (routes, levels, GPIO registers, signals, input
routing) from a snapshot while keeping this fabric's own `driven` (the outside
levels) and `tie`/`tied` (the `--wire` ties), then re-settles every pad so the
resolved level (and any edge) reflects the kept outside driver against the
restored chip state. `SocBus::restore_scalars` now calls it instead of cloning
`s.pins` wholesale. One function shared by all three chips, so the fix lands
for `lp-emu-esp32c6`, `lp-emu-esp32s3` and `lp-emu-esp32v3` at once.

A plain (non-reset) snapshot `restore()` — used by the determinism tests to
rewind into a freshly built machine — is unaffected: a fresh `Fabric` starts
with no outside driver and no tie, so keeping "this fabric's board side"
keeps nothing there and the restore is still whole in practice. No test or
committed transcript exercises a reset together with a driven pad, so none
needed accommodating the other way.

**Regression coverage** —
`lp-emu-esp-common::pins::tests::restore_chip_side_keeps_a_bench_driver_and_a_wire_but_resets_the_routing`
and
`lp-emu-esp-common::bus::tests::restoring_scalars_keeps_a_pad_an_outside_driver_holds`
(`cargo test -p lp-emu-esp-common`), both written to fail against the prior
`self.pins = s.pins.clone()` line and pass against `restore_chip_side`.

**Lesson** — a snapshot/restore struct that carries one named field (`pins`,
`.rtc_fast.persistent`, a region) for two different reasons invites a single
`clone()`/overwrite at the restore site to treat both reasons alike. When a
restore is meant to model a *physical* reset boundary (chip vs. board, HP
domain vs. LP domain, a region that survives `POWERON` vs. one that does not),
the split has to be made explicit at the restore call, not left to "the whole
struct comes back" — the convenient default is also the one silicon
disagrees with.
