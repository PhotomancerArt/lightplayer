# lp-tree-store

A content-addressed copy-on-write tree store for NOR flash — **v1**, the
device-grade rework of the storage race's prototype T1 (plan
`lp2025/2026-10-08-1017-tree-store-device-round`, M2). `#![no_std]` +
`alloc`, sans-IO over a five-method `Flash` trait and an injected
`ObjectHasher`. Not linked into any firmware yet: M5 wires it behind the C6's
non-default `fs-tree` feature.

**Every on-flash byte is in [`FORMAT.md`](FORMAT.md)** (format version 3),
pinned by `tests/format_golden.rs`. This README is the design, the dials,
the RAM and code figures, and what changed from the prototype.

The format has **room to grow without a version bump** (G1, option B;
FORMAT.md "Versioning and extension"): compat and incompat flags and the
sector size in every sector header, a skippable TLV tail on the root, and
record kinds this version does not know skipped as garbage. A good header
with an unknown incompat flag, an unknown head kind or another sector size
refuses the mount with `StoreError::Unsupported` (never a misread, and not
"no store": do not format over it without asking), and so does any sector
whose magic is followed by a **newer format version** — so a core rolled
back after a newer one rewrote the store refuses it instead of seeing a
blank flash and formatting it. The writer programs a sector's magic last,
after the rest of its header reads back, so a torn header never reads as
newer. The reason: a board
updated over Wi-Fi or Bluetooth cannot be re-packed, so a later core's
change has to be readable, or cleanly refused, by the core before it.

## The design in brief

- Files are absolute paths; directories are implicit. Every file, chunk,
  directory and root is a record whose id is 64 bits of SHA-256 over a tag
  and its bytes (FORMAT.md "Ids"). A multi-part node's id is a **Merkle**
  hash of its children, so an append rewrites only the right spine.
- The tree is a **cold** directory tree (path-copied on change) and a flat
  **hot** directory of every `…/.lp/panel.json` by full path; the **root**
  names both and carries the retired-sector list. Two write heads: hot
  (panel files, the hot directory, roots) and cold (everything else); GC
  copies a sector's live records to the head of its own kind.
- **Per-call commits.** `put`, `append`, `put_chunk_deflated`, `delete`,
  `delete_prefix` each write their records, path-copy their directories and
  write a root before returning.
- **Transactions.** `begin` … `commit` (or `abort`): content records go to
  flash as they arrive (*pending*: reachable from the working tree, so GC
  keeps them); directory changes wait in a small RAM delta (written as
  pending directories when it outgrows `txn_delta_max`); `commit` writes one
  root. A cut before that root leaves the old state; the transaction's
  records are garbage. A per-call write is a one-call transaction.
