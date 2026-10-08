# lp-tree-store on-flash format — version 2

This is every byte the tree store puts on flash, and the rules a reader
needs to find the committed state. It is written so a second
implementation (the M8 inspect/check tool, a host-side reader, a future
port) can be built from it alone. `tests/format_golden.rs` pins an image of
a fixed tree; this document and that golden must agree.

All integers are **little-endian** unless said otherwise. "CRC-32" is
CRC-32/ISO-HDLC (`lp-crc32`): reflected polynomial `0xEDB8_8320`, initial
value and final XOR `0xFFFF_FFFF`; `crc32(b"123456789") = 0xCBF4_3926`.

## The medium

The partition is `N` sectors of `S` bytes (`4 ≤ N ≤ 65535`,
`512 ≤ S ≤ 32768`; the C6 uses `S = 4096`, design target `N = 128`, today's
partition `N = 176`). NOR semantics: erase sets a sector to all `0xFF`;
program can only clear bits. Address of byte `o` of sector `s` is `s·S + o`.

`record_max` (`R`, 128 ≤ `R` ≤ `S − 20`) is a store dial, **not recorded on
flash**: it bounds what a writer produces. A reader must accept any record
that fits its sector (`16 + len ≤ S − 20`). The firmware's value is fixed
per build (see the README's record-size choice).

## Sector

```
offset  size  field
0       4     magic        0x3153544C  ("LTS1" read as bytes 4C 54 53 31)
4       2     version      2  (this document)
6       1     head kind    0 = cold, 1 = hot
7       1     reserved     0
8       4     sector seq   global order in which sectors were opened (larger = later)
12      4     erase count  erases of this sector so far, this one included
16      4     CRC-32 of bytes 0..16
20      …     records, back to back, from offset 20
```

- A header that does not decode exactly (magic, version, reserved byte,
  head kind ∈ {0, 1}, CRC) makes the sector **untrusted**: nothing in it is
  read, whatever it holds.
- A writer programs the header only after an erase it has read back as all
  `0xFF`, and reads the header back after programming it.
- Before every erase, a writer programs the header to **20 zero bytes**
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
sector**. Reading a sector's records from offset 20:

1. 16 bytes of `0xFF` = **end** of the sector's records.
2. Anything else that does not parse — an unknown kind or codec, a non-Blob
   with codec ≠ 0, id 0 — or a record whose `16 + L` runs past the sector,
   or whose CRC does not match, **closes** the sector: no byte after it is
   read, and a writer never appends to it again.
3. Kind and codec values 5 and 2 were the race prototype's dictionary and
   deflate-with-dictionary (version 1); they are never written and close
   the sector like any other unknown value.

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
0   8   seq            commit sequence (larger = later)
8   8   cold dir id    the tree "/" minus the hot files
16  8   hot dir id     the hot directory
24  2   retired count n
26  2n  retired sectors, strictly ascending
```

Cold and hot dir ids are never 0. Retired sectors failed a read-back and
are never opened, erased or collected again; a reader treats them like any
other sector (their records stay readable).

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

1. Read all `N` sector headers. Order the trusted sectors by sector seq.
2. In that order, read every trusted sector's records per "Record" (header
   **and** payload, CRC-checked: a torn program can leave a good header
   over a short payload). Keep, per id, the copy in the latest sector.
3. Among CRC-good Root records whose payload decodes, take the one with
   the highest seq, and check its **closure**: every id reachable from it
   (root → cold and hot directories → their entries → multis → chunks) is
   present, and every Root, Dir and Multi on the way decodes. A directory
   node that is a Multi is reassembled and its entries followed. If the
   closure is incomplete, try the root with the next-highest seq; no
   further. Neither complete = the flash holds no store.
4. The chosen root is the committed state: the cold tree plus the hot
   files, and its retired list.

A writer appends only to a sector that is not closed and whose bytes after
its last record all read `0xFF`.

## What a writer guarantees (the invariants)

- **I1 — root last.** Every record a root names is written (and read back)
  before the root. A cut before the root leaves the previous root
  committed; a transaction's records without a root are garbage.
- **I2 — at least one copy.** A sector is killed and erased only after
  every live record in it has a copy elsewhere that was read back.
- **I3 — derived liveness.** Nothing on flash records liveness or free
  space: live = reachable from the committed root (and, while writing, from
  the writer's uncommitted tree).

## Versioning

The sector header's `version` is the format version. **This is version 2.**
Version 1 was the storage race's prototype: never fielded, and not readable
by this code (it had a dictionary record kind, a dictionary codec, a 36-byte
root, and whole-content ids for multis).

Once a store is fielded (the adoption round), this format is
**never-break**: a change that an existing reader would misread bumps the
version and ships with a migration; the golden in
`tests/format_golden.rs` is never re-recorded to make a change pass. Until
then a change is allowed but still bumps the version and re-records the
golden in a commit that says so.

A reader refuses a sector whose version it does not know (the sector is
untrusted), so a store written by a newer version does not mount on an
older reader — it is seen as "no store", never misread.
