# lp-tree-store on-flash format — version 3

This is every byte the tree store puts on flash, and the rules a reader
needs to find the committed state. It is written so a second
implementation (the M8 inspect/check tool, a host-side reader, a future
port) can be built from it alone. `tests/format_golden.rs` pins an image of
a fixed tree; this document and that golden must agree.

All integers are **little-endian** unless said otherwise. "CRC-32" is
CRC-32/ISO-HDLC (`lp-crc32`): reflected polynomial `0xEDB8_8320`, initial
value and final XOR `0xFFFF_FFFF`; `crc32(b"123456789") = 0xCBF4_3926`.

## The medium

The partition is `N` sectors of `S` bytes (`4 ≤ N ≤ 65535`; `S` a power of
two, `512 ≤ S ≤ 65,536`; the C6 uses `S = 4096`, design target `N = 128`,
today's partition `N = 176`). NOR semantics: erase sets a sector to all
`0xFF`; program can only clear bits. Address of byte `o` of sector `s` is
`s·S + o`.

**The sector size is configuration, not a format version.** A board picks
`S` from the format's range (a bigger part might take 32 or 64 KiB erase
blocks); a reader given another `S` than a store's headers record refuses
the store (rule 3), so a store of one size is never misread by a reader
configured for another. An implementation may support a narrower range:
this one (`lp-tree-store`) takes `512 ≤ S ≤ 32,768`, because its RAM sector
table keeps each sector's end and live bytes as u16 (a 64 KiB store needs
those widened in RAM; no byte of the format changes).

Every sector header records `S` (as its log2), so a raw image describes its
own sector size: a tool handed an image and no geometry reads byte 7 of the
first header that checks (trying each power of two from 512 to 65,536 that
it supports as the stride if sector 0 is erased or killed) and then reads
every header at multiples of `S`. **`N` is recorded nowhere** and needs no field: it is the
partition's length divided by `S`, which the partition table (or the image
file's length) already gives, and nothing in the format depends on it
beyond the retired list's sector numbers.

`record_max` (`R`, 128 ≤ `R` ≤ `S − 24`) is a store dial, **not recorded on
flash**: it bounds what a writer produces. A reader must accept any record
that fits its sector (`16 + len ≤ S − 24`) and **never assumes `R`**: every
chunk carries its own length, so a board may raise `R` (a bigger board, a
store of bigger files) or lower it, and a store written under one `R`
reads, appends and collects under another (only dedup between the two is
lost). The firmware's value is fixed per build (see the README's
record-size choice).

## Sector

```
offset  size  field
0       4     magic           0x3153544C  ("LTS1" read as bytes 4C 54 53 31)
4       2     version         3  (this document)
6       1     head kind       0 = cold, 1 = hot
7       1     sector size     log2 of S (12 for 4096)
8       2     compat flags    u16; this version defines none and writes 0
10      2     incompat flags  u16; this version defines none and writes 0
12      4     sector seq      global order in which sectors were opened (larger = later)
16      4     erase count     erases of this sector so far, this one included
20      4     CRC-32 of bytes 0..20
24      …     records, back to back, from offset 24
```

The magic and the version are bytes 0..6 in **every** version, past and
future; nothing else in the header is promised to stay where it is.

A reader classifies a header in this order:

0. The magic matches and the version is **greater** than the reader's: the
   store is **unsupported** and the mount is refused (as in 2), without
   checking the CRC — a newer layout may have moved it. A reader must
   never treat a newer writer's sector as untrusted: if a newer core
   rewrote every sector and the board rolled back to an older one, every
   sector would be untrusted, the store would look blank, and formatting
   it would erase the user's files. **A refused mount is never a reason to
   format**: firmware must not format on `Unsupported`, only on a store
   that is genuinely absent, and only when a person asks for it.
1. Magic, version and CRC do not all match: the sector is **untrusted**.
   Nothing in it is read, whatever it holds. (An erased, killed or torn
   header, or one of an older or never-assigned lower version — 1, 2, 0:
   see "Versioning".)
2. An incompat flag this reader does not know is set: the store is
   **unsupported** and the mount is refused — nothing past the headers is
   read, and no other sector is used to find a root. It is never treated as
   "this sector is untrusted, mount the rest": a newer writer's other
   sectors could still hold an older, complete root, and mounting it would
   silently roll the store back.
3. The sector size byte is not log2 of the `S` the reader was given, or the
   head kind is not one this version defines: **unsupported**, as in 2. (A
   wrong `S` is a misconfigured reader or a foreign image; a new head kind
   changes where records may be, so it is an incompat change by
   construction.)