- **Streaming append** (the push's 4 KiB `WriteChunk`s): stored chunks sit
  at fixed `record_max − 16` offsets, the stored tail chunk is re-chunked
  with the new bytes, the multi tree regrouped (full inner multis keep their
  ids). A file built by appends is the same node a single put of it is.
- **Host-deflated chunks** (plan D2): `put_chunk_deflated(path, offset,
  logical_len, expected_id, deflated)` inflates into a ≤ 4 KiB buffer,
  hashes, refuses (`Corrupt`, nothing written) a chunk that does not inflate
  to its length or its id, and stores the deflated bytes as they came (or
  the logical bytes, when the stream does not fit a record or does not
  shrink — same id). Device writes are stored.
- **Verify after write, retire.** Every program is read back; every erase
  is read back as all `0xFF`, then its sector header programmed and read
  back. A mismatch retires the sector (never opened, erased or collected
  again; persisted in the next root), and the record goes elsewhere.
- **Mount** reads every trusted record whole (CRC over header and payload)
  but indexes none of them: that pass finds where each sector's trusted
  records end and keeps the newest two roots. Then it indexes only the
  newest root's closure (falling back one step): level by level, one scan of
  record headers over the trusted sectors, newest sector first, so the first
  copy seen is the one FORMAT.md keeps; each record found is visited with
  the mark's own step. Its RAM is the live set's, whatever the garbage on
  flash.
- **Lookups walk.** No path is kept in RAM: `get`, `file_size` and
  `exists` read one directory record per path level from the working tree
  (the delta laid over it), as writes do. A lookup costs flash reads, not
  resident RAM (see "Future options" for the table this replaced).
- **GC** runs only when free space is short, at the start of a write phase
  (when everything written is reachable from the committed root, the working
  tree, the delta and the in-flight directories of a flush): a full mark
  makes the live bytes exact, the packing bound (a byte sum of the live
  records and the write's, against the usable sectors) refuses before any
  write,
  then victims are collected — sectors with garbage first (greedy or
  LFS cost-benefit), every one of them if need be, then sectors whose only
  waste is a tail, until those stop freeing sectors; last, a head whose own
  garbage would let the write open fewer sectors is renewed (its live
  records copied to a new head of its kind).

## Invariants (hold after every operation and every cut)

- **I1 root:** the committed state is the newest CRC-good root with a
  complete closure; it is written last.
- **I2 at least one copy:** GC erases a victim only after every live record
  in it has a copy that was read back.
- **I3 derived liveness:** nothing on flash records liveness; live =
  reachable from the committed root (and, while writing, the working tree).
- **Index closure** (what makes dedup safe without a per-id mark): the RAM
  index holds, after a mark, exactly the live records, and between marks
  those plus every record written since; nothing it names is erased before
  the next mark prunes it.

## RAM

What the store keeps between operations (`TreeStoreStats.resident_ram_bytes`
— capacities × element sizes of its own vectors; a counting allocator in
`ram_budget_tests.rs` confirms the store holds exactly that after mount):

| structure | per item |
|---|---|
| index: sorted `(id u64, sector u16 \| offset u16)` | 12 B per record |
| sector table: end u16, live u16, erase count u32, seq u32 | 12 B per sector |
| retired list | 2 B per retired sector |

Between marks the index also holds every record written since; when it has
grown an eighth (+ 16) past the last mark's live set, the next write marks
and prunes it back. Arrays grow by an eighth, not by doubling.

Transient (heap a call allocates and frees, the caller's file buffer
excluded): a record's payload (≤ `record_max`), a directory's bytes and
entries along the path, a file's leaf list (16 B per chunk), the mark's
bitset and stack, a 4 KiB inflate buffer (`put_chunk_deflated`), and — for a
transaction — its delta (≤ `txn_delta_max` + one call). **Mount** holds the live index as it builds it, the two
widest adjacent tree levels (16 B per id), 12 B per sector of sorted
headers, and a record's payload; nothing that grows with the garbage on
flash. In exchange it scans record headers once per tree level (16 B per
record on flash per level, stopping a level once every id is found).

Measured (lp-nor-sim simulator, default dials, see "G1 figures" below).

## Dials (`StoreConfig`)

| dial | default | meaning |
|---|---|---|
| `record_max` | 1024 | largest record, header included (128 ..= sector − 24); see "Record size" |
| `gc_policy` | `CostBenefit` | `Greedy` (most garbage) or `CostBenefit` (LFS: `(1−u)·age/(1+u)`) |
| `reserve` | 3 | sectors every write must leave free |
| `txn_delta_max` | 2048 | RAM a transaction's directory delta may hold before it is written as pending directories |

Features: `soft-sha` (default; `SoftSha256` over `sha2`), `lpfs` (`LpFsTree`,
`lpfs::LpFs` over the store, with `begin_batch`/`commit_batch`/`abort_batch`
as the transaction), `host-deflate` (`host_deflate_chunks`, miniz_oxide —
host only), `nor-sim` (`Flash` for `lp_nor_sim::NorFlashSim`), `stats`
(`TreeStore::stats`, `TreeStoreStats`, `reset_transient_peak`: the counters
and the RAM-peak accounting, ~1.2 KB of RV32 code; off in firmware, on for
the crate's tests and `lp-store-bench`).

## API

`TreeStore::format(&mut flash, &mut hasher, &cfg)`,
`TreeStore::mount(flash, hasher, cfg)` (never panics; `Err((error, flash,
hasher))` when no complete root), `get`, `file_size`, `exists`, `list`
(sorted, plain string prefix), `put`, `append`, `put_chunk_deflated`,
`delete`, `delete_prefix` (`"<dir>/"` only: the directory and everything
under it, one change; any other prefix is `InvalidPath`),
`delete_file_and_tree`, `begin`, `commit`, `abort`, `in_transaction`,
`stats` and `reset_transient_peak` (feature `stats`), `free_sectors`,
`flash`/`flash_mut`, `into_flash`, `into_parts`. After `StoreError::Flash`
the store must be dropped.

## Inspecting an image: `lp-cli hardware tree`

Feature `inspect` (host tooling, never on a device; it adds `serde` for
`--json`) is `StoreImage`: a read-only view of a raw image of the partition
that repeats mount's decisions over the bytes instead of a `Flash`, using the
store's own decoders. It never mounts, so it reads where mount refuses, and
`lp-cli hardware tree` is built on it. Every command reads a raw partition
image (`--image`: what `lp-cli hardware lpfs save` writes as
`raw-lpfs-*.bin`, or a whole 4 MiB chip image) or a board (`--port`, over the
bootloader as `lpfs report` does), and **never writes**:

```bash
lp-cli hardware tree inspect --image raw-lpfs-0x350000-123.bin [--records] [--json]
lp-cli hardware tree check   --image raw-lpfs-0x350000-123.bin [--reread second.bin] [--json]
lp-cli hardware tree extract --image raw-lpfs-0x350000-123.bin --out ./recovered
```

- **`inspect`** prints the sector table (state `valid` / `blank` / `killed` /
  `needs-erase` / `NEWER` / `UNSUPPORTED`, head kind, seq, erase count,
  records, live/used bytes, `RETIRED`, why a sector stopped being read), the
  roots found and the one mount would choose (and why any was passed over),
  the file tree with sizes, and live against garbage bytes. `--records`
  lists every record (kind, codec, length, id, CRC, live/garbage/older copy);
  `--json` is the library's `ImageReport` and always carries them.
- **`check`** is the store's fsck. Beyond mount (which only needs every id a
  root reaches to be present and parse) it recomputes every reachable id from
  its bytes, inflates every chunk, checks multi levels and lengths, file
  sizes against their entries, and every directory name against the writer's
  rules (mount accepts a CRC-good tree whose names break them; `list` calls
  such a name corrupt). It also accounts for the rest of the flash: every
  sector that refuses the mount is named (`NEWER` = the magic and a format
  version above this tool's, FORMAT.md "Sector" rule 0) with whether the
  other sectors are a complete store, orphans, older copies (which must be
  byte-identical), torn tails (a warning: a handled crash), retired sectors,
  and, with `--reread` (or `--port`, which reads the board twice), sectors
  that read differently. Exit **2** on any error-level finding; a missing
  committed state is one. It never repairs.
- **`extract`** writes the committed tree's files into a new or empty
  directory. It reads the trusted sectors only, so it works on an image the
  store refuses to mount (a leaked bit made one header read as a newer
  version): the odd sector is reported and the rest extracted. A name that
  could leave the directory, or a file whose path is also a directory, is
  skipped and named (exit 2).

Public API this added: `StoreImage` (`open`, `report`, `check`, `extract`),
`detect_sector_size`, and the plain-data report types (`ImageReport` and its
parts, `CheckReport`/`Finding`/`Severity`, `Extraction`). The tests hold it to
the real mount (same root, same live bytes per sector, same files) on stores
the writer made, to `tests/format_golden.hex`, and to forged images.

## Tests

`cargo test -p lp-tree-store` (≈ 20 s debug, every test runs in parallel):
codecs and record layouts; round trip, dedup, multi-part files and a big
directory surviving GC, torn roots, untrusted sectors, garbage flash,
`NoSpace` before any write, GC under both policies, verify-after-write
retiring a worn sector (both wear modes; the retirement survives remount);
transactions (commit once, read your writes, abort, a bounded delta over
120 files), appends (only new chunks written; same node as a single put),
deflated chunks (verified, refused, stored coded, incompressible stored
plain); `delete_prefix` taking whole directories only; RAM against the budget on a c40-shaped
tree (and what reading every file once costs in flash reads) and on a full store (mount's peak ≤ 16 KB at 128 sectors in both),
cross-checked with a counting allocator; mount's index and live bytes equal
to a full mark's after GC copies and a level-1 directory multi, and the
newest of two copies indexed; the `LpFs` adapter against `LpFsMemory`; the
format golden.

**The cut sweeps** (`test_support::sweep`): for each step, every sampled
cut point × every tear model (by default the three guessed ones: clean,
byte-prefix, random-bits; torn erases leave weak bits): cut, power-cycle,
mount, the **whole state** must be the old or the new one; a step that did
not land is re-run (every third with a second cut inside the recovery) and
must reach the new state, also after a remount, with no sector retired (no
sweep wears one out, so a retirement would be a tear mistaken for wear).
Workloads: per-call commits (put, panel, append, delete-tree,
delete) at `record_max` 256 and 1024; a transaction of ten puts and a
delete-and-repush in one slot; a 40 KB file appended in 4 KiB calls; a
deflated push in a transaction; GC-forcing churn on 10 sectors at 256 and
1024. `LP_TREE_STORE_SWEEP_CUTS=1000000 LP_TREE_STORE_SWEEP_STEPS=12 cargo
test --release -p lp-tree-store cut_sweep` runs every cut point;
`LP_TREE_STORE_SWEEP_TEARS=calibrated` (or any of `lp-nor-sim`'s
`TearModel::NAMED`, comma-separated) picks the tear models — the torn-header
test (`a_torn_kill_erase_or_header_program_never_reads_as_newer`) follows the
same dial — and `LP_TREE_STORE_SWEEP_SECTORS=128` the geometry of every sweep
but the GC one, whose point is a small flash.

## Code size (RV32)

`size-probe/` is a standalone `no_std`/`no_main` binary (its own workspace):
a RAM flash and every entry point the firmware calls, `default-features =
false` (so no `stats`), `opt-level = "z"`, LTO, `codegen-units = 1`, a bump
allocator, and a stand-in hasher (the C6's SHA is hardware; `--features
soft-sha` links `sha2` instead). It builds **with the C6's own flags**
(`size-probe/.cargo/config.toml`, the same as `lp-fw/fw-esp32c6`'s:
`-Zbuild-std=core,alloc` with `optimize_for_size` and
`compiler-builtins-mem`, `-C force-frame-pointers`, `-Z
location-detail=none`, `-Z fmt-debug=none`, `panic=abort`): with the plain
release profile core's sort and formatting look kilobytes bigger and the
store ~4.5 KB smaller than they are on the C6. `--features lpfs` makes the
same calls through `LpFsTree` as a `dyn lpfs::LpFs` (every trait method
linked), for the store plus its adapter. Format and mount go through
`&mut` both, so they share one monomorph (a by-value mount links the store
twice, +8 KB). From `size-probe/`: `cargo build --release --target
riscv32imac-unknown-none-elf [--features lpfs]`, then `rust-size -A` and
`rust-nm --demangle --print-size --size-sort`.

"New to the C6" (the size study's method, plan
`2026-10-08-1017-tree-store-device-round/size-study.md`): the probe's
`.text` symbols, hash suffixes dropped, minus every name a shipped C6 image
already has (the split image `p2.elf` of CI's newest green `main` run,
`71817042569f`), minus ROM routines (`memcpy`, `memmove`, `memset`,
`__udivdi3`, …), `lp_deflate`/`lp_crc32` (linked for OTA) and the probe's
own flash, hasher and allocator; `_start` (format, mount and the calls,
inlined) counts as new.

See "G1 figures" for the numbers.

## Future options

**A RAM path table** (removed in the size pass, `5db02b3d0`): a sorted
table of `(path hash u64, file id u64, size u32)` rows, 20 B per file, built
at mount by hashing every path (id tag 5, never written), so `get`,
`file_size` and `exists` found a file with no flash read; two live paths
with one hash shared a row marked collided and fell back to the walk, and a
transaction kept an undo log of the rows it changed (≈ 24 B per path). It
cost **3,328 B of new C6 code** (3,538 B `.text`; with its collision
handling, the undo log and mount's path walk) and **20 B per file of
resident RAM** (2,660 B at the synthetic c40, a third of the budget). What
it bought: `file_size` of every c40 file in 0 reads instead of 146 KB in
1,376 reads, and `get` of every file in 243 KB / 656 reads instead of
390 KB / 2,032. Worth bringing back (behind a feature, the walk staying the
default) on a part with RAM to spare; the format does not change either
way.

## What changed from the prototype

Cut (plan D7): JSON-tree mode, the trained dictionary (`Dict` kind,
deflate+dict codec, the sampler), the device-side encoder and its `flate2`
dependency, root fallback past one step.

Changed: B-trees → sorted arrays (index, delta); the RAM path map of every
path string → no path in RAM (lookups walk; v1's path-hash table was
removed in the size pass, "Future options"); whole-commit staging in
RAM → per-call commits, transactions and streaming appends; whole-content
multi ids → Merkle multi ids; dedup against "indexed and closure-complete"
(a mark per check) → dedup against the index, whose closure rule makes it
safe; SHA-256 hard-wired → injected hasher; the root's dictionary and key
fields → the retired list; format version 1 → 3 (2 was this round's first
layout, before the extension room).

Added: verify-after-write and sector retirement (and `lp-nor-sim`'s
injected wear-out), host-deflated chunks, the `LpFs` adapter and the trait's
batch methods, FORMAT.md and its golden.

The prototype's design corrections that still apply all hold: kill before
erase; dedup only against complete closures; a record never spans a sector;
CRC over header and payload; mount reads every record whole; a head resumes
only over an all-`0xFF` tail; the hot directory names panels by full path;
`NoSpace` before any record of a write when the packing bound says so;
cost-benefit age counts sector opens; fallback goes one root back and no
further. (Chunk ids are now codec-independent: a chunk's id is over its
logical bytes, stored or deflated.)

### Found while building v1

1. **The prototype never marked a big directory's files live.** A directory
   over one record is a `Multi` of chunks of its bytes; the prototype's mark
   followed the multi's children (chunks) but never parsed the bytes, so the
   files of a directory with more than ~35 entries (at `record_max` 1024)
   were garbage to GC. Latent there (no prototype test GC'd a big
   directory); v1's mark reassembles a directory node, and directory multis
   carry a flag (FORMAT.md "Multi").
2. **GC could not win back tails.** A sector stops being a head when a
   record does not fit, keeping an unused tail; the prototype's victim rule
   only took sectors with garbage, so a flash of live-dense sectors with
   tails refused a write the packing bound had admitted. v1 collects
   tail-only sectors too, and stops when collections stop freeing sectors
   (with records near `record_max`, every sector ends with the same tail
   and compaction gains nothing).
3. **A re-run after a cut costs space the prototype did not pay.** The
   prototype deduplicated against the torn attempt's records (indexed,
   closure-complete garbage), so re-running a step after a cut wrote almost
   nothing. v1 prunes the index to the live set at mount (RAM), so a re-run
   writes its records again; on a nearly full flash that is what GC has to
   find room for.
4. **The per-call directory estimate was too coarse to keep.** Reserving one
   `record_max` per touched directory before a write refused commits a
   nearly full flash could take; each directory now makes room for exactly
   its records when it is written (in-flight records are marked live).
5. **Mount's transient RAM grew with garbage** (fixed after G1, plan P8).
   The first v1 mount indexed every record on flash before it knew which
   were live, and a 128-sector flash full of panel-write garbage peaked at
   ~33 KB. Mount now indexes only the chosen root's closure, found by
   header scans (`mount_walk.rs`): ~14 KB, garbage or not, for ~30 % more
   bytes read (see "Mount" under the G1 figures).

### Found by M3's long walks (fixed after G1)

`docs/defects/2026-10-09-tree-store-rerun-after-a-cut-is-refused-at-the-edge.md`:
on a nearly full flash, a step that fitted was refused `NoSpace` when run
again after a power cut. Nothing committed was lost; the store refused a
write it could hold. Four causes, each pinned by `edge_gc_tests.rs`:

1. **GC gave up early.** It stopped after `reserve + 2` collections that
   freed no sector, although garbage spread thin over many sectors frees
   one only once enough of it is collected. It now collects every victim
   with garbage (each wins its garbage back, so the run ends) and applies
   the stall rule to tail-only compaction alone.
2. **A cut closes the head it was programming** (a record that fails its
   CRC ends what mount trusts in that sector), so the re-run needs a new
   sector for that head. Collecting the closed hot sector copied its live
   records — the root, the hot directory, panels — into the full cold head,
   opening a sector for the one it freed. GC now copies to the head of the
   victim's own kind, which gives the hot head its room back.
3. **The packing bound refused what a layout held.** It took a sector off
   for the second head, so a write the heads' room took fault-free was
   refused after a cut by the bound on the same live set. The bound is now
   the reserve alone (what it always claimed: a write it rejects cannot fit).
4. **A head's garbage was never collected.** A full hot head is mostly old
   roots and hot directories, and no victim rule reaches a head; GC now
   renews a head (its live records to a new head of its kind) when that
   lets the write open fewer sectors.

Still open (`docs/defects/2026-10-10-tree-store-gc-cannot-pack-what-the-bound-admits.md`):
on a 16-sector store at the reserve, GC's in-order copies of near-1 KB
records cannot always pack the cold sectors as tight as the layout a cut
replaced, so a few re-runs are still refused (8 of 756 of `lp-store-bench
mutants`' full-flash cases, lp-nor-sim).

## G1 figures (2026-10-08)

Every number here is an **lp-nor-sim simulator** number — not emulator,
not silicon. RAM and space from `lp-store-bench measure` on the spike's
real c40 corpus (132 documents, 216 KB, plus board files) and from
`ram_budget_tests.rs` on a c40-shaped synthetic tree (a counting allocator
the ground truth); code from `size-probe/`.

### RAM, c40, `record_max` 1024 (resident = index + sector table)

| | 128 sectors | 176 sectors |
|---|---:|---:|
| c40 pushed, `host_deflate` (push / save / panel) | 3,756 / 3,960 / 3,768 B | 4,332 / 4,536 / 4,344 B |
| c40 pushed, `stored` (push / save / panel) | 5,316 / 5,712 / 5,328 B | 5,892 / 6,288 / 5,904 B |
| full store, synthetic c40 then 2,174 writes (400 erases, ~3× the flash), deflated: held after a remount / max during the run | 3,984 / 4,692 B | — |
| full store, the same stored (1,873 writes): held after a remount / max during the run | 6,084 / 7,332 B | — |

(Index 185–202 records host-deflated, 315–348 stored; sector table 1,536 B
at 128, 2,112 B at 176. At G1 each also held a 20 B row per file, 138–139
files: 8,076 B stored push at 128 then; see "The size pass" below. Both
columns re-measured after the size pass; mount reads on the real c40 fell
~5.5 KB and 92 calls with the path walk gone.)

Transient per call (counting allocator, synthetic c40, stored, 128
sectors): `put` a 2.9 KB shader 3,758 B · `put` the panel 801 B · `append`
4 KiB to an 18 KB file 2,240 B · `get` an 18 KB file 1,159 B beyond the
returned buffer · `file_size` 302 B (a directory read; 0 B with the path
table). A whole-project **push transaction** holds its delta (≤ 2 KB
before it flushes, plus one call); the largest buffer over the full-store
run is 4,294 B (6,736 B at G1, with the transaction's undo log).

### Mount (P8: bounded after G1)

Peak heap during `TreeStore::mount` (counting allocator, `ram_budget_tests.rs`,
synthetic c40, 128 sectors unless said) and flash bytes it read, before
(`938df14aa`: index every record, then prune) and after (index the root's
closure by header scans):

| | peak before → after | bytes read before → after | header scans |
|---|---:|---:|---:|
| c40 freshly pushed, stored, 128 sectors | 15,134 → **14,187 B** | 279,526 → 324,996 | 7 |
| c40 freshly pushed, stored, 176 sectors | 15,854 → **14,907 B** | 280,486 → 325,956 | 7 |
| 128 sectors full of panel garbage, stored (379 live records) | 33,662 → **14,187 B** | 495,523 → 640,753 | 7 |
| 128 sectors full of panel garbage, deflated (204 live records) | 33,734 → **12,992 B** | 525,278 → 719,564 | 6 |

The test asserts ≤ 16 KB at 128 sectors in all four (≈ 30 B more per sector
past 128). On the real c40 corpus (`lp-store-bench measure`, bytes / read
calls): stored push / save / panel 219 / 276 / 263 KB in 1,531 / 1,971 /
2,128 calls before, 257 / 334 / 328 KB in 3,903 / 5,595 / 6,211 calls after;
host_deflate 103 / 152 / 146 KB → 127 / 195 / 200 KB (1,056–1,653 →
2,582–5,038 calls). The extra bytes are 16-byte headers, one scan per tree
level, plus the directories and multis the visit reads again; the extra
calls are those header reads one by one.

### Space and the record size (smallest partition, `--min-sectors`)

| `record_max` | host_deflate push / save / panel | stored push / save / panel |
|---:|---:|---:|
| 512 | 25 / 28 / 27 | 54 / 65 / 55 |
| **1024** | **23 / 26 / 25** | **57 / 70 / 58** |
| 2048 | 23 / 25 / 23 | 64 / 83 / 67 |

`record_max` 1024 stays the default: within one sector of 2048 on deflated
pushes, 13 sectors better than 2048 for stored writes (device saves are
stored), and half 2048's per-record buffers. Write amplification at 1024:
0.36–0.50 host-deflated, 0.84–0.95 stored. Mount reads 127–335 KB (every
byte in use once, plus the header scans; see "Mount"). Resident RAM moves the other way (c40 pushed, 128 sectors,
stored / host_deflate): 10,236 / 7,764 B at 512, 8,076 / 6,516 B at 1024,
6,720 / 5,964 B at 2048 — 512 doubles the index and goes over the budget
stored; 2048 saves ~1.4 KB of index for 7–13 more sectors stored.

### Code (RV32, `size-probe`, the C6's flags, stand-in hasher)

At this commit: core probe `.text` **42,210 B**, of which **31,262 B new to
the C6**; through `LpFsTree` (`--features lpfs`) `.text` 57,268 B, **39,908 B
new to the C6** (the adapter and its glue ≈ 8.6 KB). Of the core probe's
new code, `_start` (format, mount and the calls, inlined) is 5,918 B, of
which the probe's own glue is about 0.6 KB (the study's estimate), so the
store alone is ≈ 30.7 KB against the ≈ 30 KB target (Yona, after G1). The
largest functions: `RecordLog::append` (2.1 KB, verify and retire inlined),
`ensure_room` (1.5 KB), `flush` (1.2 KB), `mark_and_prune` (1.0 KB),
`rebuild_dir` (1.0 KB).

On the study's own C6 image (another branch's build, fewer shared names)
the same probe measured ~1.6 KB more new code: at the study's commit
`938df14aa` this method gives 36,546 B where the study gave 38,132 B. The
format room and the bounded mount (P8–P9b, after the study) added ≈ 1.6 KB
of it back, so on the study's scale the store alone is ≈ 32.2 KB, against
its predicted ≈ 30.6 KB for the same cuts.

### The size pass (P11)

Each cut in its own commit, measured on that commit with the C6's flags
(`.text` of the core probe, then new to the C6, then the same through
`LpFsTree`); lp-nor-sim simulator for RAM and reads:

| cut | core `.text` | new to the C6 | via `LpFsTree` |
|---|---:|---:|---:|
| start (`4158daa76`, the C6 flags) | 50,792 | 38,184 | 46,258 |
| one shared heap-sort body | −830 | −762 | −636 |
| counters and RAM-peak accounting behind `stats` (the probe stops calling `stats()`) | −1,222 | −1,212 | −994 |
| byte-sum packing bound | −412 | −346 | −348 |
| **no path table** (lookups walk; `delete_prefix` whole directories) | −3,538 | −3,328 | −3,462 |
| paths and entry names as bytes | −1,868 | −592 | −386 |
| one non-generic `op` body | −382 | −356 | −256 |
| `LpFsTree` on the store's sort | +26 | +30 | −280 |
| one writer for put / append / deflated chunks | −342 | −342 | +26 |
| bounds-check tidy (one paid; two tried made it bigger) | −14 | −14 | −14 |
| **end** | **42,210** | **31,262** | **39,908** |

Kept: both write heads and the hot directory, verify-after-write, the
host-deflate verify, append, no file-size cap, and **both GC policies**
(measured: `Greedy` alone saves 162 B new to the C6 — the cost-benefit
score's u64 division is a ROM routine there — and `CostBenefit` alone
saves nothing; neither is free to drop, so the dial stays).

RAM and reads, before → after (synthetic c40, stored, 128 sectors):
resident 8,744 → 6,084 B; full store held after a remount 8,744 → 6,084 B
stored, 6,644 → 3,984 B deflated; mount peak 14,187 B (12,992 B deflated)
before and after, mount reads 325,500 → 320,482 B. **Reading every file
once:** 243,173 B in 656 reads → 389,588 B in 2,032 reads (each lookup now
reads its directories first). **Every file's size once:** 0 reads → 146,415
B in 1,376 reads.

### Power cuts

- In-crate sweeps at every cut point (`LP_TREE_STORE_SWEEP_CUTS=1000000
  LP_TREE_STORE_SWEEP_STEPS=12`, release), format version 3 with the
  magic programmed last (P9b): **9,963 cuts, 0 failures** (per-call 390 +
  306, transaction 837, append 1,215, deflated push 75, GC churn 4,536 +
  2,604; the extra program per sector opened adds cut points). P9's
  single-program header was 9,759 cuts, 0 failures; with the newer-version
  refusal and that header the append and GC sweeps fail (a torn header
  left the magic in front of a half-programmed version). Version 2 was
  9,819. The default run samples them in < 20 s.
- `lp-store-bench smoke`, format version 3 after the size pass: T1 stored
  865 cases + 25 random-walk cuts, T1 host_deflate 865 + 25, **0 failures,
  0 non-atomic** (P9: 862 + 25 and 859 + 25; version 2: 853 + 25 and
  856 + 25; F2, which this format does not touch, was 0 failures, 153
  non-atomic at P7 and was not re-run).

### Power cuts under the calibrated tears (P10)

`TearModel::Calibrated` is the tear measured on a real C6's flash (CX1, 200
cuts, `docs/reports/2026-10-08-c6-nor-tear-calibration.md` §5): a torn
erase pre-programs the sector to `0x00` front to back before lifting it, so
it leaves a `0x00` run from offset 0, all `0x00`, a zero residue with weak
bits, or a sector that reads `0xFF` (silently, 101 of 166 erase cuts); a
torn program stops on a 32-byte command or a 4-byte word. The five forced
shapes (`calibrated_zeroing`, `_all_zero`, `_erasing`, `_reads_ff_weak`,
`_reads_ff`) make every torn erase that one state. All numbers below are the
**lp-nor-sim simulator**, release builds, every cut point; the store's code
is `7bef0e91b`'s (P10 changed only the sweeps and the bench's drivers).

**0 failures and 0 non-atomic outcomes under every model, in every row.**
The cut points are the workload's, not the model's, so each row's cases and
torn erases are the same under all nine models; only what a torn operation
leaves differs.

In-crate sweeps (`LP_TREE_STORE_SWEEP_CUTS=1000000
LP_TREE_STORE_SWEEP_STEPS=12`), per model, under each of `clean`,
`byte_prefix`, `random_bits`, `calibrated` and the five forced shapes:

| geometry | cuts | of them torn erases | double cuts | failures | sectors retired |
|---|---:|---:|---:|---:|---:|
| the sweeps' own (16/24/32/16; GC churn on 10) | 3,321 | 71 | 1,085 | 0 | 0 |
| 128 sectors (all but GC churn) | 941 | 19 | 301 | 0 | 0 |
| 176 sectors (all but GC churn) | 941 | 19 | 301 | 0 | 0 |

(3,321 = per-call 130 + 102, transaction 279, append 405, deflated push 25,
GC churn 1,512 + 868; × 3 guessed models = the 9,963 above.) The torn-header
test (a format over a live store cut at every op × 24 seeds, every header
read 4×: never reads as a newer format) also passes under all nine.

`lp-store-bench sweep` (exhaustive single cut, seeds 1 and 2), the same
counts at 128 and at 176 sectors and under every one of the nine models:

| candidate | workload | cases | of them torn erases | failures | non-atomic |
|---|---|---:|---:|---:|---:|
| t1 | save:c40 | 1,364 | 30 | 0 | 0 |
| t1 | panel:c40 | 1,830 | 22 | 0 | 0 |
| t1 | push:c40 | 3,288 | 108 | 0 | 0 |
| t1@codec=host_deflate | save:c40 | 1,344 | 26 | 0 | 0 |
| t1@codec=host_deflate | panel:c40 | 1,830 | 22 | 0 | 0 |
| t1@codec=host_deflate | push:c40 | 1,890 | 42 | 0 | 0 |

`lp-store-bench double` (sampled first cuts, a second cut in the recovery),
under `calibrated` at 128 and 176 sectors and under the guessed three at 128:

| candidate | workload | cases | of them torn erases | failures |
|---|---|---:|---:|---:|
| t1 | save:c40 | 2,560 | 32 | 0 |
| t1 | panel:c40 | 12,512 | 176 | 0 |
| t1 | push:c40 | 256 | 0 | 0 |
| t1@codec=host_deflate | save:c40 | 2,560 | 0 | 0 |
| t1@codec=host_deflate | panel:c40 | 12,512 | 176 | 0 |
| t1@codec=host_deflate | push:c40 | 256 | 0 | 0 |

`lp-store-bench smoke --tears <model>` at 128 and 176, each of `calibrated`
and the five forced shapes: 299 cases (2 torn erases) + 25 random-walk cuts
per codec, 0 failures, 0 non-atomic.

Why the shapes change nothing here: mount marks every sector without a
trusted header "needs erase" (`sector_table.rs` `NEEDS_ERASE`), and a
sector is opened only after this session erased it and read it back as
`0xFF` (`RecordLog::kill_and_erase`), so a torn erase in any state —
`0x00` throughout, a residue, or one that reads erased — is erased again
before it holds a record. The header's magic is programmed last
(P9b), so a `0x00` run never meets a header that reads newer.

What this does not cover: the model is one part on one board (JEDEC
`0x464016`); weak bits are the 8 reads the calibration took, and retention
of a sector that read erased after a torn erase is not measured (the store
re-erases it, which is why it does not matter here unless a fully erased
sector itself drifts); the model counts a program's 32-byte commands from
the start of the in-flight page operation, and the store's records start
at any offset, while every silicon program the calibration saw was
page-aligned — whether the ROM's commands start at the address or at a
32-byte boundary for an unaligned program is unmeasured. The c40 workloads
peak at 71 of 128 sectors fault-free (`measure`: stored save 71, panel 67,
push 56; host_deflate 36 / 34 / 23), which is why 176 reads the same, and
few if any of their torn erases can be of a GC victim (a sector still
holding records); the in-crate GC churn sweep (10 sectors, 16–17 GC runs a
sweep) is where torn erases of collected sectors are met.
