---
status: open
found: 2026-10-10      # e2e (the tree-store cut walk's littlefs control, emulated C6)
area: fw-esp32-common lp_fs (LpFsFlash::write_file) over littlefs-rust 0.1.0
class: write-ordering
related:
  - lp2025/2026-10-08-2339-tree-store-firmware-and-emulator
  - 2026-10-08-littlefs-rust-cross-directory-rename-cut-loses-the-next-entry
---
# A power cut while littlefs creates a file leaves it on flash, empty

**Symptom** — the tree-store cut walk's littlefs control (report-only; `just
walk-tree-store-cuts-emu push`, today's shipped image, `lp-emu:esp32c6:t1+net=lan+flash-cut`)
cut Studio's two-slot push of `projects/test/basic` at in-range op 9, a sector
erase. After the power cycle the board mounts littlefs and serves
`/projects/demo-b/clock.json` as a **0-byte file**: a state no acknowledged
call ever left. Every tear model, `clean` included (the erase simply does not
happen), gives the same image, so it is the state *before* op 9 — between two
littlefs metadata commits of one `write_file`. Replay key `(push, seed
24983259012, index 9, clean)`; the chip was kept at
`target/tree-store-cuts/push-Littlefs-clean-9.bin` (local, not committed).
9 of 643 littlefs push cuts; save (292) and switch (46) were clean. The
panel scenario (CI form, 30 cuts) shows the same in 9: the first
`/projects/demo/.lp/panel.json` the auto-save creates is left empty (replay
key `(panel, seed 140116046667, index 53, calibrated)`) — the panel state
then reads as absent, so the written value is lost.

**Root cause** — `LpFsFlash::write_file` of a file that does not exist yet is
two littlefs commits: the create (an empty entry in the directory's
metadata) and, at close, the data. A cut between them leaves the empty entry.
An overwrite of an existing file was not seen to tear this way (the save
scenario's 292 cuts left the old content or the new). Not checked against the
C littlefs reference.

**Impact** — low today: Studio's push writes the slot the board does not boot
from, and `startup_project` names the new slot only after the load, so the
empty file sits in a slot nothing runs; the next push clears it. A file
created whole in one call elsewhere (a first `/.lp/access.json`, a first
network file) would read as empty after such a cut — the access file then
reads as damaged, which is locked (safe).

**Fix** — none. The tree store (`fs-tree`) commits a file and its directory
in one root and showed no such state in 2,000+ cuts; littlefs leaves the
image in the adoption round. Until then this is the control's known residue.
