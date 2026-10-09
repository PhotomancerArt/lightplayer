# lp-tree-store

A **prototype** content-addressed copy-on-write tree store for NOR flash: the
storage testbed's candidate **T1** (plan `lp2025/2026-10-07-2337-storage-testbed`,
design `lp2025/2026-10-07-1858-lpfs-fit-spike/explore.md` §5, §8–§10).
`#![no_std]` + `alloc`, sans-IO over a five-method `Flash` trait. Nothing here
is linked into firmware. It exists to be raced through simulated power cuts
(`lp-nor-sim`, `tools/lp-store-bench`); correctness first, then measurable,
then readable — not tuned.

## The design in brief

- Files are absolute paths (`/projects/a/project.json`); directories are
  implicit. Every file, directory and the dictionary is a **node** whose id is
  64 bits of SHA-256 over a one-byte tag and its *logical* bytes
  (`object_id.rs`). Not cryptography (spike U9): dedup trusts the id, no byte
  compare.
- A node that fits one record (`record_max`, header included) is one record;
  a bigger one is chunks under a tree of `Multi` records whose top carries the
  node's id (`node_layout.rs`). **A record never spans a sector.**
- The tree is two parts: a **cold** directory tree (content-addressed `Dir`
  records, path-copied on change) and a flat **hot** directory of every
  `…/.lp/panel.json`, named by full path. The **root** names both and the
  current dictionary, so a panel write is one blob + the hot dir + a root.
- **Commit** (`tree_store.rs`): `put`/`delete_prefix` only change a RAM map
  (`WorkEntry::Staged` holds the bytes — **nothing touches flash until
  `commit`**). Commit lays out every new record, skips ids already present
  (dedup), marks, makes room or refuses, appends the records — blobs, chunks,
  multis, the dictionary, directories — and writes the `Root` **last**.
- Two **write heads**: hot (panel files, the hot dir, every root) and cold
  (everything else, and GC copies).
- **Mount** (`tree_store.rs::load`): read every sector header; scan each valid
  sector's records in sector-sequence order, CRC-checking header *and*
  payload, into a RAM index `id → (sector, offset, len, kind, codec)`; pick the
  root by I1; load the path map; resume each head only if the tail after its
  last record reads all `0xFF`.
- **GC** (`gc_*.rs`): mark from the current root *and* the commit being
  written; pick a victim by policy; copy its live records to the cold head,
  read each copy back and compare; then kill + erase the victim.

## On flash

```
sector  : [sector header 20 B][record][record]…[0xFF…]
sector header (LE): magic "LTS1" u32 | version u16 = 1 | head kind u8 (0 cold, 1 hot)
                    | 0 u8 | sector seq u32 | erase count u32 | CRC-32 of the first 16 B
record header (LE): kind u8 | codec u8 | payload len u16 | id u64
                    | CRC-32 of the first 12 header bytes + payload        (16 B)
kinds   : 1 Blob | 2 Multi | 3 Dir | 4 Root | 5 Dict
codecs  : 0 stored | 1 deflate | 2 deflate+dict   (Blob only; others are stored)
Blob    : stored = the bytes; deflate = logical len u16 | raw deflate;
          deflate+dict = logical len u16 | dictionary id u64 | raw deflate
Multi   : level u8 | total logical len u32 | count u16 | child ids u64…
          (level 0 children are Blob chunks, level L children are level L−1 Multis)
Dir     : count u16 | per entry: kind u8 (1 file, 2 dir) | name len u16 | name
          | logical size u32 | id u64     (sorted by name, kind)
Root    : seq u64 | cold dir id | hot dir id | dictionary id (0 = none) | next key id u32
```

- A sector header is programmed only right after a **completed** erase, and
  is **killed** (programmed to all zeros) before every erase. A sector without
  a valid header is "needs erase", whatever it reads.
- A record header of all `0xFF` ends a sector's records. A header that does
  not parse, a length past the sector, or a CRC mismatch **closes** the sector:
  nothing after it is read, and nothing is ever appended to it.
