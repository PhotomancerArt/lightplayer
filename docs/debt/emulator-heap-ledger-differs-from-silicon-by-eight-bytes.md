---
status: carried
since: 2026-09-07
logged: 2026-09-08
area: lp-emu-esp32c6 boot + fw-esp32c6 allocator
related:
  - docs/adr/2026-09-06-esp-soc-emulator-architecture.md
  - docs/reports/2026-09-08-esp32c6-emulator-walk.md
  - lp-emu/transcripts/esp32c6/boot-idle-flash/
  - lp-emu/transcripts/esp32c6/upload-walk-usb/
  - lp-emu/transcripts/esp32c6/rom-up-boot/
---
# The emulated C6's idle heap is 8 bytes freer than silicon's, and nobody can say why

**Shape** — the ESP32-C6 emulator's memory claim is that the shipped image's
allocator reaches byte-identical figures on the emulator and on a board. It
does, on every figure but one: at every heartbeat of every capture, the
emulator reports `freeBytes` exactly **+8** and `usedBytes` exactly **−8**
against silicon. `totalBytes` and `largestFreeBlock` agree; the stack
high-water agrees; the filesystem's cost agrees to the byte on both sides.
Silicon has one live 8-byte allocation the emulator does not, and it is there
before the first heartbeat.

It is not a sampling artefact and not a loader artefact. The three candidates
the plan could test are all refuted:

| candidate | how it was refuted |
|---|---|
| a heartbeat sampled at different moments | M6 P4 keyed the series on the 5 s tick; the gap is identical at 5 s and 10 s |
| the board's boot history (`bootCount`) | M6 P5 measured it constant across boot 3 and boot 10, two commits, both links |
| the loader — an app placed rather than loaded | M7 booted ROM-up through the real mask ROM and IDF bootloader; the ledger is byte-identical to the direct load's, so the bootloader is not where it comes from. **Committed 2026-09-08**: `lp-emu/transcripts/esp32c6/rom-up-boot/lp-emu-esp32c6-t1-2026-09-08-735af98ae.txt`, held by `m7_replays.rs::the_eight_byte_gap_survives_the_rom_up_boot` — the refutation is a file now, not a run |

This is structural rather than a bug because the thing that would name it is a
measurement nobody has: a **power-on** capture. Every silicon transcript in
the tree was taken after a reset, and a reset is not a power-on — the RTC
domain, the ROM's own scratch, and anything the previous image left behind all
survive one. The emulator, by construction, always starts from power-on.

**Carrying cost** — small and constant, which is exactly why it is debt rather
than a defect. The heap gate reading from the emulator
(`just heap-budget-check-chips`) is a ratchet on the emulator's own figures,
so the gap does not make it flake; the walk record and the record file both
carry it in writing. What it costs is a caveat on every heap claim the
emulator makes — "byte-equal to silicon apart from a constant 8 B" — and the
half-hour each new reader spends satisfying themselves that the caveat is
bounded.

**Workarounds** — none needed. State the gap; never tune toward it. The
figures live beside each other in `scripts/heap-budget-record.json` under
`chips.esp32c6`, `measured` (this tree, from the emulator) next to
`silicon_reference` (the committed transcript, with its commit).

**Incident log**

- 2026-09-07 (M6 P5, PR #594) — the gap is measured constant across two boot
  counts, two commits and both links; `bootCount` refuted. Ruled DD50: a
  pinned, reported gap, not a gate.
- 2026-09-08 (M7, PR #596) — ROM-up boot is byte-identical to the direct
  load, so it is not the loader either. G7-4.
- 2026-09-08 (M8, PR #599) — carried into the heap gate's record as
  `silicon_reference`, printed on every chip check so it cannot widen unseen.

**Exit criteria** — a `bootCount 1` power-on capture of `boot-idle-flash` on
the desk C6 (power removed, not reset), recorded through the runner, showing
either that silicon's figures then match the emulator's — in which case the
8 B was a reset artefact and this entry retires — or that they do not, in
which case the 8 B is a real allocation and the next step is a heap-walk dump
behind a firmware feature. The capture needs Yona's hands and is on the desk
list; it is not blocking anything.
