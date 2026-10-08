# lp-tree-store

A content-addressed copy-on-write tree store for NOR flash — **v1**, the
device-grade rework of the storage race's prototype T1 (plan
`lp2025/2026-10-08-1017-tree-store-device-round`, M2). `#![no_std]` +
`alloc`, sans-IO over a five-method `Flash` trait and an injected
`ObjectHasher`. Not linked into any firmware yet: M5 wires it behind the C6's
non-default `fs-tree` feature.

**Every on-flash byte is in [`FORMAT.md`](FORMAT.md)** (format version 2),
pinned by `tests/format_golden.rs`. This README is the design, the dials,
the RAM and code figures, and what changed from the prototype.

## The design in brief

- Files are absolute paths; directories are implicit. Every file, chunk,
  directory and root is a record whose id is 64 bits of SHA-256 over a tag
  and its bytes (FORMAT.md "Ids"). A multi-part node's id is a **Merkle**
  hash of its children, so an append rewrites only the right spine.
- The tree is a **cold** directory tree (path-copied on change) and a flat
  **hot** directory of every `…/.lp/panel.json` by full path; the **root**
  names both and carries the retired-sector list. Two write heads: hot
  (panel files, the hot directory, roots) and cold (everything else, GC
  copies).
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
- **Mount** reads every trusted record whole (CRC over header and payload),
  picks the newest root whose closure is complete (falling back one step),
  marks from it, prunes the index to the live set, and hashes every path
  into the path table.
- **GC** runs only when free space is short, at the start of a write phase
  (when everything written is reachable from the committed root, the working
  tree, the delta and the in-flight directories of a flush): a full mark
  makes the live bytes exact, the packing bound refuses before any write,
  then victims are collected — sectors with garbage first (greedy or
  LFS cost-benefit), then sectors whose only waste is a tail, and GC stops
  when collections stop freeing sectors.

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
| path table: sorted `(path hash u64, file id u64, size u32)` | 20 B per file |
| sector table: end u16, live u16, erase count u32, seq u32 | 12 B per sector |
| retired list | 2 B per retired sector |

Between marks the index also holds every record written since; when it has
grown an eighth (+ 16) past the last mark's live set, the next write marks
and prunes it back. Arrays grow by an eighth, not by doubling.

Transient (heap a call allocates and frees, the caller's file buffer
excluded): a record's payload (≤ `record_max`), a directory's bytes and
entries along the path, a file's leaf list (16 B per chunk), the mark's
bitset and stack, a 4 KiB inflate buffer (`put_chunk_deflated`), and — for a
transaction — its delta (≤ `txn_delta_max` + one call) and undo log (24 B
per path touched). **Mount** indexes every record on flash before it prunes,
so its peak grows with the garbage on flash, not with the live data.

Measured (lp-nor-sim simulator, default dials, see "G1 figures" below).

## Dials (`StoreConfig`)

| dial | default | meaning |
|---|---|---|
| `record_max` | 1024 | largest record, header included (128 ..= sector − 20); see "Record size" |
| `gc_policy` | `CostBenefit` | `Greedy` (most garbage) or `CostBenefit` (LFS: `(1−u)·age/(1+u)`) |
| `reserve` | 3 | sectors every write must leave free |
| `txn_delta_max` | 2048 | RAM a transaction's directory delta may hold before it is written as pending directories |

Features: `soft-sha` (default; `SoftSha256` over `sha2`), `lpfs` (`LpFsTree`,
`lpfs::LpFs` over the store, with `begin_batch`/`commit_batch`/`abort_batch`
as the transaction), `host-deflate` (`host_deflate_chunks`, miniz_oxide —
host only), `nor-sim` (`Flash` for `lp_nor_sim::NorFlashSim`).

## API

`TreeStore::format(&mut flash, &mut hasher, &cfg)`,
`TreeStore::mount(flash, hasher, cfg)` (never panics; `Err((error, flash,
hasher))` when no complete root), `get`, `file_size`, `exists`, `list`
(sorted, plain string prefix), `put`, `append`, `put_chunk_deflated`,
`delete`, `delete_prefix` (a `"<dir>/"` prefix is one change),
`delete_file_and_tree`, `begin`, `commit`, `abort`, `in_transaction`,
`stats`, `reset_transient_peak`, `free_sectors`, `flash`/`flash_mut`,
`into_flash`, `into_parts`. After `StoreError::Flash` the store must be
dropped.

