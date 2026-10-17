---
status: retired
since: 2026-09-06      # the C6 emulator's SPI flash engine, from its first model
logged: 2026-10-10
retired: 2026-10-17    # the `t4` time grade (C6); the S3 and the classic still carry it
area: lp-emu/esp/lp-emu-esp-common engine/spi_flash.rs (every emulated chip's NOR)
related:
  - lp2025/2026-10-08-2339-tree-store-firmware-and-emulator (Q17)
  - docs/reports/2026-10-10-tree-store-emulator-walk.md
  - docs/adr/2026-09-10-the-emulator-first-device-walk.md
---
# Emulated flash commands take no time

**Shape** — the emulated SPI NOR executes every program and erase at once:
no write-in-progress busy time, no 4 KiB erase's tens of milliseconds, no
page program's fraction of one. An emulated filesystem's commit, mount or
format time is therefore its **CPU** time only, on any time grade. Adding WIP
busy time is a new time-grade rung: it would move every pinned emulator
figure, so it needs its own plan.

**Carrying cost** — every storage figure quoted from the emulator needs a
second, separately labelled silicon estimate beside it, and the two can
disagree by orders of magnitude: the tree store's first-boot format is
68 ms of emulated CPU and ≈ 3.7 s of erases on silicon (176 × ≈ 21 ms), each
erase inside esp-storage's critical section — invisible here. A walk cannot
see a watchdog or a missed frame that flash time would cause.

**Workarounds** — quote emulated time as CPU only, with the flash commands
counted (`FlashOpCensus`: programs, program bytes, erases; `FlashCensus`:
SPI1 reads) and an estimate (erase ≈ 21 ms, program ≈ 0.5 ms per 256-B page,
labelled ESTIMATE); `lp-cli/tests/support/tree_store_measure.rs` does both.
Any flash-time claim is a silicon claim (M7, CX1).

**Incident log**
- 2026-10-10 — the tree-store emulator walk (M5) reports mount, commit and
  format as CPU-only emulated microseconds with command counts and a
  labelled silicon estimate; D4's ≈ 3.7 s format is an estimate, to be
  measured on CX1.
- 2026-10-17 — paid down on the C6: `lp-emu:esp32c6:t4` (plan
  `lp2025/2026-10-16-0848-tree-store-adoption`, `emu-flash-timing.md`).
  Format 3,957 ms emulated against CX1's 3,884–3,917 (1.01–1.02), mount at
  0.94–1.11 of silicon across four firmwares (empty stores aside), the flash-tears write in 340 µs against
  318–353; `docs/reports/2026-10-17-c6-flash-timing-emulator.md`.

**Exit criteria** — an opt-in time-grade rung that bills program/erase busy
time from the part's datasheet (or M4's measurements), with the pinned
figures re-blessed under it, so an emulated commit time includes its flash.
**Met for the C6, 2026-10-17:** `t4` is that rung, opt-in by grade, with
CX1's measured busy times (`NorTiming::CX1`). No pinned figure runs at `t4`,
so none needed re-blessing: `t1`–`t3`, which every pinned figure runs at, did
not move. A flash-time claim may now be quoted from `t4` with its
configuration name, as a `modeled` figure — never gated. The S3 and the
classic share the engine and still pass no timing; their storage figures
still need this entry's old workaround (CPU only, plus a labelled estimate).
