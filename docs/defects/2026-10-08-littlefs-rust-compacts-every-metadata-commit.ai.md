---
status: open
found: 2026-10-08      # test (storage testbed 30-day endurance run, lp-nor-sim)
area: littlefs-rust 0.1.0 (crates.io; light-player/littlefs-rust) — `lfs_dir_commitcrc`, and the `loop_limits` fetch cap
class: backend-contract-divergence
related:
  - 2026-10-08-littlefs-rust-relocation-cut-leaves-lpfs-unmountable
  - lp2025/2026-10-07-2337-storage-testbed
---
# littlefs-rust erases a sector on every file write: it never writes the FCRC tag

**Symptom** — the storage testbed's 30-day endurance run (`tools/lp-store-bench`
on PR #1040's branch; 1 push, 10 saves and 1,440 rewrites of
`/projects/a/.lp/panel.json` a day) left one sector with 21,617 erases for F2,
and 2,983 for F1 (the firmware's own layout, which the daily push
re-creates). A lamp that is never re-pushed shows the F2 shape on F1 too:
**21,602 erases on one sector in 30 days** (176-sector `lpfs`, the C6's real
size). The total was ~1 erase per file write. A 350 B file rewritten 200 times
erases 200 sectors, on `lp-nor-sim` and on a plain RAM block device alike, so
the simulator isn't the cause. All of these are **simulator numbers**
(`lp-nor-sim`), not silicon.

**Root cause** — littlefs (disk version 2.1, which this port formats) ends
every commit with an FCRC tag: a CRC of the next `prog_size` bytes as erased.
On a later fetch, a pair whose tail still matches that CRC is marked `erased`
and the next commit is *appended*. A pair that cannot prove it is erased
gets *compacted* (erase the other block, rewrite everything). The port's
`lfs_dir_commitcrc` (`littlefs-rust-core/src/dir/commit.rs`) keeps the C
function as its doc comment, FCRC block included, but the Rust body skips
it. Every fetch therefore finds no FCRC (`dir.erased = false`) and every
commit compacts: one erase per write instead of about one per block-full of
commits.

A second limit sits behind it. The port's `loop_limits` feature, which is in
its default features and so in the firmware, panics a fetch past 256 tags
(`MAX_FETCH_TAG_ITER`, `dir/fetch.rs`). Upstream has no such cap. Today no
block ever gets that long, because every commit compacts. With FCRC restored,
appended commits run past it on the first long-lived pair (the patched bench
panicked on every endurance run until the cap was raised to
`block_size / 4`).

**Fix** — none yet; the fix belongs in `light-player/littlefs-rust`. Write the
FCRC as the C does (read `prog_size` bytes at `noff`, `lfs_bd_crc` them,
`lfs_dir_commitattr` an `LFS_TYPE_FCRC` tag before the CCRC), and size
`MAX_FETCH_TAG_ITER` from the block (≥ `block_size / 4`, the most 4-byte tags a
block holds). Both changes need littlefs-rust's own C-compat tests run.
Patched that way in a scratch copy, the same 30 days at 176 sectors with no
re-push measured **4,792 erases in total instead of 44,457** (9.3× fewer), with
the hot sector at 2,161 instead of 21,602. The patched library's cut sweeps (f1 `panel`/`save`/`push`
on c20, f2 `panel` on c40, three tear models, two seeds) showed no failure
kind the stock library doesn't already show. Mount reads went up (f1 c20:
17 KB / 136 calls → 89 KB / 1,467), because metadata logs now hold more than
one commit.

On-flash compatibility: an FCRC tag is part of littlefs 2.1, and this port's
fetch already parses it. A filesystem written by a fixed firmware therefore
mounts on today's firmware (rollback), and today's filesystems mount on a
fixed one. That firmware just compacts each pair once and starts appending.
No migration is needed.

Turning on `block_cycles` (metadata-pair wear levelling, off by default) is
the other half. It is **on hold** behind this fix and its sibling, see the
related defect.

**Regression coverage** — `tools/lp-store-bench/src/candidates/littlefs_volume.rs`
`littlefs_compacts_on_every_commit` (stacked on PR #1040's branch) pins the
one-erase-per-write behaviour. It fails once the library writes its FCRC;
drop the pin then.

**Lesson** — a port that carries its source as doc comments can still drop a
step, and a step that only saves wear shows up in no functional test. Every
read and write still round-trips. Only an erase count over simulated time
showed it. Wear and write amplification belong in a filesystem's test
surface next to power-cut safety.
