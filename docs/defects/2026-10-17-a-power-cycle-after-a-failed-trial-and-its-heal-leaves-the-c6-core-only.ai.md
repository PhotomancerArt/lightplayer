---
status: fixed
found: 2026-10-17      # CX1 (c6-expendable, 14:C1:9F:E6:54:90) and CX2 (c6-button, E4:B3:23:AE:80:98), the C6 cutover sitting (#1179)
fixed: 2026-10-17      # lp_bootctl::booted_by, read by fw-esp32c6 ota/boot_state.rs
area: lp-bootctl boot_choice (cold retry of a failed trial) × fw-esp32c6 ota/boot_state.rs (trust) × the split layout's engine placement
class: untested-path
related:
  - 2026-10-04-c6-split-link-firmware-loader-and-boot-records   # ADR: records, choose, D9 cold retries
  - 2026-10-06-ota-update-protocol                              # ADR: rollback, heal
---
# A power cycle after a failed trial and its heal leaves the C6 core-only

**Symptom.** On both desk C6s, on the cutover build (and the same through the
ESP-IDF door, below): OTA X → Y (`UpToDate`), Y → a core that dies on trial
(`fixture-trial-dies`: `[LOADER] core @0x18000 (trial)`, the panic, then
`[LOADER] core @0x218000 (rolled back)`, the host told `RefusedBuild`), Y
offered again heals the engine (`UpToDate`, `refusedBuild` named), the board
runs its project. Then **one power cycle**:

```
[LOADER] core @0x18000 skipped: no image there
[LOADER] core @0x218000 (fallback)
[CORE] core @0x218000 … (fallback) … engine not started (trial)
[CORE] boot state not trusted (the boot records disagree with the MMU about where this core runs) — nothing is written this boot
[OTA] core-only: on trial, waiting for a host to confirm
[OTA] refused: program 0x16020
[OTA] core-only: needs-engine
```

No project runs, the update light says needs-engine, and every later power
cycle does the same. A warm reset still rolls back properly (and runs the
project) until the next power cycle.

**Mechanism.** After the rollback, record 0 still names the dead trial
(`seq 3`, trial, core `0x18000`), attempted but never `started`. On a
**cold** reset `lp_bootctl::failed` does not call such a trial failed until
`COLD_RETRY_CAP` cold retries (D9: a power cut must not roll a trial back),
so `choose` picks it again. But the heal put Y's engine where the dead core
was: with the core high (`0x218000`) the engine sits at the low end, so
`0x18000` now holds the engine header (`LPEH`). The loader skips it ("no
image there") and falls back to Y's core. Y's core recomputes `choose`,
finds the record it was "chosen" by is not where the MMU says it runs, so it
does not trust the boot state: it writes nothing (so no cold retry is ever
tallied and the state never ends), believes itself on trial, and does not
start its engine.

**Not caused by the cutover.** `boot_choice.rs`, the core's trust check and
the layout are unchanged from `main`; the same flash, with the ESP-IDF
bootloader at `0x0` instead of ours, boots the same way on the emulator
(`lp-emu:esp32c6:t1`, `lp-cli emu run --merged <flash> --flash-part cx1`):
`[LOADER] core @0x18000 skipped: no image there` → `core @0x218000
(fallback)` through both doors. No scenario power-cycles after a heal: U12
heals and stops; S3 rolls back warm.

**Evidence.** CX1's flash in the stuck state:
`results-cutover/cx1/cx1-after-ota-stuck.bin` (CX2's beside it) in the
planning directory `lp2025/2026-10-16-0848-tree-store-adoption`; the
emulator repro images (both doors) in `~/.photomancer/cutover-img/repro/`.
Both boards were put back with a plain USB flash of the package (record 0
proven, record 1 erased), files kept.

**Fix (no persisted byte changes meaning).** Of the three candidates, (b):
the core reads the fallback as the rollback it is. `lp_bootctl::booted_by`
takes `choose`'s pick and where the MMU says the core runs; when the pick is
an unconfirmed trial whose core is not the one running, and the other record
is proven and names the running core, the boot is the other record's,
`rolled_back`. `BootState::read` uses it instead of `choose`. The core then
trusts its state, refuses the dead build (`refusedBuild`), starts its
engine, and the next update's trial record goes into the dead trial's sector
(the one the running core's record is not in), retiring it.

Why (b) and not the others: it writes nothing and changes no record, mark
or loader byte (the loader still picks the dead trial cold and skips it with
a reason); it covers a trial that died warm **after** `started` too, which
(a) cannot (a started trial is never failed by a cold boot, whatever the
tally); and it keeps the refused build, which (c)'s retiring the record
would forget. Any other disagreement with the MMU (a proven pick that will
not load, a core neither record names) is still untrusted. The ADR's
2026-10-17 amendment "a fallback from a dead trial is a rollback" records it.

**Tests.** `lp-bootctl` unit tests (`booted_by` against `choose`, the
fallback rule, the cases left untrusted); `emu_split_scenarios` S12 (the
state constructed: a dead trial's record naming X's engine, two power
cycles); `emu_ota` U19 (X → Y, Y → Y-dies rolled back, Y healed, two power
cycles: Y runs its engine and still refuses Y-dies). Both failed on `main`
(`boot state not trusted`, `engine not started (trial)`) and pass with the
fix, through both doors.

**Silicon (CX1, c6-expendable, 14:C1:9F:E6:54:90).** The same walk on
`main`'s images and then on the fix's, each from a USB flash of X with
`projects/test/basic` uploaded: X → Y `UpToDate`, Y → Y-dies
`RefusedBuild`, Y again `UpToDate` (project running, `refusedBuild` named),
then three `board power-cycle`s. On `main` every cycle came up `(fallback)`
· `engine not started (trial)` · `boot state not trusted` · `core-only: on
trial`. With the fix every cycle came up `(rolled back)` with its engine,
`[CORE] rolled back: the newer core (build 0x433c544c) failed its trial`,
the project's output opened, and the hello said `running` with
`refusedBuild` 1128027212 (Y-dies).