- Chunks decode with `lp_deflate::inflate(src, buf, start)`, `buf[..start]` =
  the dictionary. A chunk decodes to at most 4 KiB, so a read needs a
  dictionary + 4 KiB buffer.

## Invariants (hold after every operation and every cut)

- **I1 root:** the committed state is the CRC-good `Root` with the highest
  seq whose closure is complete (every reachable id indexed, every `Dir` and
  `Multi` on the way re-read with a good CRC). Written last. If the newest is
  incomplete, mount tries the next (`root_select.rs`).
- **I2 at least one copy:** GC erases a victim only after every live record
  in it has another copy that was read back and compared (`gc_copy.rs`).
- **I3 derived liveness:** live = reachable from the current root (plus, during
  a commit, from the commit's new root and its planned records). Nothing on
  flash records liveness or free space; free = no live record, not a head.

Two rules the invariants needed that the design did not state (see
"Ambiguities and defects" below): **kill before erase**, and **dedup only
against an id whose closure is complete**.

## Dials (`StoreConfig`) and defaults

| dial | default | meaning |
|---|---|---|
| `record_max` | 1024 | largest record, header included (128 ..= sector − 20) |
| `gc_policy` | `CostBenefit` | `Greedy` (most garbage) or `CostBenefit` (LFS: `(1−u)·age/(1+u)`, age = sector seqs since the victim was opened, u = live / capacity) |
| `reserve` | 3 | sectors every commit must leave free |
| `codec` | `Deflate` | `Stored`, `Deflate`, `DeflateDict` |
| `dict_size` | 8192 | trained dictionary size |
| `dict_train_min` | 16384 | new (not yet stored) bytes in one commit that make it "a push" and train a fresh dictionary |
| `json_tree` | false | **not implemented**: `true` is `Err(Unsupported)` |

- **Encoder:** feature `encode` (default on): `flate2` on the `zlib-rs`
  backend at level 9, raw (no zlib header), with `Compress::set_dictionary`
  for `DeflateDict` — the same encoder and call `lpa-update`'s OTA packer uses.
  `miniz_oxide`'s compressor has no preset-dictionary API, so it could not do
  `DeflateDict`. Every encoded chunk is decoded back with `lp_deflate` before
  use; a chunk that does not shrink is stored. Without `encode` (a device
  image) every chunk is written stored; every codec still decodes.
- **Dictionary:** trained at push time from the commit's new cold content
  (`store_dictionary.rs`): count every 24-byte window at a 4-byte stride,
  keep windows seen twice or more, merge their first occurrences into spans,
  dedupe, concatenate best-last, trim to `dict_size`. Simple and
  deterministic, not zstd's COVER. It is a node (a `Multi` of stored chunks at
  8 KB), named by the root and by each chunk that uses it; old dictionaries
  stay live while an old chunk still names them.
- **NoSpace:** before anything is written, commit computes exactly how many
  sectors the heads must open for its records (the same rule `append` follows,
  `space_estimate.rs`). If the free sectors cover that plus `reserve`, it
  writes. Otherwise it checks a packing bound — every live record plus the new
  ones packed first-fit-decreasing must leave one sector for the other head and
  the reserve — and returns `NoSpace` **before any write** when that fails.
  When it passes, GC runs until there is room; if GC cannot get there,
  `NoSpace` comes **after GC copies but before any record of the commit**, so
  the committed state is untouched either way. After `NoSpace` the staged
  changes remain; `discard_uncommitted()` drops them.

## API (what the testbed adapter calls)

`TreeStore::format(&mut flash, &cfg)`, `TreeStore::mount(flash, cfg)` (never
panics; `Err((error, flash))` on no complete root), `put`, `get`
(read-your-writes), `delete_prefix` (plain string prefix), `list` (sorted),
`commit` (whole-step atomic; no-op when nothing changed), `discard_uncommitted`,
`stats()`, `free_sectors()`, `flash()`/`flash_mut()` (fault plans),
`into_flash()`. After `StoreError::Flash` the store must be dropped. Feature
`nor-sim` implements `Flash` for `lp_nor_sim::NorFlashSim`.

