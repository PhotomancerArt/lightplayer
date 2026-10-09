//! The log under the tree: sectors, write heads, the RAM index, and the only
//! code that reads, programs or erases flash.
//!
//! Every program is read back and compared, and every erase is read back as
//! all `0xFF` (then its header is programmed and read back). A mismatch
//! **retires** the sector: it is never opened, erased or collected again
//! (the retired list rides in every root), and the record goes elsewhere.

use alloc::vec;
use alloc::vec::Vec;

use crate::flash::Flash;
use crate::object_id::ObjectId;
use crate::ram_index::{RamIndex, RecordLoc};
use crate::record_header::{HeaderRead, RECORD_HEADER_LEN, RecordHeader, encode_header};
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::sector_header::{HeadKind, KILLED_SECTOR_HEADER, SECTOR_HEADER_LEN, SectorHeader};
use crate::sector_table::{ERASED, NEEDS_ERASE, SectorTable};
use crate::store_error::StoreError;

/// Flash-level counters (feature `stats`).
#[cfg(feature = "stats")]
#[derive(Clone, Copy, Debug, Default)]
pub struct LogCounters {
    pub bytes_read: u64,
    pub records_written: u64,
    pub record_bytes_written: u64,
    pub sectors_opened: u64,
    pub erases: u64,
    pub verify_failures: u64,
    pub gc_copies: u64,
    pub gc_copy_bytes: u64,
}

/// Sectors + heads + index over a [`Flash`].
pub struct RecordLog<F: Flash> {
    pub flash: F,
    pub sector_count: u32,
    pub sector_size: u32,
    pub sectors: SectorTable,
    pub index: RamIndex,
    /// The sector each head appends to (`HeadKind::index`).
    pub heads: [Option<u32>; 2],
    pub next_sector_seq: u32,
    /// A sector being collected: never handed out as free.
    pub gc_victim: Option<u32>,
    #[cfg(feature = "stats")]
    pub counters: LogCounters,
    /// The largest transient buffer seen.
    #[cfg(feature = "stats")]
    pub largest_buffer: usize,
}

type R<T, F> = Result<T, StoreError<<F as Flash>::Error>>;

/// Write attempts per record before giving up on the flash.
const WRITE_ATTEMPTS: usize = 4;

impl<F: Flash> RecordLog<F> {
    pub fn new(flash: F) -> Self {
        let sector_count = flash.sector_count();
        let sector_size = flash.sector_size();
        Self {
            flash,
            sector_count,
            sector_size,
            sectors: SectorTable::new(sector_count),
            index: RamIndex::default(),
            heads: [None, None],
            next_sector_seq: 1,
            gc_victim: None,
            #[cfg(feature = "stats")]
            counters: LogCounters::default(),
            #[cfg(feature = "stats")]
            largest_buffer: 0,
        }
    }

    /// Record bytes a sector holds after its header.
    pub fn sector_capacity(&self) -> u32 {
        self.sector_size - SECTOR_HEADER_LEN
    }

    pub fn addr(&self, sector: u32, offset: u32) -> u32 {
        sector * self.sector_size + offset
    }

    /// A transient buffer of `bytes` (the peak `stats` reports; nothing
    /// without the feature).
    #[inline(always)]
    pub fn note(&mut self, _bytes: usize) {
        stat!(self.largest_buffer = self.largest_buffer.max(_bytes));
    }

    pub fn read(&mut self, addr: u32, buf: &mut [u8]) -> R<(), F> {
        stat!(self.counters.bytes_read += buf.len() as u64);
        self.flash.read(addr, buf).map_err(StoreError::Flash)
    }

    /// The header at `loc`, parsed (not CRC-checked against its payload).
    pub fn header_at(&mut self, loc: RecordLoc) -> R<RecordHeader, F> {
        let mut h = [0u8; RECORD_HEADER_LEN as usize];
        self.read(self.addr(loc.sector, loc.offset), &mut h)?;
        match RecordHeader::parse(&h) {
            HeaderRead::Record(r) => Ok(r),
            _ => Err(StoreError::Corrupt("record header")),
        }
    }

