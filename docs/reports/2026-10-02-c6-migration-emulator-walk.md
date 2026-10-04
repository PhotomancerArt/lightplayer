# The C6 repartition's migration, walked on the emulated C6 (2026-10-02)

Plan `lp2025/2026-10-01-1843-c6-repartition`, phase P08. Everything here ran
on **`lp-emu:esp32c6:t1`** (non-strict: espflash's and esptool-js's stub reads
one block the C6 boot set does not map, `flash_over_socket.rs`) at
**`lp-emu` commit `6a417cdff`** (this branch before its rebase onto main
`1645a7f4d`; the same change is `d3b73107b` after it). Nothing here is
hardware-validated; durations are emulated seconds, never host time.

## The fielded board

Every scenario starts from a chip built the same way (`lp-cli hardware lpfs
fixture`, hidden): this tree's packaged C6 firmware
(`target/studio-web-assets/firmware/esp32c6-4mb`, 3,031,632 B, no padding)
at `0x0`, the **frozen pre-2026-10 table**
(`lp-app/lpa-link/testdata/partitions-esp32c6-legacy-v1.csv`) at `0x8000`,
and a 240-block littlefs at `0x310000` holding `projects/test/basic` and
`catalog/projects/playful-choker` under `/projects/`, the XIAO C6 board
manifest at `/hardware.json`, `/.lp/device.json` (uid
`dev0000000000000011`), `/.lp/access.json` with one browser key, and a
9,000-byte file (more than one block) — 23 files, 67,922 B. The current
firmware boots from it and mounts the old filesystem through the table
(P01), which is itself the first assertion every scenario makes. (Not in the
fixture: an empty directory — the fixture command packs files only.)

The pre-M3 CI image (a firmware that hardcodes `0x310000`) was **not** run:
NEVER RUN. The fixture board runs this tree's firmware on the old table.

## Step 0 — can the emulator be read? Yes

`lp-cli/tests/emu_layout_migration.rs::the_bootloader_reads_back_the_chip_byte_for_byte`:
`lp-cli hardware lpfs save` reads the table (3 KB) and the whole old
filesystem (960 KB) over espflash's stub, through a pty, from the emulated
download console — **byte-identical to the chip file** (99.35 s emulated).
The browser half is every walk scenario below: esptool-js's `readFlash`
inspects the board before Studio asks anything.

It found a real bug on the way: the host read pipelined 1024 packets while
espflash's per-packet ack clears the port's input buffer, losing the next
packet's head (`truncated at 8185 of 983040 bytes`) —
`docs/defects/2026-10-02-the-host-filesystem-read-throws-away-in-flight-packets.md`,
fixed (one packet in flight). And a pty has no modem lines: the host
provider now connects without the reset dance on one, and opens it without
a baud on macOS (`IOSSIOSPEED` is ENOTTY there); real ports are untouched.

## The Studio walk (`just walk-migration-emu <scenario>`)

Real Studio — the release bundle, served by the script on this worktree's
stable `dev-port.sh` slot — in headless Chrome, `?emu=ws://…` against
`lp-cli emu serve` holding the fixture board (`kind=rom-up`). Studio's words
are used only to know when to click; every claim is the board's (its hello's
`fs`/`deviceUid`, its `[INIT]`/`[FS]` lines in the door's console) or the
chip's (`lp-cli hardware lpfs report --image` of the chip file the door
wrote back, every file by SHA-256). Verdict JSON and screenshots per
scenario under `target/walk-migration-emu/<scenario>/`.