**Path-hash collisions.** Two live paths with one 64-bit hash share a row
marked collided, and lookups of that hash walk the directories; mount finds
every such pair, and a write finds one when its walk says the path did not
exist under an occupied row. A path that does not exist but hashes like one
that does reads as that file — the same 64-bit risk content ids take
(spike U9). `txn_tests::colliding_path_hashes_fall_back_to_the_walk` forces
collisions with a test hasher.

## Tests

`cargo test -p lp-tree-store` (≈ 20 s debug, every test runs in parallel):
codecs and record layouts; round trip, dedup, multi-part files and a big
directory surviving GC, torn roots, untrusted sectors, garbage flash,
`NoSpace` before any write, GC under both policies, verify-after-write
retiring a worn sector (both wear modes; the retirement survives remount);
transactions (commit once, read your writes, abort, a bounded delta over
120 files), appends (only new chunks written; same node as a single put),
deflated chunks (verified, refused, stored coded, incompressible stored
plain), forced path-hash collisions; RAM against the budget on a c40-shaped
tree and on a full store, cross-checked with a counting allocator; the
`LpFs` adapter against `LpFsMemory`; the format golden.

**The cut sweeps** (`test_support::sweep`): for each step, every sampled
cut point × every tear model (clean, byte-prefix, random-bits; torn erases
leave weak bits): cut, power-cycle, mount, the **whole state** must be the
old or the new one; a step that did not land is re-run (every third with a
second cut inside the recovery) and must reach the new state, also after a
remount. Workloads: per-call commits (put, panel, append, delete-tree,
delete) at `record_max` 256 and 1024; a transaction of ten puts and a
delete-and-repush in one slot; a 40 KB file appended in 4 KiB calls; a
deflated push in a transaction; GC-forcing churn on 10 sectors at 256 and
1024. `LP_TREE_STORE_SWEEP_CUTS=1000000 LP_TREE_STORE_SWEEP_STEPS=12 cargo
test --release -p lp-tree-store cut_sweep` runs every cut point.

## Code size (RV32)

`size-probe/` is a standalone `no_std`/`no_main` binary (its own workspace):
a RAM flash and every entry point the firmware calls, `default-features =
false`, `opt-level = "z"`, LTO, `codegen-units = 1`, `panic = "abort"`, a
bump allocator, and a stand-in hasher (the C6's SHA is hardware;
`--features soft-sha` links `sha2` instead, +3.8 KB). From `size-probe/`:
`cargo build --release --target riscv32imac-unknown-none-elf`, then
`rust-size -A` and `rust-nm --demangle --print-size --size-sort`.

See "G1 figures" for the numbers.

## What changed from the prototype

Cut (plan D7): JSON-tree mode, the trained dictionary (`Dict` kind,
deflate+dict codec, the sampler), the device-side encoder and its `flate2`
dependency, root fallback past one step.

Changed: B-trees → sorted arrays (index, path table, delta, undo); the RAM
path map of every path string → a path-hash table; whole-commit staging in
RAM → per-call commits, transactions and streaming appends; whole-content
multi ids → Merkle multi ids; dedup against "indexed and closure-complete"
(a mark per check) → dedup against the index, whose closure rule makes it
safe; SHA-256 hard-wired → injected hasher; the root's dictionary and key
fields → the retired list; format version 1 → 2.

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
5. **Mount's transient RAM grows with garbage.** Mount must index every
   record on flash before it knows which are live; a 128-sector flash full
   of panel-write garbage peaks at ~33 KB (see the G1 figures).

## G1 figures (2026-10-08)

Every number here is an **lp-nor-sim simulator** number — not emulator,
not silicon. RAM and space from `lp-store-bench measure` on the spike's
real c40 corpus (132 documents, 216 KB, plus board files) and from
`ram_budget_tests.rs` on a c40-shaped synthetic tree (a counting allocator
the ground truth); code from `size-probe/`.

