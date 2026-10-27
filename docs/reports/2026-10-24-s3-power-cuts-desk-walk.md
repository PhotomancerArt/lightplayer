# The S3's first real power cuts during an update (S3 updates plan, M7 P11)

2026-10-24, 21:24–21:55 PDT. Agent-run (session `s3-power-cuts`), Yona at the
desk in case FS3 needed hands. It did not.

- **Board:** FS3 `fixture-s3`, XIAO ESP32-S3 Plus (16 MB), MAC
  `D8:3B:DA:47:29:70`, antenna attached, powered from USB on hub `1-1.2.3`
  port 3 and its USB 3 twin `1-2.2.3` port 3. Configuration `silicon:esp32s3`.
- **Cut:** `board power-cycle fixture-s3 --off-secs 2`, which since
  lp-board-bench#10 switches both twins of the VIA hub, so VBUS really drops.
- **Images:** two split images of `3a9fa2715`, built by
  `scripts/ota/build-image.sh --chip esp32s3` with the shipped feature set
  (`esp32s3,server,float-f32,fs-tree`): X `a0a0a0a0+3a9fa2715cf6`, Y
  `b1b1b1b1+3a9fa2715cf6`. Core 1,081,008 B, engine 2,095,237 B.
- **Driver:** `scripts/ota/hw-power-cut.py --chip esp32s3` (this PR's version).
  Each cut writes X's package at `0x0` (it ends at `0x330000`, well short of
  `lpfs` at `0x610000`), waits for X's heartbeat, starts `lp-cli link capture
  --ota-offer <Y>/ota`, cuts T seconds in, then runs a new host offering Y
  until it says `done`, then captures Y's hello and first heartbeat.
- **Project:** Yona's "Skirt 620 five" (five strips) stayed in `lpfs`
  throughout. FS3 started and ended on release `2026.10.24-6`, and `lpfs` was
  backed up first.

## The invariant held: 13 real cuts, 13 recovered, none needed a cable

After every cut, FS3 came back on USB by itself. A new host (the cable's
link reconnecting, no click and no flash) finished the update on Y with Y's
engine `running` and the project loaded.

| T (s) | where the cut landed | board's first state after | resumed at | proof the power really went | host reconnect → `UpToDate` |
|---:|---|---|---|---|---:|
| 0.5 | backup, before the hand-over | X `Running` (engine intact) | — (update restarted) | heartbeat `power-on`, uptime 10,033 ms; bootCount 3 → 4 on Y (no cut: ≥ 6) | 39.9 s |
| 3 | backup, before the hand-over | X `Running`, then core-only | — (core from 0) | bootCount 2 → 5 at the next X (no cut: 6) ¹ | 28.6 s |
| 4 | core | `Updating (Core …)` | core 188,416 / 1,081,008 (17 %) | bootCount 5 → 4 (no cut: ≥ 8) | 28.0 s |
| 8 | core | `Updating (Core …)` | core 557,056 (52 %) | bootCount 5 → 3 (no cut: ≥ 8) | 25.1 s |
| 12 | core, last sector | `Updating (Core …)` | core 1,060,864 (98 %) | bootCount 2 → 3 (no cut: ≥ 5) | 20.7 s |
| 11 | Y's core committed, on trial in core-only, no engine bytes yet | `OnTrial` → confirmed → `NeedsEngine` | — (engine from 0) | bootCount 4 → 3 (no cut: ≥ 7) | 20.2 s |
| 15 | engine | `Updating (Engine …)` | engine 131,072 / 2,095,237 (6 %) | bootCount 5 → 2 (no cut: ≥ 8) | 17.8 s |
| 18 | engine | `Updating (Engine …)` | engine 598,016 (29 %) | bootCount 4 → 2 (no cut: ≥ 7) | 14.3 s |
| 24 | engine | `Updating (Engine …)` | engine 1,314,816 (63 %) | bootCount 3 → 2 (no cut: ≥ 6) | 8.5 s |
| 30 | engine, last sectors | `Updating (Engine …)` | engine 2,031,616 (97 %) | bootCount 3 → 2 (no cut: ≥ 6) | 2.6 s |
| 34 | after `UpToDate` (Y confirmed) | Y `Running` | — | bootCount 3 → 1 | 0.0 s |
| 35.5 | after `UpToDate` | Y `Running` | — | bootCount 4 → 1 | 0.0 s |
| 36.5 | after `UpToDate` | Y `Running` | — | bootCount 2 → 1 | 0.0 s |

T is when the driver called `board power-cycle`. The switch lands about 3–4 s
later (the CLI's start plus uhubctl), so the "where" column comes from the
captures, not from T.

¹ Cut 3 ran on the script before this PR's change, which captured no heartbeat
on Y. Its proof is the next cut's pre-cut heartbeat: after a real cut the
count restarts and reads 5 there (X power-on, hand-over, Y trial, Y proven,
X reflashed); without one it would read 6.

**How the proof works.** A cut that only shows the board leaving and
rejoining USB proves nothing (a cut of one USB twin does that too:
`docs/defects/2026-10-23-board-power-cut-one-usb-twin-and-every-check-passed.md`).
The S3's heartbeat carries `recovery.bootCount`, kept in RTC memory, which only
a power loss clears. An update with no cut adds at least three boots to X's
count (the hand-over to core-only, Y's trial, Y proven). A real cut restarts
the count at 1, so Y reads the number of boots since the cut, at most four.
A cut that lands while the engine runs also gives a heartbeat reading
`resetReason: power-on`. Before the cuts, one plain `board power-cycle` read
`power-on`, uptime 5,014 ms, bootCount 38 → 1, which calibrates both readings.
The driver now refuses to call a cut PASS without one of them
(`NO-CUT-PROOF`).

**Not hit:** the window between the engine's commit and the host's confirm
(Y on trial with its engine). It is about 0.4 s wide on silicon, and the
switch's latency varies by seconds, so two tries (35.5 s, 36.5 s) both landed
just after `UpToDate`. The emulator's sweeps cover that window (PR #1262).

## The update on silicon, uncut

Over USB with no cut, X → Y took 36–42 s: backup and the hand-over to core-only
about 6 s, Y's core about 10 s (≈ 105 KB/s of image), its commit and reset,
then the engine about 20 s, its commit, trial and confirm. The running engine
accepts the core install by erasing its own engine header and resetting into
core-only (`lpc_update::board::running_engine::hand_over_to_core_only`), so a
cut any time after the hand-over boots X's proven core with no engine, and the
host heals it from there.

## The spike's five questions (only what this sitting shows)

- **Q-S1** (MMU writes and cache invalidation while running from flash): yes.
  X's engine and Y's engine, at its new place `@0x20000`, mapped and ran after
  every update (`engine 2095237 B`, `state: running`), including after a core
  that had just written the engine from core-only.
- **Q-S2** (fetches at MMU index ≥ `0x100`): the engines ran, which the plan
  reads as the answer. This sitting did not read MMU entries.
- **Q-S3** (the bootloader on the board): not answered. The ROM's and the IDF
  bootloader's text go out on USB-Serial-JTAG before any host opens the link,
  and `lp-cli link capture` never sees them.
- **Q-S4** (raw flash for records and marks): the core wrote them. Y went
  trial → proven in every run, and cut 11 left a trial core that kept its
  trial across the cut and was confirmed afterwards. No SPI1 or `RDID` trap
  showed up. The records were not read back raw this sitting.
- **Q-S5** (reset reasons): a power cut reads `power-on`, an espflash write or
  reset reads `user-reset`, and an update's own resets read `software-reset`.
  A requested reboot and a panic were not exercised here.

## Also seen

- **Large `espflash read-flash` on FS3 fails.** A 1.5 MB read of `lpfs` failed
  twice ("Corrupt data, expected 1000 bytes but received fdb"). 128 KiB pieces
  worked, two of twelve after one retry each.
- **The wedge question is still open** (power-cut audit, row 19): FS3 never
  dropped off USB during the sitting, so there was no wedge to try a real power
  cycle on.

## Evidence

Every capture (`cut-*.{pre,cut,rec,fin}.txt` and `.err`, `summary.json`), the
`lpfs` backup and the known-good captures are in the planning workspace:
`lp2025/2026-10-20-2335-s3-radio-and-ota/m7/p11-cuts/`.
