"""A 32-bit model of `rlsf::Tlsf<'static, usize, usize, FLLEN, 32>` — the heap
esp-alloc 0.10 builds when `ESP_ALLOC_CONFIG_HEAP_ALGORITHM=TLSF` — at the
geometry it has on rv32 (the ESP32-C6) and Xtensa (the classic, the S3).

Provenance: derived by reading `rlsf` 0.2.2's `src/tlsf.rs` (crates.io,
checksum `1646a59a9734b8b7a0ac51689388a60fe1625d4b956348e9de07591a1478457a`,
MIT OR Apache-2.0, by yvt), the version `Cargo.lock` pins. No code is copied;
this is an independent model of the behaviour that file defines, so a host
replay can place blocks where the device's allocator would. What it models,
with the source's names:

- `GRANULARITY = size_of::<usize>() * 4` = 16 B on a 32-bit target; every
  block (header included) is a multiple of it, and starts 16-aligned.
- `BlockHdr { size, prev_phys_block }` = 8 B, the per-block cost of a used
  block (`UsedBlockHdr`). A free block also holds `next_free`/`prev_free`
  (`FreeBlockHdr`, 16 B), which fits in the smallest block. The payload
  starts at `block + 8` (`round_up(block + 8, align)` for `align >= 16`).
- `allocate`: `search = round16(size + max(align - 8, 0) + 8)`; `map_ceil`
  rounds `search` up to the next second-level list boundary ("good fit"); the
  first non-empty list at or above it (bitmap scan within the first level,
  then the next set first level) gives its HEAD block; the block is split at
  `round16(padding + 8 + size)` and the remainder pushed onto the head of its
  list.
- `deallocate`: merge with the next physical block if free, then the
  previous if free; push the merged block onto the head of its list (LIFO).
- `insert_free_block_ptr`: the pool start rounds up to 16, the length down
  to 16, and a 16 B used sentinel caps the end; a pool longer than
  `MAX_POOL_SIZE = 2^(4 + FLLEN)` (when that fits in a `usize`) is cut into
  chunks of that size, each with its own sentinel.
- `map_floor(size)`: `fl = floor(log2 size) - 4`, `sl` = the 5 bits below
  the top bit (`SLLEN = 32`, `SLI = 5`); below 512 B every granule has its own
  list.
- `Tlsf::iter_blocks` / `max_payload_size` (what esp-alloc's `free()` sums):
  a free block's payload capacity is its size less the 8 B header.

The device's `largest_free_block()` (the gates' probe) binary-searches the
largest `alloc(N, align 4)` that succeeds; under TLSF that is NOT the largest
hole but `list_min(the largest hole's list) - 8`, because `map_ceil` never
searches the list a request's own size falls in. [`TlsfRegion.largest_request`]
returns exactly that.
"""

from __future__ import annotations

GRAN = 16
HDR = 8
SLI = 5
SLLEN = 32
USIZE_BITS = 32


def round16(x: int) -> int:
    return (x + 15) & ~15


def round_up(x: int, align: int) -> int:
    return (x + align - 1) & ~(align - 1)


def map_floor(size: int):
    """`(fl, sl)` of the list a free block of `size` bytes is linked into."""
    top = size.bit_length() - 1
    fl = top - 4
    shift = top - SLI
    sl = (size >> shift) & (SLLEN - 1) if shift >= 0 else (size << -shift) & (SLLEN - 1)
    return fl, sl


def map_ceil(size: int):
    """`(fl, sl)` of the first list every block of which is >= `size`."""
    top = size.bit_length() - 1
    shift = top - SLI
    if shift > 0 and size & ((1 << shift) - 1):
        size = ((size >> shift) + 1) << shift
    return map_floor(size)


def list_min(fl: int, sl: int) -> int:
    """The smallest block size list `(fl, sl)` holds."""
    top = fl + 4
    shift = top - SLI
    return (1 << top) + (sl << shift if shift >= 0 else sl >> -shift)


