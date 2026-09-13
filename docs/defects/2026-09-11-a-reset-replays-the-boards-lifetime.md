---
status: fixed
found: 2026-09-11      # live-debugging, the tab emulator's loose-ends plan (#710, "W3 — where it stands")
fixed: 4940c2b53       # #737, C6 only — the classic (lp-emu-esp32v3) has the same shape, not fixed
area: lp-emu/esp/lp-emu-esp32c6
class: assumed-context
related: [lp2025/2026-09-11-0911-tab-emulator-loose-ends/w4-reset-replay-fix.md, lp2025/2026-09-10-1707-c6-emulator-in-tab/G1-handoff.md]
---
# A reset inside a slice replays the board's whole guest lifetime, silently

**Symptom** — `just walk-no-board --tab` (the C6-in-tab plan's G1) stopped at
the flash step: `esptool.js` never got its SYNC answered and reported `Failed
to connect with the device`, after the board had been up through the
identify flow and the picker for around thirty minutes of wall time. A
sibling investigation (#710, W3) reproduced it on demand: it did **not**
reproduce on a young board (esptool connected in 11.5 s) and reproduced every
time on an old one.

**Root cause** — `Esp32C6Machine::run_until` (`lp-emu/esp/lp-emu-esp32c6/src/machine.rs:4410`)
fixes `stop_cycle` as an **absolute** guest cycle once, before its loop
(`:4412`: `let stop_cycle = stop.stop_cycle.unwrap_or(u64::MAX);`). When the
guest's reset dance lands inside that slice, `MachineRequest::Reset` →
`reboot()` → `restore(power_on)` zeroes the clock, and the loop `continue`s
against the *old*, now-stale `stop_cycle`. So the one `emu_run` call that
carries a reboot runs the guest from power-on all the way to
`old_cycles + budget` — the machine replays its entire prior lifetime inside
a single slice, and the host (the Worker, in the tab; `lp-cli emu serve`'s
door, natively) is frozen for the duration.

Measured (#710, node, the shim driven directly — `emu.control("download-mode")`
then one 40 ms slice) on a blank rom-up board:

| board age (guest) | the slice that carries the reboot | cycles after |
|---|---|---|
| 0.5 s | 4.6 s of wall | `old + budget` |
| 2 s | 11.5 s of wall | `old + budget` |
| 8 s | 46 s of wall | `old + budget` |

This is the same signature `machine.rs`'s own comment already records for
the native door ("a board 33 s of guest time old went silent for ~6 s of
wall … one 120 s old for ~3.5 minutes") — that fix zeroed the host-poll
deadlines on reboot, but the replay term above survives it. Natively the
cost is board-age ÷ (native speed) and is a nuisance; at ~0.3× dilation in a
tab it is board-age × ~6, which is fatal to any request with a bounded
deadline — `emulator_tab_bridge.js`'s 30 s `REQUEST_DEADLINE_MS` among them.

**Who it hits** — every reset: esptool-js's classic reset dance (the G1
failure above); mode A's Flash and Erase verbs, which both reset after their
direct write (D5/D24) — on a board that has been up for minutes, both freeze
the worker for minutes; `lp-cli emu serve`'s door. Two consequences are
masked by it: the worker's own "micros went backwards" reboot detection and
`EmulatorPort`'s `cyc`-goes-backwards detection never fire on this path,
because the replay keeps the clock monotonic throughout.

**Fix** — landed 2026-09-13, PR #737 (merge `4940c2b53`), the escalated brief
(E4) dispatched: `stop_cycle` becomes `mut`; the reset arm reads what is
**left** of the budget while the old clock origin still stands, then rebases
the bound onto the new origin after a successful `reboot()`. `old + budget`
→ `budget`; no `emu_*` ABI number was added. Measured on the shipping CLI
binary: a `--usb-script "100 reset"` slice that used to stop after
16 160 000 cycles now stops after 160 000.

**C6 only — the classic has the same shape and is NOT fixed.**
`lp-emu/esp/lp-emu-esp32v3/src/machine.rs`'s `Machine::run_until` (`:2717`,
`let stop_cycle = stop.stop_cycle.unwrap_or(u64::MAX);`) is never rebased
after `Machine::reboot`'s `self.restore(&power_on)` zeroes the clock — the
identical defect, unfixed. #737 reported it to the Xtensa director rather
than fixing it there; it is not this entry's fix and not closed by it.

**Regression coverage** — `lp-emu-esp32c6`'s
`machine::tests::a_reboot_inside_a_slice_consumes_the_budget_and_not_the_boards_lifetime`
ages a board 100 ms of guest time, resets it a quarter of the way into a 1 ms
slice, and asserts the slice consumes `≈ budget` (120 000 cycles — the
100 ms already spent — 40 000, subtracted from the 160 000-cycle budget),
never `old + budget` (16 160 000).

**Lesson** — an absolute stop bound computed before a loop starts is only
safe if nothing inside the loop can change what "zero" means. A reboot
changes the clock's origin; any code that reads a "cycles remaining" fact by
subtracting from an absolute target computed before the reboot is reading a
stale target, and the wrongness scales with exactly the quantity nobody was
watching — how old the thing being reset already was.
