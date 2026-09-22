---
status: open
found: 2026-09-22      # how: live-debugging, writing the P3 reset-domain model
area: lp-emu/esp/lp-emu-esp32c6 machine.rs (Esp32C6Machine::restore_in)
class: fidelity
related:
  - lp2025/2026-09-22-0015-c6-lp-domain-reset/
  - docs/adr/2026-09-22-emulator-power-on-versus-reset.md
  - docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md
---
# An emulated reset restores every memory region, so `.rtc_fast.persistent` does not survive one — though the ROM proves silicon keeps it

**Symptom** — no board and no failing transcript; found reading
`Esp32C6Machine::restore_in` while building the reset-domain model
(`c6-lp-domain-reset` P3). A `reboot()` restores *every* guest memory
region from the power-on snapshot, `.rtc_fast.persistent` — the LP
island's own RAM, the region an app uses to carry state across a reset —
included. On silicon, an HP-only reset does not reach LP SRAM at all, and
the ROM's own `__pre_init` only zeroes `.rtc_fast.persistent` when the
reset cause it reads is `POWERON` (`rtc_get_reset_reason() == 1`) — a
conditional zero that would be dead code if the region did not otherwise
survive every other kind of reset. The emulator zeroes it (by restoring
the power-on image) on **every** reset, `POWERON` or not.

**Root cause** — the reset-domain model this plan built
(`Peripheral::domain()`, `Domain::{Hp, Lp}`) filters which *peripherals'*
register state a reboot restores, and ten blocks now correctly answer
`Lp` and survive one. Memory has no equivalent: `restore_in` restores
every region unconditionally, because a `Domain` is a property `impl
Peripheral for RegFile` (and friends) can carry and a memory region is
not a `Peripheral` at all — it is `SocBus::restore_regions`, called
before the peripheral restore and with no filter parameter. The LP
domain's boundary, real on hardware and now real for peripherals in this
emulator, simply has no memory-side counterpart to be filtered by.

A second, smaller gap the same code review found: **the scheduler**.
`restore_in` also restores `s.sched` wholesale, so a restored run's
schedule is the power-on schedule. An LP peripheral with an event
outstanding at the moment of a reboot would keep its registers (correct,
now that it is `Domain::Lp`) but lose the event that was going to fire
them (not modelled either way). This is harmless *today* — none of the
ten `Domain::Lp` blocks schedules anything: the eight accept blocks
cannot, and the two modelled ones (`LpPeri`, `LpI2cAnaMst`) apply their
one stateful edge (the reset-line pulse) lazily on the block's next
access rather than through the scheduler — but the same shape of gap.

**Fix** — none yet; filed `status: open`. The shape a fix would take:
give a memory region the same domain declaration a peripheral has (a
region descriptor carrying `Domain`, `restore_regions` gaining an
`Option<Domain>` filter the way `restore_peripherals_in` already has),
and restore `.rtc_fast.persistent` from `s` only when the domain filter
is `None` or `Some(Hp)`. The scheduler gap would need the reverse: an
LP-scheduled event surviving a reboot's `sched.restore`, which no
peripheral needs yet.

**Regression coverage** — none, and none is possible today without
moving a byte-identical transcript: the plan's own inviolable invariant
("a clean board's recorded behaviour does not change") means no
committed, transcript-graded run ever reboots, so nothing exercises this
path where a wrong answer would show up as a diff. `tests/bootloader_hang.rs`'s
reboot and power-cycle tests never write `.rtc_fast.persistent` either —
they exercise `LP_PERI`'s registers, not LP SRAM.

**Lesson** — a domain model that reaches peripherals and stops there is
honest about peripherals and silent about everything else a reset can
touch. `notes.md`'s D4 scoped memory *out* deliberately (the "conservative
first cut" — fewer ways to move a clean board's transcript), which was
the right call for the plan's own scope, but conservative-and-undocumented
reads as finished; this entry is the naming. The general shape — a new
axis of "what a reset preserves" gets modelled for the one kind of thing
(peripherals) a plan is scoped to touch, and silently does not extend to
the other kinds of state (memory, the scheduler) a real reset boundary
also respects — is worth watching for if the domain model grows a third
consumer.
