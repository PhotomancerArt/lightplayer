# The S3's update checks on silicon: rollback, a crashing engine, reset codes, boot records (S3 updates plan, M7 P11)

2026-10-25, 11:56–12:12 PDT. Agent-run (session `s3-update-checks`), no hands
needed. The other half of M7 P11; the power cuts are
[`2026-10-24-s3-power-cuts-desk-walk.md`](2026-10-24-s3-power-cuts-desk-walk.md).

- **Board:** FS3 `fixture-s3`, XIAO ESP32-S3 Plus (16 MB), MAC
  `D8:3B:DA:47:29:70`, antenna attached, on the desk Mac's VIA hub `1-1.2.3`
  port 3. Configuration `silicon:esp32s3`.
- **Images:** four split images built by `scripts/ota/build-image.sh --chip
  esp32s3` with the shipped features (`esp32s3,server,float-f32,fs-tree`):
  X `a0a0a0a0+5e29587b24a9`, Y `b1b1b1b1+5e29587b24a9`, Y-dies
  `c2c2c2c2+5e29587b24a9` (`+fixture-trial-dies`: its core panics on trial
  before it marks itself started) and Y-engine-dies `d3d3d3d3+3454e4197ea2`
  (`+fixture-engine-dies`, new in this change: its engine panics at its entry
  on every boot). None of them is in a build def.
- **Host:** `lp-cli link capture <port> --ota-offer <dir>` over USB, the host
  Studio's update card drives too. Known-good was release `2026.10.25-1`
  (`lp-cli firmware install --release latest`), first and last. `lpfs` was
  backed up first and never written; Yona's "Skirt 620 five" ran on every
  engine that started.

## Verdicts

| Check | Verdict |
|---|---|
| A core that dies on trial rolls back by itself and is refused | **Pass** |
| An engine that keeps crashing leaves a reachable board, and a good build installs over it | **Pass** (emulated first, then silicon; same lines) |
| Reset codes: requested reboot, panic, update resets, host reset | **Pass** (power cut: the 2026-10-24 walk) |
| Boot records read back raw agree with the loader and the core | **Pass** |
| Q-S3, the bootloader on the board | **Answered**: espflash 3.3's bundled IDF bootloader, byte-identical to the package |
| The 1.5 MB flash read that came back corrupt | **Host tool**, not the board (defect filed, `wontfix`) |

## A core that dies on trial (Y → Y-dies)

One offer, no cable, 16.5 s:

```text
[LOADER] core @0x20000 (trial)
panicked at lp-fw/fw-esp32s3/src/split.rs:106:9:
fixture-trial-dies: dying on trial before started
rst:0x3 (RTC_SW_SYS_RST),boot:0x8 (SPI_FAST_FLASH_BOOT)
[LOADER] core @0x500000 (rolled back)
[RECOVERY] boot: cause=software-reset level=yellow safe_mode=false prior_boot_complete=false
[RECOVERY] last run crashed (panic): at boot: fixture-trial-dies: dying on trial before started (at lp-fw/fw-esp32s3/src/split.rs:106)
[CORE] rolled back: the newer core (build 0x79b5537c) failed its trial
[host-ota] board: b1b1b1b1+5e29587b24a9 NeedsEngine (refuses build 0x79b5537c)
[host-ota] done: Stopped(Decision(RefusedBuild { build: 2041926524 }))
```

Y's engine header had been erased at the hand-over to core-only, as the
design says, so the rolled-back Y came up `NeedsEngine`. A host holding Y's
engine healed it (`--ota-heal-only`, 19.3 s), and Y ran its project again,
still refusing the failed build (`refusedBuild: 2041926524`). Its heartbeat
then read `lastCrash: {cause: panic, path: boot, bootsAgo: 2}`, level yellow.

## An engine that keeps crashing (Y → Y-engine-dies)

The core installed and was confirmed, then the engine went in (36.3 s in
all). Then the engine panicked on four boots in a row, and the core stopped
entering it:

```text
[LOADER] core @0x20000 (proven)
panicked at lp-fw/fw-esp32s3/src/main.rs:977:9:        ×4, each followed by rst:0x3
[RECOVERY] boot: cause=software-reset level=red safe_mode=true prior_boot_complete=false
[CORE] core @0x20000 +1083536 (proven) build d3d3d3d3+3454e4197ea2 · … · engine keeps crashing (4 incomplete boots)
[OTA] core-only: engine keeps crashing (4 incomplete boots) — not starting it
[host-ota] board: d3d3d3d3+3454e4197ea2 EngineCrashing
[host-ota] done: Stopped(Decision(ReportCrashing { build_id: "d3d3d3d3+3454e4197ea2" }))
```

That is the design: there is no older engine to roll back to (the update
replaced it), so the board stays reachable, core-only, and the host reports
it rather than healing it with the same engine. A host offering a good build
(Y) then installed it over the crashing board with no cable: backup, core on
trial, engine, proven, `UpToDate` in 32.8 s.