`TreeStoreStats`: index entries and RAM (entries × 16 B), path-map RAM, sector
table RAM, largest single buffer (staged content, the plan, a read, a mark
set), dedup hits, GC runs / copies / copy bytes, commits, records and bytes
written, sectors opened, erases, and the last mount's flash bytes read.

## Tests

`cargo test -p lp-tree-store` (≈ 10–20 s debug): round trip per codec,
read-your-writes, list, delete, remount; dedup; multi-part nodes at
`record_max` 256 (a 40 KB blob, a 40-entry directory); a torn root falls back
and the torn sector is never appended to; a sector without a header is never
trusted (a valid root record planted behind no header is ignored); garbage
flash never mounts and never panics, then formats; `NoSpace` before any
program or erase, then the store keeps working; GC keeps live data and
reclaims garbage under both policies over ~10× the flash; JSON-tree refused.

**The cut sweeps** (`test_support::sweep`): for each step of a workload, run it
fault-free to count its ops *n*; for sampled *k* in 0..=*n* and every
`TearModel` (clean, byte-prefix, random-bits — torn erases leave weak bits),
fork the pre-step flash, cut at *k*, power-cycle, mount, require the **whole
state** to be the old or the new one; every third cut tears the recovery run
too (a double cut); then re-run the step fault-free and require the new state,
also after a remount. Workloads: a 4-step push/panel/edit/delete one on 16
sectors under every codec, and a GC-forcing churn on 10 sectors under
stored/1024, deflate+dict/256 and deflate/512 (GC runs inside the cut range).
`LP_TREE_STORE_SWEEP_CUTS=1000000 LP_TREE_STORE_SWEEP_STEPS=12 cargo test
--release -p lp-tree-store cut_sweep` runs them exhaustively (every *k*, 12
churn steps): **green, 28.5 s** on 2026-10-08.

## Code size (RV32)

`size-probe/` is a standalone `no_std`/`no_main` binary (its own workspace,
excluded from the root one): a RAM flash, format, mount, put, delete_prefix,
commit, get, list, stats; `default-features = false` (no encoder: writes
stored, reads link `lp_deflate`), `opt-level = "z"`, LTO, `codegen-units = 1`,
`panic = "abort"`, a bump allocator. From `size-probe/`:
`cargo build --release --target riscv32imac-unknown-none-elf`, then
`rust-size -A` and `rust-nm --demangle --print-size --size-sort`.

| part (2026-10-08) | `.text` bytes |
|---|---:|
| **whole ELF** | **49,852** |
| `lp_tree_store` symbols | 21,088 |
| store code inlined into `_start` | ~4,700 |
| `alloc` B-tree, the store's own instantiations | 12,936 |
| `sha2` (SHA-256 compress; the C6 has it in hardware) | 3,170 |
| `compiler_builtins` (shared with any firmware) | 2,306 |
| `lp_deflate` (≈ 2 KB with its inlined inflate; already in firmware for OTA) | 720 + inlined |
| other (`alloc`/`core` helpers) | 4,904 |

So the store itself costs **≈ 39 KB** (its code + its B-tree
instantiations), against explore.md's 8–14 KB estimate. Two wins are already
in: one store instantiation instead of two (the probe mounts `&mut RamFlash`
like `format` does: a by-value mount links the whole store twice — 87 KB), and
a tiny insertion sort instead of `core`'s sorts (−14 KB). The next lever is
the B-trees: a sorted `Vec` for the index and the path map would remove most
of 13 KB.

## First look at c40 (scratch run, 128 sectors; the harness's numbers rule)

Push of c40 (132 docs, 216,028 B) as `/projects/a/…`: sectors in use after the
push — stored 51–63, deflate 20–22, deflate+dict 19–20 (`record_max`
512/1024/2048); RAM index 2.3–7.9 KB (141–493 records); path map ≈ 10.7 KB;
largest buffer 38 KB (reading the biggest map file whole); mount reads
82–207 KB (every written byte — see defect 1).

## What the prototype does not do

