---
status: open
found: 2026-10-08      # test (storage testbed random walk, lp-nor-sim power cuts)
area: littlefs-rust 0.1.0 (crates.io) — `Filesystem::rename` across directories
class: write-ordering
related:
  - lp2025/2026-10-07-2337-storage-testbed
  - lp2025/2026-10-07-1858-lpfs-fit-spike
---
# A power cut during a cross-directory rename in littlefs-rust loses an unrelated file

**Symptom** — in the storage testbed (`tools/lp-store-bench`, candidate F2), a
random walk lost the whole `/.lp` directory — `access.json` included — after a
simulated power cut, although the interrupted step never touched `/.lp`.
Reduced with the public API only: format; write `/.lp/access.json` and
`/hardware.json`; create `/projects/b/.lp`; write `/.f2-new.tmp`; rename it to
`/projects/b/.lp/panel.json`. A *clean* cut (the op simply does not happen) in
the last couple of the rename's operations, then a remount, and `/.lp` is gone.

**Root cause** — not yet read in the library. The evidence points at
littlefs's repair of an interrupted cross-directory move (the "move" state a
mount finishes): the entry that disappears is the one sorting **right after the
moved entry** in the source directory (with the temporary named `/zz.tmp`,
which has nothing after it, nothing is lost) — the shape of an off-by-one in
the repair. A rename within one directory is not affected. Not yet checked
against the C littlefs reference.

**Fix** — none in the library. The testbed's F2 keeps every temporary beside
its target (same-directory renames only). The product calls no `rename` on
littlefs today (`/hardware.json.next` is settled by write + delete), so no
shipped path is exposed.

**Regression coverage** —
`tools/lp-store-bench/src/candidates/littlefs_volume.rs`
`littlefs_cross_dir_rename_cut_loses_the_next_entry` pins the loss (it fails
once the library is fixed — drop the pin then).

**Lesson** — "littlefs renames are atomic" holds for the C library's design,
not necessarily for this port under a cut: any future atomic-replace through
littlefs-rust must rename within one directory, and should be cut-tested on
`lp-nor-sim` before it ships.
