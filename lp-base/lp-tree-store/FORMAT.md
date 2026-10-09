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
two, `512 ≤ S ≤ 32768`; the C6 uses `S = 4096`, design target `N = 128`,
today's partition `N = 176`). NOR semantics: erase sets a sector to all
`0xFF`; program can only clear bits. Address of byte `o` of sector `s` is
`s·S + o`.

Every sector header records `S` (as its log2), so a raw image describes its
own sector size: a tool handed an image and no geometry reads byte 7 of the
first header that checks (trying each power of two from 512 to 32768 as the
stride if sector 0 is erased or killed) and then reads every header at
multiples of `S`. **`N` is recorded nowhere** and needs no field: it is the
partition's length divided by `S`, which the partition table (or the image
file's length) already gives, and nothing in the format depends on it
beyond the retired list's sector numbers.

`record_max` (`R`, 128 ≤ `R` ≤ `S − 24`) is a store dial, **not recorded on
flash**: it bounds what a writer produces. A reader must accept any record
that fits its sector (`16 + len ≤ S − 24`). The firmware's value is fixed
per build (see the README's record-size choice).

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

A reader classifies a header in this order:

1. Magic, version and CRC do not all match: the sector is **untrusted**.
   Nothing in it is read, whatever it holds. (An erased, killed or torn
   header, or one of another format version: see "Versioning".)
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
  store no longer needs the feature).
- Bits are allocated from bit 0 upwards in each set and documented here
  with the version of this document that added them. A bit, once fielded,
  is never reused for a different meaning. Adding a flag does **not** bump
  the format version.

Writing and killing:

- A writer programs the header only after an erase it has read back as all
  `0xFF`, and reads the header back after programming it.
- Before every erase, a writer programs the header to **24 zero bytes**
  ("kill"). A torn erase can then never leave an old valid header in front
  of weak bits.
- Head kind says which write head opened the sector: **hot** sectors hold
  `…/.lp/panel.json` files, the hot directory and roots; **cold** sectors
  hold everything else and GC copies. A reader does not need it except to
  resume appending.

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
codec 2; nothing in version 3 gives them a meaning.

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

A chunk's logical length is at most 4096.

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

Entries are sorted by (name bytes, kind); a file and a directory may share
a name. A directory whose bytes fit one record (`≤ R − 16`) is one `Dir`
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
**Version 3 defines no tags and writes an empty tail.**

Every tag is **skippable**: a reader ignores a tag it does not know. There
is no "must understand" tag — information an older reader must not ignore
goes behind an incompat flag instead. A writer **drops** tags it does not
know: its next root carries only the tags it defines. So a tag may hold
only something that stays correct when it vanishes after an older writer's
commit (a hint, a cache, a counter that can be rebuilt), and a newer reader
must treat a missing tag as "not known". The root's id covers the whole
payload, tail included.

## Ids

`id(tag, bytes) = first 8 bytes of SHA-256(tag ‖ bytes)` read **big-endian**
as a u64; the value 0 is replaced by 1. Tags:

| tag | for | bytes hashed |
|---:|---|---|
| 1 | Blob | the chunk's **logical** bytes (so a stored and a deflated copy of a chunk share an id) |
| 2 | Dir (one record) | the payload |
| 3 | Multi | the payload (a Merkle hash of its children, the flags byte included) |
| 4 | Root | the payload |
| 5 | (path hash, RAM only) | the full path's UTF-8 bytes; never written |

A file's node id is its one Blob's id (a file of at most `R − 16` stored
bytes, or one chunk) or its top Multi's id. Ids are 64 bits and **not**
cryptographic (spike U9): a writer reuses a record whose id it already
holds without comparing bytes.

## Files and paths

A path is absolute, `/`-separated, no empty component, no trailing `/`, at
most 32 components and 65535 bytes. Directories are implicit: a directory
exists while a file is under it. A path is **hot** when it ends with
`/.lp/panel.json`; a hot file appears only in the hot directory, every
other file only in the cold tree.

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

A writer appends only to a sector that is not closed (by a record that
failed to check, or by an unknown compat flag) and whose bytes after its
last record all read `0xFF`.

## What a writer guarantees (the invariants)

- **I1 — root last.** Every record a root names is written (and read back)
  before the root. A cut before the root leaves the previous root
  committed; a transaction's records without a root are garbage.
- **I2 — at least one copy.** A sector is killed and erased only after
  every live record in it has a copy elsewhere that was read back.
- **I3 — derived liveness.** Nothing on flash records liveness or free
  space: live = reachable from the committed root (and, while writing, from
  the writer's uncommitted tree).

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
  express (the header's own fields moving, a record header change, a CRC
  change). An older reader sees every sector of the new version as
  untrusted — "no store", never misread — so a version bump ships with a
  migration that rewrites **every** sector before the new firmware writes
  (a mix of old- and new-version sectors could leave an older reader a
  stale but complete root), and a board must be re-packed to take it.

**Why additive room (option B, G1, 2026-10-08).** The only executor that
re-packs a filesystem is the layout migration
(`docs/adr/2026-10-02-c6-repartition-and-layout-migration.md`): it reads and
rewrites the partition on the host, in one bootloader session, **over
USB**. A board updated over Wi-Fi or Bluetooth (the update channel) cannot
be re-packed: its next core must read the flash the previous core wrote, as
it is, and a rolled-back core must read what the newer one left. So a
change a later core makes must be expressible without a version bump — as
a flag, a tag or a skippable kind that the core it replaces handles safely
— and version bumps are kept for changes worth a USB re-pack.

Once a store is fielded (the adoption round), this format is
**never-break**: a change that an existing reader would misread is either
behind an incompat flag or bumps the version and ships with a migration;
the golden in `tests/format_golden.rs` is never re-recorded to make a change
pass. Until then a change is allowed but still bumps the version (or, inside
one unmerged PR, re-records the golden in a commit that says which
deliberate change it records).