4. Otherwise the sector is **trusted**. If a compat flag this reader does
   not know is set, the sector is read like any other but is **closed to
   appends**: the writer never resumes a head in it. GC may still copy its
   live records out and erase it, after which it is opened again under the
   writer's own (zero) flags.

The flag rules, for whoever adds the first flag:

- **compat** bit: describes the sector in a way a reader that does not
  know it can ignore, and that stays true while no record is appended
  (e.g. "records here are 4-byte aligned"). Old readers read the sector;
  old writers do not append to it, and erase it only through GC.
- **incompat** bit: an old reader would misread or lose something if it
  read this flash (a new codec on a reachable chunk, a new record kind a
  known kind names, a new directory entry kind, a changed payload layout).
  Set it on **every** sector the writer opens from then on, and write a
  record that needs the feature only into a sector that carries it: then
  wherever such a record is, its own sector's header makes an old reader
  refuse the whole store (and once GC has erased the last of them, the
  store no longer needs the feature). A core sets one only after its
  over-the-air trial is confirmed (the trial rule, "Versioning").
- Bits are allocated from bit 0 upwards in each set and documented here
  with the version of this document that added them. A bit, once fielded,
  is never reused for a different meaning. Adding a flag does **not** bump
  the format version.

Writing and killing:

- A writer programs the header only after an erase it has read back as all
  `0xFF`, and reads the header back after programming it.
- It programs the header in two operations, **the magic last**: bytes
  4..24 first, read back; then, only if they read back as written, bytes
  0..4; then the whole header is read back. So the magic is never on flash
  in front of a version the writer did not write: a power cut or a worn
  cell during the header can only leave a header without the magic, which
  is untrusted (rule 1), never one that reads as a newer version (rule 0).
  Every writer of every version keeps this order.
- Before every erase, a writer programs the header to **24 zero bytes**
  ("kill"). A torn erase can then never leave an old valid header in front
  of weak bits.
- **Formatting** need not erase every sector. A writer that makes an empty
  store kills (24 zero bytes, read back) the header of every sector it
  does not erase and that does not read all `0xFF`, before it writes the
  store's first record: then no earlier store's sector — its roots, of any
  sequence, included — is trusted again, and a kill a power cut left weak
  is finished. It erases sectors 0 and 1 (a littlefs partition's
  superblock pair) so the partition no longer reads as littlefs. Every
  other sector is erased (killed first, as above) when it is first opened.
- Head kind says which write head opened the sector: **hot** sectors hold
  `…/.lp/panel.json` files, the hot directory and roots; **cold** sectors
  hold everything else. GC copies a sector's live records to a sector of
  the same kind. A reader does not need it except to resume appending.

## Record

```
offset  size  field
0       1     kind         1 Blob | 2 Multi | 3 Dir | 4 Root
1       1     codec        0 stored | 1 deflate   (non-zero only for Blob)
2       2     len          payload length L
4       8     id           u64, never 0 (see "Ids")
12      4     CRC-32 of header bytes 0..12 followed by the L payload bytes
16      L     payload
```

Records follow each other with no padding; **a record never spans a
sector**. Reading a sector's records from offset 24:

1. 16 bytes of `0xFF` = **end** of the sector's records.
2. A header with id 0, or a record whose `16 + L` runs past the sector, or
   whose CRC does not match, **closes** the sector: no byte after it is
   read, and a writer never appends to it again.
3. A record that checks (id ≠ 0, fits, CRC good) but whose kind, or
   kind/codec pair, this version does not define is an **unknown record**
   (below): it is skipped and reading goes on after it.
4. Otherwise it is a record of a known kind.

### Unknown records

The defined pairs are: kind 1 (Blob) with codec 0 or 1; kinds 2, 3 and 4
with codec 0. Every other pair is unknown. An unknown record:

- is **garbage**: it is never indexed, and nothing a reader of this version
  follows can name it. A known record naming its id makes the closure
  incomplete ("missing record"), like any absent id.
- does not close its sector: the records after it are read, and a writer
  may append after the sector's last record as usual.
- is never copied by GC; it disappears when its sector is erased.

So a newer writer may add a record kind without a flag **only** for
information an older reader may lose (an index, a hint, a cache) and that
no known kind refers to. A kind or codec that a known record refers to — a
new chunk codec, a new node kind — needs an incompat flag. Kind values 0
and 255 are never assigned. The race prototype (version 1) used kind 5 and
codec 2; nothing in version 3 gives them a meaning. **Kinds 6 and 7 are
reserved** for the index checkpoint (below): version 3 writes neither and
reads both as unknown records.

