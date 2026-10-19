---
status: fixed
found: 2026-10-19      # reading #1181's fix: the window it left (no board, no walk)
fixed: 2026-10-19      # lpc-update retire_spare_record (write order) + lp_bootctl::stray_trial (reader)
area: lpc-update piece_stage (core stage write order) × fw-esp32c6 ota/update_target_impl.rs × lp-bootctl boot_choice (cold retry of a dead trial)
class: untested-path
related:
  - 2026-10-17-a-power-cycle-after-a-failed-trial-and-its-heal-leaves-the-c6-core-only
  - 2026-10-04-c6-split-link-firmware-loader-and-boot-records   # ADR: records, choose, D9 cold retries
  - 2026-10-06-ota-update-protocol                              # ADR: the core stage
---
# A cut before the next core's record boots it under the dead trial's

**Symptom (emulated, `lp-emu:esp32c6:t1`, both doors).** `emu_ota` U20 on `main`'s images,
our door: U19's state (X → Y, Y → Y-dies rolled back warm, Y healed), then
X offered. X's core goes to `0x18000`, where the dead trial's record (sector
0, `+0x160df0`) points. A clean power cut at the core stage's last program
command, then a cold boot:

```
[LOADER] core @0x18000 (trial)
[CORE] core @0x18000 +1445360 (trial) build a0a0a0a0+… · engine not started (trial)
[CORE] trial cold retry counted: the last boot died before it started
[OTA] core-only: on trial, waiting for a host to confirm
[OTA] core confirmed
[OTA] core-only: needs-engine
```

X ran as Y-dies' trial: Y-dies' build hash, Y-dies' length (the board
reported `+1445360` from the record, not X's own) and Y-dies' marks — and the
first host to connect **confirmed** it, making the dead build's record
proven over X's bytes. No board reached this state; it was found by reading
#1181's fix, and reproduced on the emulator only.

**Mechanism.** After a warm rollback the newer boot record is still the
dead trial's: unconfirmed, attempted, never started, naming the end of the
region the dead core sat in. On a **cold** boot `choose` picks it again
(a power cut is a retry until `COLD_RETRY_CAP`; a trial that had `started`
is never failed cold at all). The next update writes its new core into that
same end — the end the running core is not in — and the core stage wrote
the new core's **record last**: erase and program the spare record sector
only after the whole core hashed. Until then the dead trial's record names
the new core's bytes. A power cut anywhere in that core's write, or between
its hash and its record, and the next cold boot loads the new core (or a
partial one) as the dead build's trial: the dead build's hash, length and
marks. That core trusts the state (the record names where it runs), runs on
trial core-only, and its engine room, write fence and refused build are
worked out from the dead build's record; a host that confirmed it would make
that record proven.

**Fix (no persisted byte changes meaning; channel 3 unchanged).** Both
halves, because fielded cores still write a core's record last when they
install this one:

1. **Write order** (`lpc-update`): the core stage erases the spare record
   sector (`UpdateTarget::retire_spare_record`, the sector the trial record
   goes into) on every start and resume, before the first byte of the core.
   From then until the trial record, the running core's record is the only
   one: a cut boots the running core, which resumes the transfer. The cost:
   a refused build named by a rollback is forgotten if the board power-cycles
   mid-transfer (the transfer's own trial record replaced it anyway).
2. **Reader** (`lp_bootctl::stray_trial`, read by `fw-esp32c6`
   `ota::begin`): a core that finds itself under an unconfirmed trial record
   of **another build**, with a proven record behind it, marks the trial
   attempted (as any trial core does first) and resets warm. The loader
   rolls back to the proven core — which the update never touched — and it
   names the dead build refused, as before the cut. It writes no new byte
   and changes no mark's meaning; a stray with nothing proven behind it is
   still booted.

**Tests.** `lpc-update` `board_transfer`
`a_core_install_after_a_rollback_retires_the_dead_trials_record_before_its_first_byte`
(the NOR model, a cut after every flash op of the install: the dead record
never names bytes the new core changed; fails on `main` at the first core
chunk). `lp-bootctl` `stray_trial` unit tests. `emu_ota` U20 (U19's state,
then X offered: clean cuts across X's core stage and at every write to the
dead record's sector, each followed by a cold boot that must run a core
under its own record, then a host that must finish X), through both doors.
**Results** (`lp-emu:esp32c6:t1`, through both doors, ours and the IDF's;
eight cut points: X's core stage at its first two erases, its middle and
its last program command, and the first four writes to the dead record's
sector):

| images | installer (Y) | new core (X) | U20 |
|---|---|---|---|
| `main` | records last | no guard | **fails**: X runs as Y-dies' trial (above) |
| fixed | retires first | guard | passes; every cut boots Y under its own record (`rolled back` before the retire — the hand-over's header erase — `proven` after) |
| mixed | `main` (records last) | guard | passes; at the last core program and the record erase X boots, prints `runs under build 0x…'s trial record, not its own … resetting to roll back`, and Y comes up `(rolled back)`, trusted, Y-dies refused |

Every run then finishes X from a host offering it. The rest of `emu_ota`
(U1–U19) and `emu_split_scenarios` (S1–S12) pass through both doors on the
fixed images. No silicon: the change is the order of two flash operations
the emulator models command by command, and a reset the trial path already
takes.

