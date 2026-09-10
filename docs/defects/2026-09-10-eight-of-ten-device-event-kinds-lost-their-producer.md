---
status: OPEN — found by the emulated scenario lane; no lane can satisfy the affected `expect` matchers
found: 2026-09-10      # emulator plan two, M6 (the walk with no board)
area: lp-app/lpa-studio-core/src/app/studio/studio_controller.rs, scripts/device-scenarios, lp-app/lpa-link/tests/trace_replay.rs
class: instrument-rot
related: [lp2025/2026-09-08-0838-emulator-plan-two-web-serial-shim/m6-walk-with-no-board.md, docs/defects/2026-08-02-serial-line-interleaving.md]
---
# Eight of the ten device-event kinds have no producer, so the golden-trace library pins nothing a capture today can reproduce

**Symptom** — an emulated run of `s1-blank-flash` walks correctly end to end
(the card reads *Blank flash — needs firmware*, the door agrees, the boot
lines are all there) and then fails its own `expect` list, because no
`state:blank-flash` record ever reaches the trace. So do `s2`, `s3`, `s7` and
`s8`. The capture is not empty — it is 212 records — but every one of them is
a `journal`.

**Cause** — `DeviceEventKind` has ten variants and the module doc calls the
JSONL shape a CONTRACT. Two of them still have a producer:

```
$ grep -rn "DeviceEventKind::" --include "*.rs" . | grep -v device_event_log.rs
lp-app/lpa-studio-core/src/app/studio/studio_controller.rs:775:   DeviceEventKind::Journal {
lp-app/lpa-studio-core/src/app/studio/studio_controller.rs:3648:  DeviceEventKind::Pool {
lp-app/lpa-studio-core/src/app/studio/studio_controller.rs:3843:  DeviceEventKind::Pool {
```

`State`, `Flow`, `Rx`, `Tx`, `Mgmt`, `Sweep`, `Sync` and `Anomaly` are emitted
by nothing, anywhere in the repo. They went in `0a1b51d13` (2026-08-25,
*"refactor!: tear down the legacy device system to an honest stub (M2)"*),
which replaced the typed lifecycle events with one `journal` stream from the
new device model's flight recorder. `Flow`'s doc comment already records half
of this — *"Producer-less since M2 of the device-model rebuild"* — and the
change was honest about the kind it named; the other seven went unremarked.

**Every committed fixture predates that.** The nine `.jsonl` files under
`lp-app/lpa-link/testdata/device-traces/` were captured on **2026-08-03**
(`s1`'s first record stamps at `1785789064.05`), three weeks earlier. So:

| | then (the fixtures) | now (any capture, any lane) |
|---|---|---|
| `state`, `flow`, `mgmt`, `sweep`, `sync`, `anomaly` | present | never emitted |
| `rx` / `tx` (capture mode) | present — `s1` carries 88 raw boot lines | never emitted; `?capture-sink=` still turns capture mode ON, and it gates a producer that is gone |
| `pool` | present | present |
| `journal` | absent | the whole trace |

**What it costs, in three places:**

1. **The scenario library's `expect` matchers are unsatisfiable.** Five of the
   six scenarios that have a silicon fixture name `state:` or `flow:`. Nothing
   can pass them — not the emulator, and **not a board**: the producer is in
   `lpa-studio-core`, above the link layer, so the lane makes no difference.
   Any hardware capture sitting held today would fail exactly as the emulated
   lane does.
2. **`trace_replay.rs` is quietly becoming vacuous.** It replays each
   fixture's `rx` lines through `BootLineClassifier` and asserts the recorded
   no-firmware verdict still reproduces. It passes today only because the
   *old* fixtures still contain `rx`. A fixture captured now has none, so it
   is replayed with zero lines, records no state, and asserts nothing — a
   green test over an empty file.
3. **The instrument's own purpose is gone.** The log exists because "jank a
   refresh fixed" left no evidence, and `anomaly` counts specifically because
   they distinguish *"disconnected after garbled input"* from a clean drop
   (`docs/defects/2026-08-02-serial-line-interleaving.md`). The counts are
   documented as maintained *regardless of capture mode* — and are now always
   zero.

**What is NOT wrong** — the record shape, the capture-mode gate, the sink, the
`?capture-sink=` reader and the runner are all intact and were exercised end
to end while finding this. The information has not been lost either: the raw
boot lines are still in the trace, wrapped one level down inside journal
entries as `Input(Event(Link { link: LinkId(9), event: Line("…") }))`, and the
classifier's verdict is there as `Note(ActivityEnded { kind: Identify, outcome:
Succeeded { summary: "blank or erased flash" } })`. What is missing is the
typed, matchable, machine-readable form the fixtures and the replay test are
written against.

**The fix is not this milestone's.** Re-deriving `state`/`flow`/`sync` from
the new device model, and re-feeding `rx`/`tx`/`anomaly` from the link layer,
is Studio product work with its own gate — M6 deliberately did not do it,
because the alternative on offer was loosening the matchers, and a matcher
loosened to make a lane pass is the exact failure the scenario library exists
to prevent. The matchers are unchanged. The emulated captures are filed as
FINDINGS (`<id>.emu.failed.jsonl`), which is what the runner does with a run
that happened but did not do what its spec expects.