class TlsfRegion:
    """One esp-alloc `TlsfHeap`: one `rlsf::Tlsf` over one pool."""

    def __init__(self, name: str, base: int, size: int, fllen: int = 32):
        self.name, self.fllen = name, fllen
        start = round16(base)
        length = (size - (start - base)) & ~(GRAN - 1)
        self.bottom, self.top = start, start + length
        self.bsize: dict[int, int] = {}  # block start -> size (header included)
        self.used: set[int] = set()  # used blocks (sentinels included)
        self.prev: dict[int, int | None] = {}  # block start -> previous physical block
        self.nxt: dict[int, int | None] = {}  # free list links
        self.prv: dict[int, int | None] = {}
        self.head: dict[tuple[int, int], int] = {}
        self.fl_bitmap = 0
        self.sl_bitmap = [0] * fllen
        self.blk_of: dict[int, int] = {}  # payload ptr -> block start
        shift = 4 + fllen
        max_pool = (1 << shift) if shift < USIZE_BITS else None
        cursor, left = start, length
        while left >= GRAN * 2:
            chunk = min(left, max_pool) if max_pool else left
            self.bsize[cursor] = chunk - GRAN
            self.prev[cursor] = None
            sentinel = cursor + chunk - GRAN
            self.bsize[sentinel] = GRAN
            self.used.add(sentinel)
            self.prev[sentinel] = cursor
            self._link(cursor, chunk - GRAN)
            left -= chunk
            cursor += chunk

    # ---- free lists

    def _link(self, blk: int, size: int):
        fl, sl = map_floor(size)
        key = (fl, sl)
        old = self.head.get(key)
        self.head[key] = blk
        self.nxt[blk] = old
        self.prv[blk] = None
        if old is not None:
            self.prv[old] = blk
        self.fl_bitmap |= 1 << fl
        self.sl_bitmap[fl] |= 1 << sl

    def _unlink(self, blk: int, size: int):
        n, p = self.nxt.pop(blk), self.prv.pop(blk)
        if n is not None:
            self.prv[n] = p
        if p is not None:
            self.nxt[p] = n
        else:
            fl, sl = map_floor(size)
            self.head[(fl, sl)] = n
            if n is None:
                del self.head[(fl, sl)]
                self.sl_bitmap[fl] &= ~(1 << sl)
                if not self.sl_bitmap[fl]:
                    self.fl_bitmap &= ~(1 << fl)

    def _search(self, search: int):
        fl, sl = map_ceil(search)
        if fl >= self.fllen:
            return None
        m = self.sl_bitmap[fl] >> sl
        if m:
            return fl, sl + ((m & -m).bit_length() - 1)
        m = self.fl_bitmap >> (fl + 1)
        if not m:
            return None
        fl = fl + 1 + ((m & -m).bit_length() - 1)
        if fl >= self.fllen:
            return None
        s = self.sl_bitmap[fl]
        return fl, (s & -s).bit_length() - 1

    # ---- the allocator

    def find(self, size: int, align: int):
        """Where `allocate(size, align)` would put the payload, without
        changing anything: `(block, ptr, new_size)` or None."""
        max_overhead = max(align - HDR, 0) + HDR
        search = round16(size + max_overhead)
        hit = self._search(search)
        if hit is None:
            return None
        blk = self.head[hit]
        ptr = round_up(blk + HDR, align)
        new_size = round16(ptr - blk + size)
        return blk, ptr, new_size

    def commit(self, hit) -> int:
        blk, ptr, new_size = hit
        size = self.bsize[blk]
        self._unlink(blk, size)
        if new_size != size:
            rest = blk + new_size
            nxt_phys = blk + size
            self.bsize[rest] = size - new_size
            self.prev[rest] = blk
            self.prev[nxt_phys] = rest
            self._link(rest, size - new_size)
            self.bsize[blk] = new_size
        self.used.add(blk)
        self.blk_of[ptr] = blk
        return ptr

    def allocate(self, size: int, align: int = 4):
        hit = self.find(size, align)
        return None if hit is None else self.commit(hit)

    def carve(self, ptr: int, size: int) -> bool:
        """Force a used block whose payload is at `ptr` (header at `ptr - 8`),
        when nothing reproduces the device's choice: split the free block that
        holds it. Keeps the model in step with a trace; rlsf never does this."""
        blk = ptr - HDR
        if blk % GRAN:
            return False
        need = round16(HDR + size)
        free = self._free_containing(blk)
        if free is None:
            return False
        fsize = self.bsize[free]
        if blk + need > free + fsize:
            return False
        self._unlink(free, fsize)
        end = free + fsize
        if blk > free:
            self.bsize[free] = blk - free
            self._link(free, blk - free)
            self.prev[blk] = free
        else:
            self.prev[blk] = self.prev[free]
        self.bsize[blk] = need
        self.used.add(blk)
        if blk + need < end:
            rest = blk + need
            self.bsize[rest] = end - rest
            self.prev[rest] = blk
            self.prev[end] = rest
            self._link(rest, end - rest)
        else:
            self.prev[end] = blk
        self.blk_of[ptr] = blk
        return True

    def _free_containing(self, addr: int):
        for blk, size in self.bsize.items():
            if blk not in self.used and blk <= addr < blk + size:
                return blk
        return None

    def deallocate(self, ptr: int):
        blk = self.blk_of.pop(ptr)
        self.used.discard(blk)
        size = self.bsize[blk]
        nxt_phys = blk + size
        if nxt_phys not in self.used:
            nsize = self.bsize.pop(nxt_phys)
            self._unlink(nxt_phys, nsize)
            del self.prev[nxt_phys]
            size += nsize
        after = blk + size
        p = self.prev[blk]
        if p is not None and p not in self.used:
            psize = self.bsize[p]
            self._unlink(p, psize)
            del self.bsize[blk]
            del self.prev[blk]
            blk = p
            size += psize
        self.bsize[blk] = size
        self._link(blk, size)
        self.prev[after] = blk

    def contains(self, ptr: int) -> bool:
        return self.bottom <= ptr < self.top

    # ---- measurements

    def free_blocks(self):
        return [s for b, s in self.bsize.items() if b not in self.used]

    def largest_hole(self) -> int:
        """The largest free block's payload capacity (`max_payload_size`)."""
        sizes = self.free_blocks()
        return max(sizes) - HDR if sizes else 0

    def largest_request(self) -> int:
        """The largest `alloc(N, align 4)` this region would serve — what the
        firmware's `largest_free_block()` probe finds."""
        if not self.fl_bitmap:
            return 0
        fl = self.fl_bitmap.bit_length() - 1
        sl = self.sl_bitmap[fl].bit_length() - 1
        return list_min(fl, sl) - HDR

    def free(self) -> int:
        """esp-alloc's `free()`: the free blocks' payload capacity."""
        return sum(s - HDR for s in self.free_blocks())

    def used_bytes(self) -> int:
        """esp-alloc's `used()`: the used blocks' sizes (sentinels included)."""
        return sum(self.bsize[b] for b in self.used)

    def check(self):
        """Every invariant rlsf keeps; raises on the first broken one."""
        addrs = sorted(self.bsize)
        prev_free = False
        for i, b in enumerate(addrs):
            s = self.bsize[b]
            assert s % GRAN == 0 and s >= GRAN, (b, s)
            if i + 1 < len(addrs):
                if addrs[i + 1] == b + s:
                    assert self.prev[addrs[i + 1]] == b, (b, addrs[i + 1])
            is_free = b not in self.used
            assert not (is_free and prev_free), f"adjacent free blocks at {b:#x}"
            prev_free = is_free
