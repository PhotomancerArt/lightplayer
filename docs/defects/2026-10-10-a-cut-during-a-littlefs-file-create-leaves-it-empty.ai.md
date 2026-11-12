---
status: fixed
found: 2026-10-10      # e2e (the tree-store cut walk's littlefs control, emulated C6)
fixed: this change     # PR #1134
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
C littlefs reference. (Since read: littlefs-rust-core's
`lfs_file_opencfg_` — `src/file/ops.rs`, whose doc comment quotes the C
original — commits `CREATE` + name + an empty inline struct at open when the
entry is new, so this is littlefs's own shape, not a port defect.)

**Impact** — low today: Studio's push writes the slot the board does not boot
from, and `startup_project` names the new slot only after the load, so the
empty file sits in a slot nothing runs; the next push clears it. A file
created whole in one call elsewhere (a first `/.lp/access.json`, a first
network file) would read as empty after such a cut — the access file then
reads as damaged, which is locked (safe).

**Fix** (2026-10-14, `fw-esp32-common/src/lp_fs.rs`) — `write_file` of a
file that does not exist writes it whole under a hidden name beside it
(`<name>#lp-new`, `CREATE_TEMP_SUFFIX`) and renames it into place: one
metadata commit, within one directory (never across, see
`2026-10-08-littlefs-rust-cross-directory-rename-cut-loses-the-next-entry`),
with a suffix that sorts the temporary next to its target so both share a
metadata pair. A cut leaves no file or the whole one; a left temporary is
never listed, the next create of that file takes it over, and removing the
directory removes it. An overwrite stays one commit at close, which is now
checked (littlefs-rust's own `write_file` dropped the close's error). No
on-flash format change: an older firmware sees one more file. Readers of the
small state files (`panel.json`, `/lightplayer.json`, `/.lp/device.json`)
already read an empty or unparseable file as absent; each now says so in one
plain line. The access file still reads one as damaged and locked (the safe
answer; the fix means a create cut no longer makes one).

The control cut walk, on the same machine (`lp-emu:esp32c6:t1+net=lan+flash-cut`,
direct load, emulated, never hardware-validated):

| scenario | `main` `dc5164ef1` (cuts / failures) | this fix |
|---|---:|---:|
| push (long) | 649 / 0 | 736 / 0 |
| save (long) | 292 / 0 | 292 / 0 |
| switch (long) | 46 / 0 | 70 / 0 |
| panel (CI form) | 30 / **9** | 37 / 0 |
| panel (long, two writes) | 184 / **21** | 196 / 0 |

The panel failures on `main` are this defect exactly (`panel.json` 102 → 0 B,
ops 48–53). The push no longer reproduced on `main` at `dc5164ef1` (0 of 649;
the op count is 927, not the report's), so its 9 are not re-measured here;
the mechanism is the same one, and the host test below covers it. The new
file costs one more metadata commit per create, so the op counts grew. The
fs-tree walks are unchanged (CI subset 162 cuts, 0 failures, the same table
as `main`'s image; boot walks 8/8).

**Regression coverage** — `fw-esp32-common` `lp_fs::tests`
`a_cut_anywhere_in_a_create_leaves_no_file_or_the_whole_one` cuts every
flash operation of a create (inline and past the 512 B inline limit) and
fails at op 6 of 8 without the fix; `..._an_overwrite_leaves_the_old_bytes_or_the_new`,
`a_left_temporary_is_hidden_and_the_next_create_takes_it_over`,
`a_reserved_name_is_refused`. End to end: the littlefs control of
`lp-cli/tests/emu_tree_store_cuts.rs` (report-only).

**Lesson** — littlefs makes each commit atomic, not each call: a create is
two. "Atomic write" on littlefs means one commit per acknowledged state, and
a cut walk at every flash operation is what shows where the commits fall.