The same id may be on flash more than once (a GC copy whose original was
not yet erased; a record re-written after a cut). Copies are byte-identical
by construction; a reader keeps the one in the sector with the highest
sector seq.

### Blob (kind 1)

A whole small file, or one chunk of a bigger node.

- codec 0 (stored): payload = the bytes.
- codec 1 (deflate): payload = logical length u16 (≤ 4096) ‖ a raw deflate
  stream (RFC 1951, no zlib header) that inflates to exactly that many
  bytes with no preset dictionary. Written only as the host sent it
  (`put_chunk_deflated`), after the board inflated and hashed it.

A deflated chunk's logical length is at most 4096 (a reader refuses a
longer one); a stored chunk is bounded only by its record (`≤ S − 40`; a
writer cuts stored chunks at `R − 16`).

### Multi (kind 2)

A node bigger than one record: an ordered list of children.

```
0   1   flags+level   bit 7 = the node's bytes are a directory; bits 0–6 = level
1   4   total logical length (sum of the children's)
5   2   count c ≥ 1
7   8c  child ids
```

Level 0 children are Blobs; level `L > 0` children are Multis of level
`L − 1`. The node's bytes are its children's bytes concatenated, in order.
Writers group children `fanout = ⌊(R − 16 − 7) / 8⌋` per record from the
left, so every Multi except the rightmost at each level is full, and chunk
boundaries sit at fixed offsets (`R − 16` for stored chunks).

### Dir (kind 3), and directory nodes

A directory's bytes:

```
count u16, then per entry:
  kind u8           1 = file, 2 = directory
  name length u16
  name              UTF-8, non-empty, no '/'
  size u32          the file's logical length (0 for a directory)
  id u64            the file's or directory's node id
```