### RAM, c40, `record_max` 1024 (resident = index + path table + sector table)

| | 128 sectors | 176 sectors |
|---|---:|---:|
| c40 pushed, `host_deflate` (push / save / panel) | 6,516 / 6,720 / 6,548 B | 7,092 / 7,296 / 7,124 B |
| c40 pushed, `stored` (push / save / panel) | 8,076 / 8,472 / 8,108 B | 8,652 / 9,048 / 8,684 B |
| full store, synthetic c40 then ~2,000 writes (3× the flash), deflated: at rest / max | 6,644 / 7,584 B | — |
| full store, the same stored: at rest / max | 8,744 / 10,248 B | — |

(Index 185–202 records host-deflated, 315–348 stored; 138–139 files;
sector table 1,536 B at 128, 2,112 B at 176.)

Transient per call (counting allocator, synthetic c40, stored, 128
sectors): `put` a 2.9 KB shader 3,838 B · `put` the panel 897 B · `append`
4 KiB to an 18 KB file 2,240 B · `get` an 18 KB file 1,159 B beyond the
returned buffer · `file_size` 0 B. A whole-project **push transaction**
holds up to ~6.7 KB (24 B undo per path × 138 paths, plus a ≤ 2 KB delta).
**Mount** peaks at 15.1 KB on a freshly pushed c40 and **33.7 KB** on a
128-sector flash full of panel-write garbage (both codecs), because it
indexes every record on flash before pruning to the ~200–380 live ones.

### Space and the record size (smallest partition, `--min-sectors`)

| `record_max` | host_deflate push / save / panel | stored push / save / panel |
|---:|---:|---:|
| 512 | 25 / 28 / 27 | 54 / 65 / 55 |
| **1024** | **23 / 26 / 25** | **57 / 70 / 58** |
| 2048 | 23 / 25 / 23 | 64 / 83 / 67 |

`record_max` 1024 stays the default: within one sector of 2048 on deflated
pushes, 13 sectors better than 2048 for stored writes (device saves are
stored), and half 2048's per-record buffers. Write amplification at 1024:
0.36–0.50 host-deflated, 0.84–0.95 stored. Mount reads 100–280 KB (every
byte in use).

### Code (RV32, `size-probe`, opt-level z, LTO, stand-in hasher)

| part | `.text` bytes |
|---|---:|
| **whole ELF** | **44,582** |
| `lp_tree_store` functions | 25,750 |
| store code inlined into `_start` (format, mount, the calls) | ~5,000 |
| `alloc`/`core` instantiations and outlined helpers | ~6,080 |
| `lp_deflate` (already in the C6 image for OTA) | 4,562 |
| `compiler_builtins` (shared with any firmware) | 2,350 |
| `lp_crc32` (already in the image) | 98 |
| the probe itself (RAM flash, hasher, allocator) | 722 |

So the store costs **≈ 36.8 KB** of new code (≈ 40.6 KB with `sha2` instead
of the hardware SHA) against littlefs's ~25 KB that it would replace, and
against the prototype's ≈ 39 KB. The largest functions: `RecordLog::append`
(1.9 KB), `ensure_room` (1.7 KB), `flush` (1.3 KB), `mark` (1.1 KB), `u64`
division for the cost-benefit score (0.95 KB), `rebuild_dir` (0.9 KB),
`decode_dir` (0.9 KB, owned `String`s and UTF-8 checks).

### Power cuts

- In-crate sweeps at every cut point (`LP_TREE_STORE_SWEEP_CUTS=1000000
  LP_TREE_STORE_SWEEP_STEPS=12`, release): **9,819 cuts, 0 failures**
  (per-call 387 + 303, transaction 825, append 1,176, deflated push 75, GC
  churn 4,491 + 2,562). The default run samples them in < 20 s.
- `lp-store-bench smoke`: T1 stored 853 cases + 25 random-walk cuts, T1
  host_deflate 856 + 25, **0 failures, 0 non-atomic** (F2 alongside: 0
  failures, 153 non-atomic, as before).