    /// `id`'s header and payload, checked: the header names `id`, the CRC
    /// matches.
    pub fn read_record(&mut self, id: ObjectId) -> R<(RecordHeader, Vec<u8>), F> {
        let loc = self
            .index
            .get(id)
            .ok_or(StoreError::Corrupt("missing record"))?;
        let mut raw = [0u8; RECORD_HEADER_LEN as usize];
        self.read(self.addr(loc.sector, loc.offset), &mut raw)?;
        let HeaderRead::Record(h) = RecordHeader::parse(&raw) else {
            return Err(StoreError::Corrupt("record header"));
        };
        if h.id != id {
            return Err(StoreError::Corrupt("record id"));
        }
        let mut payload = vec![0u8; usize::from(h.len)];
        self.note(payload.len());
        self.read(
            self.addr(loc.sector, loc.offset + RECORD_HEADER_LEN),
            &mut payload,
        )?;
        if !RecordHeader::crc_ok(&raw, &payload) {
            return Err(StoreError::Corrupt("record CRC"));
        }
        Ok((h, payload))
    }

    /// The first `buf.len()` payload bytes at `loc`, unchecked (a deflated
    /// chunk's logical length; the CRC was checked when it was scanned or
    /// written).
    pub fn read_payload_prefix(&mut self, loc: RecordLoc, buf: &mut [u8]) -> R<(), F> {
        self.read(self.addr(loc.sector, loc.offset + RECORD_HEADER_LEN), buf)
    }

    pub fn is_head(&self, sector: u32) -> bool {
        self.heads.contains(&Some(sector))
    }

    /// Free = not retired, not a head, not the GC victim, and holding no
    /// live byte (by the live upper bound).
    pub fn is_free(&self, s: u32) -> bool {
        !self.is_head(s)
            && self.gc_victim != Some(s)
            && (!self.sectors.is_written(s) || self.sectors.live[s as usize] == 0)
            && !self.sectors.is_retired(s)
    }

    pub fn free_count(&self) -> u32 {
        (0..self.sector_count).filter(|&s| self.is_free(s)).count() as u32
    }

    /// Bytes left in a head (0 when it has none).
    pub fn head_remaining(&self, kind: HeadKind) -> u32 {
        match self.heads[kind.index()] {
            Some(s) => self.sector_size - u32::from(self.sectors.end[s as usize]),
            None => 0,
        }
    }

    /// Append one record (payload = the concatenation of `parts`) to a head,
    /// opening a sector when it does not fit; read it back, and on a mismatch
    /// retire the sector and write it again elsewhere. Indexes it and counts
    /// it live.
    pub fn append(
        &mut self,
        kind: HeadKind,
        rkind: RecordKind,
        codec: ChunkCodec,
        id: ObjectId,
        parts: &[&[u8]],
    ) -> R<RecordLoc, F> {
        let header = encode_header(rkind, codec, id, parts);
        let total = RECORD_HEADER_LEN + parts.iter().map(|p| p.len() as u32).sum::<u32>();
        if total > self.sector_capacity() {
            return Err(StoreError::TooLarge);
        }
        for _ in 0..WRITE_ATTEMPTS {
            if self.head_remaining(kind) < total {
                self.open_sector(kind)?;
            }
            let s = self.heads[kind.index()].ok_or(StoreError::NoSpace)?;
            let end = u32::from(self.sectors.end[s as usize]);
            let addr = self.addr(s, end);
            self.program(addr, &header)?;
            let mut a = addr + RECORD_HEADER_LEN;
            for p in parts {
                self.program(a, p)?;
                a += p.len() as u32;
            }
            self.sectors.end[s as usize] = (end + total) as u16;
            if self.verify(addr, &header, parts)? {
                self.sectors.add_live(s, total);
                let loc = RecordLoc {
                    sector: s,
                    offset: end,
                };
                self.index.insert(id, loc);
                stat!(
                    self.counters.records_written += 1;
                    self.counters.record_bytes_written += u64::from(total);
                );
                return Ok(loc);
            }
            stat!(self.counters.verify_failures += 1);
            self.retire(s);
        }
        Err(StoreError::Corrupt("flash keeps failing verification"))
    }