The emulator ran the same update first (`lp-cli emu run --chip esp32s3
--rom-up-flash <Y> --host-link --ota-offer <Y-engine-dies>`): four panics,
`engine keeps crashing (4 incomplete boots)`, `ReportCrashing`, the same lines
in the same order. The S3 scenario suite's U7 still skips (its harness
power-ons clear the RTC ledger); with this fixture it can run in one
continuous run, which is a follow-up.

## Reset codes (Q-S5)

| How the board reset | ROM line | `[RECOVERY] boot: cause=` / heartbeat `resetReason` | RTC boot count | `from_s3_reason` |
|---|---|---|---|---|
| Requested reboot (`--request reboot`) | `rst:0x3 (RTC_SW_SYS_RST)` | `software-reset` | kept (+1) | warm |
| Panic in a trial core | `rst:0x3` | `software-reset`, crash record `panic` | kept | warm |
| Panic in the engine (×4) | `rst:0x3` | `software-reset`, ledger yellow → red, safe mode | kept | warm |
| An update's own resets | `rst:0x3` | `software-reset` | kept | warm |
| Host reset (`espflash reset`) | not seen (before the port opens) | `user-reset` | kept (27 → 28) | cold (0x15/0x16) |
| Power cut (2026-10-24 walk) | — | `power-on` | restarts at 1 | cold |

All 16 `rst:` lines the captures caught read `0x3`. The USB-Serial-JTAG link
survives a software reset, so a capture open across a requested reboot sees
the whole boot from the ROM's banner. `espflash reset` holds the port itself,
so a host reset's ROM line goes out before the next capture opens it.

## Boot records read back (Q-S4)

Read raw over USB (`espflash read-flash --max-in-flight 1 0x16000 0x2000`):

| When | Record 0 (`0x16000`) | Record 1 (`0x17000`) | The loader and the core said |
|---|---|---|---|
| X flashed by USB, booted | seq 1, core `@0x20000` +1,083,536, proven, `confirmed` set (the engine guard) | erased | `core @0x20000 (proven)` |
| After X → Y | unchanged | seq 2, `@0x500000` +1,083,536, build `0x1f564190`, trial flag with attempted + confirmed + started | `core @0x500000 (proven)` |
| After Y → Y-dies | seq 3, `@0x20000` +1,083,648, build `0x79b5537c`, trial, attempted only | Y's, unchanged | `core @0x500000 (rolled back)`, refuses `0x79b5537c` |

Every CRC checks. The front of the chip read back after X's flash
(`0x0..0x20000`) equals the package's `merged.bin` everywhere except the four
bytes at `0x16020`, record 0's `confirmed` mark, which the core programs on
its first boot. No SPI1 or `RDID` trap showed up: the core writes records and
marks through `esp-storage` and the reads agree.

## The bootloader (Q-S3)

The board runs espflash 3.3's bundled ESP-IDF bootloader, the one the
emulator boots: its banner on FS3 is `ESP-IDF v5.1-beta1-378-gea5e0ff298-dirt
2nd stage bootloader`, `compile time Jun  7 2023 08:07:32`, `Multicore
bootloader`, and its bytes at `0x0..0x8000` equal the package's. The earlier
walk could not answer this because its captures opened after the boot text; a
capture open across a requested reboot catches it.

## Also seen

- **The large flash read was the host tool.** espflash 3.3's pipelined read
  (64 or 8 packets in flight) fails on FS3's 1.5 MB `lpfs`; one packet in
  flight reads it in 29 s, byte-identical to `lp-cli hardware lpfs save` (31 s,
  one read). `docs/defects/2026-10-25-espflash-pipelined-reads-fail-on-the-s3s-usb.md`.
- **After one `read-flash`, FS3 stayed silent** (no frame in 330 s) until
  `espflash reset`. Seen once; the sitting script now resets after each read.
- **The sitting script's reboot step could not pass**: its `--exit-on` matched
  the hello from before the reboot. Fixed, with an engine-dies step and an RTS
  check that reads `user-reset` instead of a ROM line it never sees.
- **No wedge.** FS3 never dropped off USB, so whether a `board power-cycle`
  wakes a wedged FS3 (the power-cut audit's row 19) is still open.
- **Timings, uncut, over USB:** X → Y 34.9 s; the rollback 16.5 s; a heal
  19.3 s; Y → Y-engine-dies to `ReportCrashing` 36.3 s; a good build over the
  crashing board 32.8 s. Skirt 620 five ran at 54–56 fps.

## Evidence

Every console, the record and front-of-chip reads, the `lpfs` backup and the
emulator run are in the planning workspace:
`lp2025/2026-10-20-2335-s3-radio-and-ota/m7/p11-checks/`.
