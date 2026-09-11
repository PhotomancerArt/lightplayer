---
status: open
found: 2026-09-11      # live-debugging, the tab emulator's loose-ends plan (#710, "W3 — where it stands")
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

**Fix** — not made here (this plan's P5 is docs/ADR/cleanup only, and the
file is `lp-emu/**`, which this plan's P1 is the only phase allowed to
touch — D27). A fix brief is drafted and **not yet dispatched**:
`lp2025/2026-09-11-0911-tab-emulator-loose-ends/w4-reset-replay-fix.md`
(escalation E4). Its shape: rebase `stop_cycle` to `now + remaining` when the
clock is zeroed by a reboot, so a reboot inside a slice consumes at most that
slice's remaining budget, never the board's prior lifetime again.

**Regression coverage** — none yet; the fix brief specifies a native test in
`machine.rs`'s test module (or `tests/tab_surface.rs`) that ages a board,
resets it inside a slice, and asserts the cycles consumed are `≈ budget`, not
`old + budget`.

**Lesson** — an absolute stop bound computed before a loop starts is only
safe if nothing inside the loop can change what "zero" means. A reboot
changes the clock's origin; any code that reads a "cycles remaining" fact by
subtracting from an absolute target computed before the reboot is reading a
stale target, and the wrongness scales with exactly the quantity nobody was
watching — how old the thing being reset already was.