| ID | Scenario | Verdict | What the board / chip said |
|---|---|---|---|
| W1 | Update → the question → Continue | **pass** | question "Move this board's files to the new layout"; firmware installed; chip now `layout with files at 0x350000, 704 KB`; **all 23 files byte-identical** to what the update found; old superblock pair at `0x310000` erased; hello `fs: mounted`, `deviceUid: dev0000000000000011`; `Boot: found 2 entries in /projects` |
| W2 | Update on a board already on the new layout | **pass** | no question; installed; no file lost/moved/added; only `/.lp/access.json` changed (Studio's own key write on a USB connect — documented behaviour, not the migration) |
| W3 | over-full board (+720 KB of noise: fits 240 blocks, not 176 with the 16-block floor) | **pass** | refusal "This board's files don't fit the new firmware"; **chip byte-identical** to as found; board back on its firmware, files mounted |
| W4 | Cancel at the question | **pass** | chip byte-identical; board back on its firmware, `fs: mounted`, its uid |
| W5 | cable pull during the firmware write (`Writing at 0x10ab7e... (33%)`) | **pass** | the part-written app does not boot (`No bootable app partitions in the partition table`); the card reads "Unrecognized firmware" and offers **Flash firmware** with the board picked; that flash reads the layout again and asks "Move this board's files to the new layout" (23 files, read again from `0x310000`); Continue → **all 23 files byte-identical** at `0x350000`, old superblock erased, `fs: mounted`, same uid |
| W6 | cable pull between the firmware write and the filesystem | **not reachable** | the moment (after the firmware write, before the old filesystem's retirement) is one esptool-js step boundary; pulls keyed off the card's progress landed later every time. The held state is proven by W9 instead, and by the fake e2e (`interrupt_next_plan_after(1)`) |
| W7a | cable pull mid filesystem write (`Writing at 0x352000`) | **pass** | power-on boot: `[FS] Formatted and mounted fresh filesystem`; the card offers **Restore files** with the backup's date and Download backup |
| W7b / W8 | a NEW Chrome on the same profile (the tab was closed), same board | **pass** | the backup came back out of OPFS; Restore → "Put this board's files back" → installed; **all 23 files byte-identical** to what W7a's update found; `fs: mounted`, uid back |
| W9 | a bypassed flash (the new image written over `0x0`, no migration) | **pass** | boot: `[FS] legacy-layout filesystem found at 0x310000 — not formatting; files are held for migration; using memory FS`; the card offers **Finish update**; it moves **all 23 files byte-identical**; uid back |
| W10 | `lp-cli hardware lpfs migrate` (host path) | **pass** | `emu_layout_migration.rs`: every file byte for byte at `0x350000`, new table, old superblocks erased, the board mounts them on boot (179.28 s emulated) |
| W11 | preflight for an old-table image onto a migrated chip | **pass** | `emu_layout_migration.rs`: exit 3, chip unchanged |
| W12 | W1 on a tab-hosted board (`?emu=tab`, no door) | **pass** | the fixture chip is put into the page's board (`putFlash`, power-cycle) and read back out through the walk's own server; Studio's update asks, moves **all 23 files byte-identical** to `0x350000`, retires the old superblock; the board mounts them as itself |

**Re-walked after main's merge** (`6ee94d107`) and the move of the card's
layout verbs into the offer tree (then `devices/<id>/continue-update`,
now keyed by the board's MAC,
`cancel-update`, `download-backup`, `restore-files`, `finish-update`, as
main's core action-fields ratchet requires): W1, W3, W7a, W7b and W9
**pass** again on that head's release bundle and freshly packaged firmware
(3,031,696 B). The first W1 of that run failed one check, the hello's `fs`,
with every file moved: the walk scraped the hello from the console's RAW
lp-link bytes, and a frame boundary inside the value
(`"fs":"\x0cmounted<crc>j(…`) left printable header bytes glued to it. The
walk now reads a framed value as letters that must contain the state in
order. A harness bug, not the board's; the earlier passes were frame
boundaries that happened to fall elsewhere.

What the card says while it writes: its progress label reads "Flashing
firmware…" for the whole write, the filesystem included; "Moving files"
and `Writing at 0x35…` appear only in its terminal lines.

`walk-no-board` (and its `--tab`) on the new layout: **pass**, all six steps
each (flash → connect → identify → upload → detach → re-attach), run with the
new `--serve-release` (the walk serves the release bundle itself, no dev
server); `walk-esp32c6-emu` (the render walk) **passed** on the new layout,
byte-identical frames on all three readings (not re-run after the pre-G1
pass).

**Pre-G1 pass** (`d757d91d1`; firmware `fw-esp32c6 5f410f63c5d0`, whose
sources are unchanged since; release bundle rebuilt at that head): W1, W3,
W5, W7a, W7b, W9 and W12 **pass**; `walk-no-board` and `--tab` **pass**;
`test-emu-layout-migration` **pass** in three pieces (step 0 99.29 s
emulated, W10 179.17 s, W3-host 99.30 s + W11 0.41 s). W2 and W4 were not
re-run after their earlier passes. Two harness fixes came first: the walks
stopped the door with a SIGTERM, which skips its flash write-back, so the
chip they reported on was the last two-second snapshot (W7b read a stamp
half done that way); `stopDoor` now interrupts and waits. And the card's
layout verbs are offered at the board's MAC now,
`devices/<12 hex>/<verb>` (a link not yet identified is `devices/new-<n>`).

**Re-walked after the G1 rehearsal's fixes** (2026-10-03, `b3c0f0284`
plus the walk change below, main merged at `1059c5c51`; firmware
`fw-esp32c6 b3c0f0284cb0`, 3,036,704 B merged, release bundle rebuilt at
that head; `lp-emu:esp32c6:t1`): W1, W3, W4, W7a, W7b and W9 **pass**. W9
now also judges the held board: its console says `[ble] off (files held for
the layout change: the device store waits with them)` and never `[ble]
enabled`, and the card shows no "Who has access" for it
(`docs/defects/2026-10-03-a-held-board-runs-a-fresh-access-store-in-ram.md`).
Because Studio no longer writes access into a held board's RAM store, the
connect's ordinary key add lands on the REAL store after the move, so W9
accepts `/.lp/access.json` as the one changed file and leans on the card's
count (the fixture's entry plus this browser's, 2) for the old entries. W3's
refusal now reads in blocks: "This board's files take more than the new
layout's 176 blocks of 4 KB (an update must also keep 16 of them free)."

## Found and fixed during the walk

- **Emulator fidelity** — `power-cycle` after a USB download dance came back
  in the DOWNLOAD strap: `docs/defects/2026-10-02-the-emulated-c6-power-cycle-inherited-the-download-strap.md`.
- **Studio** — "Restore files" was offered only on the boot that formatted;
  a later reboot of the empty filesystem (`fs: mounted`, no uid) hid it while
  the backup was still pending. Now offered until the board names its
  identity again (W7b found it; never shipped).
- **Studio** — the card's "Restore files from backup (Oct 2)" overflowed its
  row beside Download backup; it reads "Restore files".
- **Fixed** — a stamp of `/hardware.json` cut between chunks left it
  truncated, on main too (every update stamps): the stamp is journaled
  through `/hardware.json.next` and the boot loader settles it —
  `docs/defects/2026-10-02-a-closed-tab-mid-stamp-leaves-hardware-json-truncated.md`.
  The emulated 6,144-byte sighting may have been the SIGTERM snapshot above.
- **Open** (found reading the tab path for W12, not walked) — a user's
  tab-hosted board updates by erasing the whole chip, files included, on
  main too: `docs/defects/2026-10-02-updating-a-tab-hosted-board-erases-its-files.md`.

## What this does not cover

Chromium's own USB stack (re-enumeration timing, a grant revoked by a
replug), radio, a real flash part's erase/program timing and wear, a board
on battery (the walk's "cable pull" is power and data together), and a
pre-repartition firmware image. Those are G1's desk sitting.

## G1 — the real-board walk (2026-10-03, PASSED)

Run on the spare XIAO C6 (`10:bd:a3:b0:8e:30`, old layout, #891 firmware)
and the PLAYFUL choker's bench C6, real Studio over Web Serial — not the
emulator. Scene 1 (inspect/measure) was walked by Yona himself; scenes 2–5
(refused, bypassed, interrupted, happy path) were rehearsed on that same
spare by an agent over real silicon before Yona's own pass; scene 6 (the
choker's update) ran on Yona's bench C6. All scenes passed.

**Scene 6 read-back** — after the choker's update, `hardware lpfs report`
against the bench C6 listed all **37 of 37** files present, byte for byte,
with only the changes an update is expected to make: `/hardware.json`
re-stamped (the new layout's board manifest) and `/.lp/access.json` gaining
this browser's key. The project, the stamp's board name, the Bluetooth lock
and the browser's BLE connection all survived, matching the emulator's W1
and the plan's G1 question 5.

**What silicon found that no emulator run had shown** (none of these were
caught by `just walk-migration-emu`, because the emulator's serial path and
every fielded board's hello were not modelled faithfully enough yet):

- **A macOS Web Serial read stall.** `inspectLayout`'s `readFlash` of 960 KB
  of mostly-erased flash stalled at "Reading the board's files" and never
  finished — Chromium's Web Serial on macOS drops `0xFF`-heavy bytes a page
  reads late (the 2026-09-26 loss, met for the first time by a read that is
  almost entirely `0xFF`). Fixed with a safer read protocol
  (`readFlashSafely`: small packets, one in flight, length + MD5 checked),
  and the emulator's serial shim gained a macOS tty model
  (`mac_tty_model.js`) so `walk-migration-emu` now reproduces and passes
  this exact failure.
  (`docs/defects/2026-10-02-studio-reading-a-boards-files-stalls-on-a-mac.md`,
  `docs/defects/2026-10-02-the-emulated-serial-path-never-drops-a-byte.md`)
- **A wire-32 board read as "pre-hello firmware."** Every fielded board
  (wire 32) failed this branch's wire-33 hello decode (the new `fs` field
  is required) and fell back to the generic "no hello ever seen" verdict,
  with a board picker, instead of "older LightPlayer — flash to update."
  Fixed by reading `proto` (and then the board id) out of a hello that
  otherwise fails to decode, in both Studio's demux and `lp-cli`'s
  `DeviceSession`.
  (`docs/defects/2026-10-02-a-wire-32-board-reads-as-pre-hello-firmware.md`)
- **A full access store lost the list Studio had just read.** On a board
  whose access store was already at its 16-entry cap, the USB connect's
  own key-add was refused, and the refusal threw away the list the same
  sync had just read — the panel showed "Who has access 0" and a locked
  Bluetooth switch for a board that in fact had a full, working list.
  Fixed: the sync keeps the list it read even when its own add is refused.
  (`docs/defects/2026-10-03-a-full-device-store-loses-the-list-studio-read.md`)
- **A held board's access failed open, not closed.** A board holding its
  old filesystem for the migration boots on a memory filesystem with no
  store; the firmware read "no store" as a brand-new board (Bluetooth on,
  unlocked for anyone), and Studio's USB sync wrote this browser's key
  into that RAM store ("Who has access 1") while the board's real 16-entry
  store sat untouched at `0x310000`. Fixed: a held board now boots locked,
  and Studio neither syncs nor lists access for it.
  (`docs/defects/2026-10-03-a-held-board-runs-a-fresh-access-store-in-ram.md`)
- **The refusal's numbers were in the wrong unit.** The over-full refusal
  stated bytes as if they were the planner's 4 KB blocks, which could read
  as a board fitting when it did not. Fixed: the refusal states the same
  measure (blocks of 4 KB, out of 176, with the 16-block reserve) the
  planner refused by.

**Q6 (copy) answers, implemented directly from Yona's live walk notes:**
the card now names the board from an older board's hello instead of
offering a picker ("it really shouldn't say 8 boards fit… ideally we'd
know what board it is"); the card names each step of an update — Reading
the board…, Waiting for your answer…, Flashing firmware…, Moving files…,
Checking the files… — instead of "Flashing firmware…" throughout ("the
'Flashing firmware…' label isn't really right for the first phase"); and
the files sheet's Continue acts on one press instead of arming a second
confirmation inside a surface that already asked the question ("the
continue button on the dialog doesn't really need a confirm… they already
committed to it once"). Landed in `828dcb97c`.

Re-walked on the emulator after every silicon-found fix (above): W1, W3,
W4, W7a, W7b and W9 pass.