Entries are sorted by (name bytes, kind). The encoding could hold a file
and a directory of one name; **a writer never makes one** ("Files and
paths"), and a reader that meets one anyway (damage, a foreign writer)
decodes it and must not fail on it. The name rules (UTF-8, non-empty, no
`/`; full hot paths, below; no file and directory of one name) are the
**writer's**: a directory *decodes* (mount step 3) when its count,
kinds, lengths and ids parse and its bytes end exactly. This reader checks
UTF-8 only where a name leaves the store as a string (a listing), and
reports such a name as corrupt there. A directory whose bytes fit one record (`≤ R − 16`) is one `Dir`
record with those bytes as its payload; a bigger one is a `Multi` with the
**directory bit** set (at every level) over stored Blob chunks of those
bytes. An empty directory exists only as the root's cold directory
(`count = 0`, payload `00 00`).

The **hot directory** uses the same bytes, but its entry names are **full
paths** (`/projects/a/.lp/panel.json`), every entry a file, every path
ending in `/.lp/panel.json`.

### Root (kind 4)

The commit anchor.

```
0       8   seq            commit sequence (larger = later)
8       8   cold dir id    the tree "/" minus the hot files
16      8   hot dir id     the hot directory
24      2   retired count n
26      2n  retired sectors, strictly ascending
26+2n   …   tail: TLV entries to the end of the payload (L bytes in all)
```

Cold and hot dir ids are never 0. Retired sectors failed a read-back and
are never opened, erased or collected again; a reader treats them like any
other sector (their records stay readable).

#### Root tail

The root's payload is length-prefixed by its record header (`L`): the
fields above end at `26 + 2n`, and the bytes from there to `L` are a
sequence of entries

```
0   1   tag      u8
1   2   length   u16, k
3   k   value
```

back to back, the last ending exactly at `L` (no entries is the empty
tail). A tail that does not parse that way — half an entry, a length
running past `L` — makes the root fail to decode (it is not a candidate).
**Version 3 defines no tags and writes an empty tail.** Tag 1 is
**reserved** for the index checkpoint's top (below); tag 0 and 255 are
never assigned.

Every tag is **skippable**: a reader ignores a tag it does not know. There
is no "must understand" tag — information an older reader must not ignore
goes behind an incompat flag instead. A writer **drops** tags it does not
know: its next root carries only the tags it defines. So a tag may hold
only something that stays correct when it vanishes after an older writer's
commit (a hint, a cache, a counter that can be rebuilt), and a newer reader
must treat a missing tag as "not known". The root's id covers the whole
payload, tail included.

### Reserved: the index checkpoint (kinds 6 and 7, root tag 1)

**Not part of version 3's writer**: no version-3 core writes these, and
every version-3 reader skips them (unknown kinds, an unknown tag). They are
allocated, with their layout, so that a later core can add a bounded mount
for big partitions (thousands of sectors) as an additive change — the
shape decided while nothing is fielded, so no two efforts hand out the same
numbers. The layout is read **in place** on flash (never loaded whole) and
updated by small sorted runs, so its RAM stays small and its writes stay
near the live set's changes whatever the partition's size.

The record numbers follow "Ids" and the location convention ("Limits at
scale": `sector u16 ‖ offset u16`, the record header's offset in its
sector).

**Index chunk** (kind 6, codec 0; id = `id(6, payload)`):

```
0   2     count c ≥ 1
2   14c   entries: id u64 ‖ sector u16 ‖ offset u16 ‖ len u16
```

One entry per indexed record: where its header is, and `len` = the
record's **total** length (`16 + L`), so one read fetches header and
payload. Entries are sorted by id, strictly ascending, and a chunk holds
whole entries only (no entry spans two chunks): `⌊(R − 18) / 14⌋` per
chunk, 71 at `R` = 1,024.

**Index fence or top** (kind 7, codec 0; id = `id(7, payload)`). Byte 0 is
`flags+level`: bit 7 set = the **top**, clear = a **fence**; bits 0–6 = a
fence's level `L`.

A fence of level `L` (one node of a run's search tree):

```
0   1     flags+level   bit 7 clear; bits 0–6 = L
1   2     count k ≥ 1
3   20k   refs: first id u64 ‖ child id u64 ‖ sector u16 ‖ offset u16
```

Its children are index chunks (`L` = 0) or fences of level `L − 1`; each
ref's `first id` is the smallest entry id under that child; refs are sorted
by `first id`, strictly ascending, and the children cover disjoint,
ascending id ranges.

The top (one per checkpoint; the root names it):

```
0     1     flags+level   0x80
1     8     covered root seq u64     the commit the index describes
9     1     head kinds h             2 in version 3 (cold, hot), in head-kind order
10    6h    per head kind: watermark sector seq u32 ‖ head end u16
…     1     run count r ≥ 1          newest first
…           per run: entry count u32 ‖ level u8 ‖ count k u16 ‖ k refs (20 B each, as a fence's)
…     2     sector rows n            0, or N
…     4n    per sector s: end u16 ‖ live u16
```

A run is a sorted set of entries; its refs are its fence of level `level`
(level 0: they name chunks directly). The newest run wins for an id in two
runs. A writer keeps the top within one record by merging runs (a base run
plus a few small delta runs is the expected shape) and adds a fence level
to a run whose refs outgrow it.

The **watermarks** are, per head kind, the newest sector of that kind and
its records' end when the index was taken. The optional **sector rows** are
each sector's records' end and live bytes at that moment (`0` for one not
written); they let a writer resume and admit writes without a mark.

**Root tail tag 1** = the top: `top id u64 ‖ sector u16 ‖ offset u16` (12
bytes).

**Using one — every piece verified on use:**

- A chunk, fence or top counts only if the header at its ref's location
  names its kind and its id and its CRC holds. Anything that does not —
  and a missing tag, or trusted sector seqs spanning `2³¹` or more — means
  **there is no index**: mount as without it.
- An entry is used only if its sector is trusted and either that sector's
  seq is older (wrapping) than its head kind's watermark, or it is the
  watermark sector itself and the entry's offset is below the recorded
  head end. Every other sector, and each watermark sector from its
  recorded end, is **rolled forward** by its record header chain; the
  log-order invariant (I4, "What a writer guarantees") makes that
  complete. A sector row is used under the same condition.
- A read through an entry checks the header's id and the record's CRC (as
  every read does), so a stale entry reads as damage and is looked up
  again by scanning, never as another record's bytes.
- The root is found by I5 (roots only in hot sectors, in log order), not
  through the index.
- Dedup and GC wait for a mark that proves the index complete for the
  committed closure.

**Cut and rollback safety, by the existing rules:** chunks, fences and the
top are written and read back before the root that names them (I1), so a
cut before that root leaves them garbage; a writer that does not know the
index (version 3, or a rolled-back core) drops tag 1 at its next commit and
never copies kinds 6 and 7, so the index simply vanishes; its erases give
sectors new seqs, which the watermarks catch.

**The prototype is not this layout.** `claude/tree-store-checkpoint-prototype`
(never merged, never fielded) wrote kind 6 as one byte stream chained across
chunks with entries straddling them, and tag 1 naming its first chunk: it
must be loaded whole, is rewritten whole at every checkpoint and does not
scale past a few hundred sectors. If it is ever built, it adopts this
layout. A reader of this layout rejects a prototype image's tag (its
location holds no kind-7 top) and mounts without an index.

## Ids

`id(tag, bytes) = first 8 bytes of SHA-256(tag ‖ bytes)` read **big-endian**
as a u64; the value 0 is replaced by 1. Tags:

| tag | for | bytes hashed |
|---:|---|---|
| 1 | Blob | the chunk's **logical** bytes (so a stored and a deflated copy of a chunk share an id) |
| 2 | Dir (one record) | the payload |
| 3 | Multi | the payload (a Merkle hash of its children, the flags byte included) |
| 4 | Root | the payload |
| 5 | reserved | never written (v1's RAM path table hashed paths with it; that table is gone) |
| 6 | reserved: index chunk (kind 6) | the payload; not written by version 3 |
| 7 | reserved: index fence or top (kind 7) | the payload; not written by version 3 |

A file's node id is its one Blob's id (a file of at most `R − 16` stored
bytes, or one chunk) or its top Multi's id. Ids are 64 bits and **not**
cryptographic (spike U9): a writer reuses a record whose id it already
holds without comparing bytes.

## Files and paths

A path is absolute, `/`-separated, no empty component, no trailing `/`, at
most 32 components and 65535 bytes. Directories are implicit: a directory
exists while a file is under it. **A path names a file or a directory,
never both**: a writer refuses a new file under a path that is a file, or
at a path that files are under (`LpFsTree`: `InvalidPath`, nothing
written; littlefs and every host filesystem refuse the same, so every tool
that reads a store — extract, a backup, a desktop folder — can hold what
it finds). Names are compared as bytes: no Unicode normalization, and case
matters (`A.glsl` and `a.glsl` are two files; a case-insensitive host
folder cannot hold both, which is an app's concern, not the store's). A path is **hot** when it ends with
`/.lp/panel.json`; a hot file appears only in the hot directory, every
other file only in the cold tree.

### The snapshot namespace

`/.lp/snapshots/` is **reserved** for snapshots of a project
(`SNAPSHOT_DIR`). Version 3 makes no snapshot; it reserves the path so a
later core can add them with no format change, kept by every core:

- **What a snapshot is.** A directory entry of kind 2 at
  `/.lp/snapshots/<project>/<name>` naming a directory node that held the
  project at that moment, plus a copy of the project's hot files at
  `/.lp/snapshots/<project>/<name>/…/.lp/panel.json` (hot by the rule
  above, so in the hot directory). It is ordinary tree: every reader's
  mark, mount closure and dedup already follow directory entries to any
  directory node, so a snapshot shares every unchanged blob, multi and
  subdirectory with the live project and with other snapshots, and costs
  only the records it alone reaches. Restore points the project's path at
  the snapshot's node; a diff walks two directory ids and descends only
  where they differ.
- **Who writes there.** Only the snapshot feature. Every other writer
  refuses a path under it (`LpFsTree`: `InvalidPath`, nothing written).
  Deleting under it is an ordinary delete (it frees what the snapshots
  alone held).
- **Write-only files.** A snapshot never contains a write-only file — an
  access file (`.lp/access.json`) or the network file (`.lp/network.json`),
  whatever `lpc_access::is_write_only_file_path` matches: the snapshot
  writer names a directory node built without them, never the project's own
  node while that holds one. And the write-only rule covers any copy
  regardless: the predicate matches the last two components wherever they
  sit, so `/.lp/snapshots/<project>/<name>/.lp/access.json` is write-only
  (pinned in `lpc-access`).
- **Listing and sync.** Snapshots are files to every listing
  (`LpFs::list_dir` of `/` or `/.lp` names them). Studio syncs and hashes a
  project by its own prefix (`/projects/<id>`), which a snapshot is never
  under, so no project sync walks them; a whole-image tool (`lp-cli
  hardware tree extract`) extracts them as files and a restore writes them
  back as files (sharing returns by dedup).
- **Space.** A snapshot's records are live like any other: admission and
  GC count them, so a full store refuses writes until snapshots go
  ("keep N, drop the oldest" is the feature's policy, not the format's).

Should a core that predates the feature write after one that made
snapshots (a failed trial's rollback), the snapshots stay files to it and
survive; that is more than the trial rule asks.

## Finding the committed state (mount)

1. Read all `N` sector headers, each classified per "Sector". Any
   unsupported header refuses the mount (the store is *unsupported*, not
   absent: a caller should not format over it without asking). Order the
   trusted sectors by sector seq.
2. In that order, read every trusted sector's records per "Record" (header
   **and** payload, CRC-checked: a torn program can leave a good header
   over a short payload; unknown records are checked and skipped). Keep,
   per id, the copy in the latest sector.
3. Among CRC-good Root records whose payload decodes, take the one with
   the highest seq, and check its **closure**: every id reachable from it
   (root → cold and hot directories → their entries → multis → chunks) is
   present, and every Root, Dir and Multi on the way decodes. A directory
   node that is a Multi is reassembled and its entries followed. If the
   closure is incomplete, try the root with the next-highest seq; no
   further. Neither complete = the flash holds no store.
4. The chosen root is the committed state: the cold tree plus the hot
   files, and its retired list.

**A faster reading of steps 2–3, with the same result.** Step 2 reads
every byte ever written, and the only records a power cut can have left
not checking are the last program in the newest sector of each head kind
(below). So a reader may instead:

- read the newest sector of each kind, and every sector closed by an
  unknown compat flag, as step 2 says (and, if it is a writer, settle the
  newest, below);
- read every other trusted sector by its **record headers** only: at each
  offset the 16 header bytes, then on to `offset + 16 + L`, ending and
  closing exactly where step 2 would on a header (`0xFF…`, id 0, a record
  past the sector) — reading a Root's payload, which must check and decode
  as in step 2 (a root that does not closes the sector there);
- in step 3, read and CRC-check **every** record of the closure it adopts,
  blobs included; a record that does not check makes the closure
  incomplete;
- and when that adopts no root, read the whole flash as step 2 says and
  decide from that alone (a newer copy that does not check, over an older
  one that does, is adopted that way; a store with no complete root is
  told from a blank one by that reading).

"The newest sector of each kind" is the highest sector seq only while the
seqs have not wrapped (the counter is u32 and `wrapping_add`s): a reader
that finds trusted seqs spanning `2^31` or more reads as step 2 says. On
any flash a writer of this format leaves, the two readings adopt the same
root, the same copies and the same sector ends; they differ only over
damage no cut makes.

A writer appends only to a sector that is not closed (by a record that
failed to check, or by an unknown compat flag) and whose bytes after its
last record all read `0xFF`.

A record whose CRC checked on one read may not check on the next: a program
torn inside a 4-byte word can leave its one missing clear weak. Only the
last program in the newest sector of a head kind can be torn that way, so
before a writer trusts that sector (resumes it, or adopts a root or a copy
found in it) it settles it: it programs the sector's last record again with
the bytes it read and checked, and reads them back (program only clears, so
this finishes the clear; a mismatch closes the sector); when the sector's
records end at a record that failed to check, it programs that record's 16
header bytes to zero, which no reader accepts. A newest sector that holds no
record is not resumed. These are writer rules: they program only bytes a
reader already classifies the same way, and no byte of the format changes.

## What a writer guarantees (the invariants)

- **I1 — root last.** Every record a root names is written (and read back)
  before the root. A cut before the root leaves the previous root
  committed; a transaction's records without a root are garbage.
- **I2 — at least one copy.** A sector is killed and erased only after
  every live record in it has a copy elsewhere that was read back.
- **I3 — derived liveness.** Nothing on flash records liveness or free
  space: live = reachable from the committed root (and, while writing, from
  the writer's uncommitted tree).
- **I4 — log order.** Every program a writer makes, except a kill (a
  sector header before its erase, a record header that failed to check)
  and a settle (programming bytes that already read so), is an **append**:
  at the end of the records of the current head of its kind, which is the
  newest sector of that kind. A sector is appended to only while it is a
  head; once a newer sector of its kind is opened, it is never programmed
  again until it is killed. A sector's seq is assigned when it is opened
  and is greater (wrapping) than every seq trusted at that moment (the
  writer's counter: the newest trusted at mount, plus one per sector
  opened), so while trusted seqs span less than `2³¹`, a later sector has
  the larger seq. Hence, for any earlier moment, everything written since lies in
  sectors opened since or above that moment's head ends in that moment's
  heads — what a roll-forward from a watermark (the index checkpoint)
  needs to be complete.
- **I5 — root placement.** Roots are written only to hot sectors (each
  commit's root, and GC's copy of the committed root, which goes to the hot
  head), and root seqs never decrease in log order: no root in an older
  sector, or earlier in the same sector, has a greater seq than one after
  it. So the committed root is the highest-seq root in the newest hot
  sector that holds a good one (or, when that root's closure is incomplete
  after a cut, the root before it): a reader can find it from the newest
  hot sectors without reading every record header on the flash.

Every v3 core is held to I4 and I5 (the cut sweeps check both after every
step and every cut, `log_order_check.rs`), because a later bounded reader
on a store an earlier core wrote depends on them: a v3-era optimisation
that broke them (topping up an older sector, a root written to a cold
sector) would silently break that reader on fielded stores. They are
writer rules over bytes every reader already classifies; no byte of the
format changes.

## Limits at scale

What each field allows, and where it stands on the largest board planned
(an 8 MB filesystem: `N = 2,048` sectors of 4 KiB). **No field fails at
8 MB, or at 64 MB**; what grows with the partition is the number of records
on flash, which a reader that reads one header per record pays at mount,
and the RAM of an index of every live record (12 B each in this
implementation) — what the index checkpoint (kinds 6 and 7, below) is
reserved for.

| field | width | limit | at 2,048 × 4 KiB |
|---|---|---|---|
| sector count `N` | not on flash; retired list and locations u16 | 65,535 sectors (256 MiB at 4 KiB) | ok |
| sector size `S` | header byte 7, log2 | 512 ..= 65,536, by configuration (above) | ok |
| sector seq | u32, wrapping | a span ≥ 2³¹ makes a reader read the whole flash ("A faster reading") | ~7,700 opens a month: ~23,000 years to wrap |
| erase count | u32 | 4·10⁹ | ok |
| compat / incompat flags | u16 each | 16 bits each; none assigned | ok |
| record `len` | u16 | 65,535; a record never spans a sector, so `≤ S − 40` | ok |
| record id | u64, not cryptographic | accidental collision ≈ `L²/2⁶⁵` (`L` live records); a crafted one is refused by the writer's byte compare ("Ids") | `L` ≈ 11,300 at full: ≈ 3·10⁻¹² |
| root seq | u64 | does not wrap (2⁶⁴ commits) | ok |
| retired list | count u16, sectors u16 | the root is one record: a reader accepts `⌊(S − 66) / 2⌋` sectors (2,015 at 4 KiB); a root held to `record_max` `R`, like every other record, names at most `⌊(R − 42) / 2⌋` — **491 at `R` = 1,024** | 491 = 24 % of the partition |
| root tail entry | length u16 | 65,535, inside the root record | ok |
| Multi | total u32, count u16, fanout `⌊(R − 23) / 8⌋` | 4 GiB a node; levels 0..127 | ok |
| Dir | count u16, name length u16, size u32 | 65,535 entries a directory | ok |
| deflated chunk | logical length u16 prefix | ≤ 4,096 logical bytes (a reader refuses more) | ok |
| stored chunk | its record's `len` | `≤ S − 40`; a writer cuts at `R − 16` | ok |
| `record_max` | not on flash | 128 ..= `S − 24`, a per-board dial readers never assume | ok |
| path | — | ≤ 32 components, ≤ 65,535 B | ok |

**One location convention.** A record that stores where another record is
(the index checkpoint is the first) writes it as `sector u16 ‖ offset u16`,
the offset of the record's header from the start of its sector. That holds
for every `N ≤ 65,535` and `S ≤ 65,536` (an offset is below `S`), so every
later record follows it rather than choosing its own widths.

## Versioning and extension

The sector header's `version` is the format version. **This is version 3.**

- Version 1 was the storage race's prototype (a dictionary record kind, a
  dictionary codec, a 36-byte root, whole-content ids for multis).
- Version 2 was the first device-grade layout: a 20-byte sector header with
  one reserved byte that had to be 0, a root whose length had to be exactly
  `26 + 2n`, and an unknown record kind closing its sector.
- Version 3 (this document) grows the header to 24 bytes for the sector
  size and the compat/incompat flags, gives the root a skippable TLV tail,
  and skips unknown record kinds instead of closing the sector.

Neither 1 nor 2 was fielded, and this code reads neither (their sectors are
untrusted); no migration is written for them.

There are two kinds of change, handled differently:

- **Additive changes do not bump the version.** A new compat or incompat
  flag, a new root tag, a new record kind that nothing known refers to: a
  reader of version 3 handles each by the rules above — it ignores it,
  closes a sector to appends, drops a tag, or refuses the store cleanly.
  Nothing is misread, and nothing needs re-packing.
- **A layout change bumps the version**: what the rules above cannot
  express (the header's own fields after byte 6 moving, a record header
  change, a CRC change). An older reader refuses a store holding any
  sector of the new version (rule 0: `Unsupported("newer format")`) —
  never misread, and never "no store". A board rolled back to the older
  core afterwards refuses its store rather than formatting over it.

**A version bump does not need a USB re-pack.** The version is in every
sector's header and bytes 0..6 are the same in every version, so a store
may hold sectors of two versions at once, and a core that reads both can
run on it. A version bump therefore ships in one of two ways:

- **in place, over the air**: the new core carries a reader of the old
  version beside its own, writes only new-version sectors, and converts the
  store on the device — sector by sector as GC copies live records into new
  sectors, or in one background pass that is GC run over every old-version
  sector. Every step is a GC step (copy, read back, then kill and erase:
  I1, I2), so a power cut anywhere leaves a mixed store the same core
  mounts. Its only limit is free space when new records are bigger than
  old ones (the conversion then needs room reserved before it starts);
- **by a re-pack over USB** (the layout migration, below), when a second
  reader is not worth carrying.

Both obey the trial rule. The cost of a later layout change is the code of
a second reader plus one rewrite of the live set, not a re-pack of every
board.

**The trial rule.** A core that arrived over the air runs **on trial**
until a link confirms it; a trial that fails boots the core before it
(`docs/adr/2026-10-06-ota-update-protocol.md`). A core sets a new
incompat flag, writes a sector of a newer version, or starts a conversion
**only after its trial is confirmed** — never on an unconfirmed trial,
whatever the feature. Everything else a newer core may do on trial (a
compat flag, a new record kind nothing known names, a new root tag) is
safe for the core before it by the rules above. So a failed trial meets the
format's rollback bar: **the previous core boots and reads the ordinary
files** (data that only a newer feature wrote — a snapshot, an index — may
be lost when the older core writes; that is accepted). The firmware's gate
is `fw_esp32_common::tree_fs::may_change_store_format(standing)`, true only
for a confirmed boot (as `lpa_server::access_store::may_migrate_device_store`
is for the device access file); version 3 defines no flag, so nothing calls
it yet, and the first change that needs it must.

**Can anything but a newer writer show the magic and a greater version?**
Rule 0 trusts 6 bytes with no CRC, so it matters what else could put them
there:

- *Erased* (`FF…`) and *killed* (`00…`) headers do not show the magic.
- *A torn kill* over a good header only clears bits, so the version stays a
  subset of 3's bits (0..3), never greater.
- *A torn erase* starts from a killed header (the writer kills before every
  erase) and moves bits towards 1. To read as the magic, exactly its 13
  one-bits must have come up and its 19 zero-bits stayed down — one
  particular 32-bit pattern, about 2⁻³² of torn erases, and a torn erase
  needs a power cut inside the erase itself. (`lp-nor-sim`'s guessed tear
  shapes are stricter still: a byte-wise mix always sets bit 0, which
  `0x4C` lacks, so only an erase edge at byte 0 with four random bytes can
  do it. A real C6's erase, as calibrated —
  `docs/reports/2026-10-08-c6-nor-tear-calibration.md`, `lp-nor-sim`'s
  `calibrated` model — first programs the whole sector to `00…` and then
  lifts every cell at once, so a cut leaves `00…` from the front or
  throughout (no magic), `FF…` (no magic), or a residue of zeros at random
  positions over `FF…`: the same one pattern in 32 bits, at best about
  2⁻³¹ when the residue holds near the magic's own share of zero bits.)
- *A torn or worn header program* cannot: the magic goes last, after the
  rest reads back ("Writing and killing").
- *Records* start at byte 24 and never cover bytes 0..6.
- *Retention loss* can: a programmed bit that leaks back to 1 in the
  version's 14 zero-bits raises it. On a v3 header that now refuses the
  store where it once only untrusted the sector (whose records, perhaps a
  root, were lost with it). The trade is deliberate: a refused store is
  intact on flash and visible; a silently untrusted sector is data loss.

`format_extension_tests.rs` cuts a format over a live store at every
operation, under every tear model and many seeds (the calibrated ones with
`LP_TREE_STORE_SWEEP_TEARS`; none did, P10), and checks that no header ever
reads as a newer version; the power-cut sweeps would fail on one (they did,
before the magic went last).

**Why additive room (option B, G1, 2026-10-08).** The only executor that
re-packs a filesystem is the layout migration
(`docs/adr/2026-10-02-c6-repartition-and-layout-migration.md`): it reads and
rewrites the partition on the host, in one bootloader session, **over
USB**. A board updated over Wi-Fi or Bluetooth (the update channel) cannot
be re-packed: its next core must read the flash the previous core wrote, as
it is, and a rolled-back core must read what the newer one left. So a
change a later core makes is best expressed without a version bump — as a
flag, a tag or a skippable kind that the core it replaces handles safely.
A version bump is still open to an over-the-air core when it carries a
reader of the old version and converts the store on the device (above);
what it costs is that reader's code, not a cable.

Once a store is fielded (the adoption round), this format is
**never-break**: a change that an existing reader would misread is either
behind an incompat flag or bumps the version and ships with a conversion
(on the device by a core that reads both versions, or over USB);
the golden in `tests/format_golden.rs` is never re-recorded to make a change
pass. Until then a change is allowed but still bumps the version (or, inside
one unmerged PR, re-records the golden in a commit that says which
deliberate change it records).