    /// Make a free sector the `kind` head: kill + erase it unless it was
    /// erased this session, verify the erase, program its header and verify
    /// that. A sector failing either is retired and another tried.
    pub fn open_sector(&mut self, kind: HeadKind) -> R<u32, F> {
        loop {
            let pick = (0..self.sector_count)
                .filter(|&s| self.is_free(s))
                .min_by_key(|&s| {
                    let erased = self.sectors.end[s as usize] == ERASED;
                    (!erased, self.sectors.erase_count[s as usize], s)
                })
                .ok_or(StoreError::NoSpace)?;
            // The previous head of this kind is closed by losing the role.
            self.heads[kind.index()] = None;
            if self.sectors.end[pick as usize] != ERASED && !self.kill_and_erase(pick)? {
                continue;
            }
            let header = SectorHeader {
                seq: self.next_sector_seq,
                erase_count: self.sectors.erase_count[pick as usize],
                kind,
            };
            self.next_sector_seq = self.next_sector_seq.wrapping_add(1);
            let bytes = header.encode(self.sector_size);
            let addr = self.addr(pick, 0);
            // The magic last, as its own program, and only once the rest
            // reads back: a torn or worn header never shows the magic in
            // front of a wrong version (FORMAT.md "Sector": a reader refuses
            // the magic + a newer version).
            self.program(addr + 4, &bytes[4..])?;
            let mut ok = self.verify(addr + 4, &bytes[4..], &[])?;
            if ok {
                self.program(addr, &bytes[..4])?;
                ok = self.verify(addr, &bytes, &[])?;
            }
            if !ok {
                stat!(self.counters.verify_failures += 1);
                self.retire(pick);
                continue;
            }
            self.sectors.end[pick as usize] = SECTOR_HEADER_LEN as u16;
            self.sectors.live[pick as usize] = 0;
            self.sectors.seq[pick as usize] = header.seq;
            self.heads[kind.index()] = Some(pick);
            stat!(self.counters.sectors_opened += 1);
            return Ok(pick);
        }
    }

    /// Forget a sector's records, kill its header, erase it, and read it
    /// back. `false` = the erase did not take: the sector is retired.
    pub fn kill_and_erase(&mut self, s: u32) -> R<bool, F> {
        self.index.remove_sector(s);
        self.sectors.end[s as usize] = NEEDS_ERASE;
        self.sectors.live[s as usize] = 0;
        let addr = self.addr(s, 0);
        self.program(addr, &KILLED_SECTOR_HEADER)?;
        self.flash.erase_sector(s).map_err(StoreError::Flash)?;
        stat!(self.counters.erases += 1);
        let c = &mut self.sectors.erase_count[s as usize];
        *c = c.wrapping_add(1);
        if !self.reads_erased(s, 0)? {
            stat!(self.counters.verify_failures += 1);
            self.retire(s);
            return Ok(false);
        }
        self.sectors.end[s as usize] = ERASED;
        Ok(true)
    }

    /// Never use `s` again: it keeps whatever records it holds (readable,
    /// still indexed if live) but is closed to appends, never free, never
    /// erased.
    pub fn retire(&mut self, s: u32) {
        self.sectors.retire(s);
        for h in &mut self.heads {
            if *h == Some(s) {
                *h = None;
            }
        }
        if self.sectors.is_written(s) {
            self.sectors.end[s as usize] = self.sector_size as u16;
        }
    }

    /// Whether `[from .. sector end)` of `s` reads all `0xFF`.
    pub fn reads_erased(&mut self, s: u32, from: u32) -> R<bool, F> {
        let mut buf = [0u8; 256];
        let mut off = from;
        while off < self.sector_size {
            let n = (self.sector_size - off).min(256) as usize;
            self.read(self.addr(s, off), &mut buf[..n])?;
            if buf[..n].iter().any(|&b| b != 0xFF) {
                return Ok(false);
            }
            off += n as u32;
        }
        Ok(true)
    }

    fn program(&mut self, addr: u32, data: &[u8]) -> R<(), F> {
        self.flash.program(addr, data).map_err(StoreError::Flash)
    }

    /// Whether flash at `addr` reads `first ++ parts…`.
    fn verify(&mut self, addr: u32, first: &[u8], parts: &[&[u8]]) -> R<bool, F> {
        let mut buf = [0u8; 64];
        let mut a = addr;
        for p in core::iter::once(first).chain(parts.iter().copied()) {
            for chunk in p.chunks(buf.len()) {
                let got = &mut buf[..chunk.len()];
                self.read(a, got)?;
                if got != chunk {
                    return Ok(false);
                }
                a += chunk.len() as u32;
            }
        }
        Ok(true)
    }

    pub fn into_flash(self) -> F {
        self.flash
    }
}