- **JSON-tree mode** (`Key`/`Node` records, key GC by marking): not built;
  `json_tree: true` is refused. Blob mode had to be green under every cut
  first, and the overnight launch came before there was time for it.
- No streaming: a commit's content sits in RAM until `commit` (a push's
  largest buffer is the whole push), and `get` returns a whole file.
- No wear leveling beyond opening the free sector with the lowest known erase
  count; an unreadable header's erase count is lost (restarts at 0).
- No lazy mount: every written byte is read at mount.
- No 256-bit Merkle root for push verification, no deltas, no host-built
  packs, no full repack, no hardware SHA.
- Index entries are one location per id; duplicates on flash are found again
  only by a remount.

## Ambiguities and defects in the design (explore.md) found while building it

1. **"Mount reads 128 sector headers and ~600 record headers (~10 KB)" is not
   enough.** A torn program can leave a record's header intact and its payload
   short; only the payload CRC shows it. Either mount reads every record whole
   (this prototype: mount bytes ≈ bytes in use), or every record must be
   verified lazily before anything trusts it — including before dedup reuses
   it. Header-only mount needs a separate header CRC *and* lazy payload checks.
2. **"Program the sector header after the erase" is not by itself a trusted
   "erase finished" mark.** The flash model's torn erases include "old bytes
   kept" shapes, so a torn erase can leave the *old* valid header in front of
   weak bits. The fix here: **kill the header (program zeros) before every
   erase**; a killed header can only read valid by ~32 weak bits landing
   right. §5's "state byte" does not help: a torn erase can return it to `0xFF`.
3. **"A blob whose id is already indexed is not written again" is unsafe as
   stated.** Garbage records outlive their children: a garbage `Multi` or
   `Dir` survives while the sector holding its chunk was erased, and a garbage
   blob survives its collected dictionary. Reusing such an id makes a new root
   with an incomplete closure. Dedup here trusts an id only if it is planned,
   live under the current root, or indexed **with a complete closure**
   (checked by marking it).
4. **The dictionary is not one record.** An 8 KB dictionary cannot fit
   `record_max` (≤ a 4 KB sector by the design's own rule); it is a multi-part
   node. And because unchanged chunks keep naming the dictionary they were
   coded against, liveness follows each chunk's dictionary id, not just the
   root's — so several dictionaries can be live after a few pushes.
5. **"The root names the panel subtree directly"** assumes one panel; there is
   one `.lp/panel.json` per project. Chosen: a flat hot directory of every
   panel file by full path, named by the root.
6. **§9 vs §10:** §9's refined T1 has stable ids + versions ("no wandering
   tree"); §10 has content addressing, where an id *is* the content and a
   change re-hashes every ancestor. These are different stores. Built: §10
   (content-addressed, path copying); the hot directory keeps the panel path
   short.
7. **"NoSpace before writing anything" conflicts with GC.** GC must sometimes
   copy before it knows whether a commit fits; a strict before-any-write
   guarantee needs an exact compaction bound. The worst-case bound (every
   sector loses `record_max − 1` to its tail) was tried first and refused a
   37 %-full 10-sector flash. Built: refuse before any write when even a
   first-fit-decreasing packing cannot fit; otherwise GC may copy and then
   refuse — still before any record of the commit, so the committed state is
   untouched.
8. **Cost-benefit "age" has no clock** in a sans-IO store; age here is how
   many sectors were opened since the victim was.
9. **Ids are codec-independent only at the node level.** Chunk boundaries
   depend on how well a chunk compresses, so chunk ids (and so chunk-level
   dedup across edits) depend on the codec and the dictionary.
10. **An all-`0xFF` record header is not proof the rest is erased.** A torn
    random-bits program can clear payload bits and none of the header's, so a
    head is resumed only after its whole tail reads `0xFF`.
11. **The code-size estimate (8–14 KB) was about 3× low** (≈ 39 KB here; see
    above).
12. **Falling back past the newest root is not a safety net.** GC keeps only
    the current root's closure, so older roots are generally incomplete; the
    fallback works one step (a torn newest root) and no further by design.
