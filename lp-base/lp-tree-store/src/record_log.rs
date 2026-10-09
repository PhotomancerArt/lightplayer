//! The log under the tree: sectors, write heads, the RAM index, and the
//! only code that reads, programs or erases flash.

use alloc::vec;
use alloc::vec::Vec;

use crate::flash::Flash;
use crate::object_id::ObjectId;
use crate::ram_index::{RamIndex, RecordLoc};
use crate::record_header::{HeaderRead, RECORD_HEADER_LEN, RecordHeader};
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::sector_header::{HeadKind, KILLED_SECTOR_HEADER, SECTOR_HEADER_LEN, SectorHeader};
use crate::sector_table::{SectorTable, SectorUse};
use crate::store_error::StoreError;

/// Flash-level counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct LogCounters {
    pub bytes_read: u64,
    pub records_written: u64,
    pub record_bytes_written: u64,
    pub sectors_opened: u64,
    pub erases: u64,
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
    pub counters: LogCounters,
}

type R<T, F> = Result<T, StoreError<<F as Flash>::Error>>;

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
            counters: LogCounters::default(),
        }
    }

    /// Record bytes a sector holds after its header.
    pub fn sector_capacity(&self) -> u32 {
        self.sector_size - SECTOR_HEADER_LEN
    }

    pub fn addr(&self, sector: u32, offset: u32) -> u32 {
        sector * self.sector_size + offset
    }

    pub fn read(&mut self, addr: u32, buf: &mut [u8]) -> R<(), F> {
        self.counters.bytes_read += buf.len() as u64;
        self.flash.read(addr, buf).map_err(StoreError::Flash)
    }

    /// A record's header and payload, checked (CRC, and that the header is
    /// the one the index expects).
    pub fn read_raw(&mut self, loc: RecordLoc) -> R<Vec<u8>, F> {
        let mut raw = vec![0u8; loc.total_len() as usize];
        let addr = self.addr(u32::from(loc.sector), u32::from(loc.offset));
        self.read(addr, &mut raw)?;
        let mut h = [0u8; RECORD_HEADER_LEN as usize];
        h.copy_from_slice(&raw[..RECORD_HEADER_LEN as usize]);
        match RecordHeader::parse(&h) {
            HeaderRead::Record(r)
                if r.kind == loc.kind && r.codec == loc.codec && r.len == loc.len =>
            {
                if !RecordHeader::crc_ok(&h, &raw[RECORD_HEADER_LEN as usize..]) {
                    return Err(StoreError::Corrupt("record CRC"));
                }
            }
            _ => return Err(StoreError::Corrupt("record header")),
        }
        Ok(raw)
    }

    /// `id`'s payload, checked.
    pub fn read_payload(&mut self, id: ObjectId) -> R<(RecordLoc, Vec<u8>), F> {
        let loc = self
            .index
            .get(id)
            .ok_or(StoreError::Corrupt("missing record"))?;
        let mut raw = self.read_raw(loc)?;
        Ok((loc, raw.split_off(RECORD_HEADER_LEN as usize)))
    }

    /// The first `n` payload bytes, unchecked (for mark: the CRC was checked
    /// when the record was scanned or written).
    pub fn read_payload_prefix(&mut self, loc: RecordLoc, n: usize) -> R<Vec<u8>, F> {
        let n = n.min(usize::from(loc.len));
        let mut buf = vec![0u8; n];
        let addr = self.addr(
            u32::from(loc.sector),
            u32::from(loc.offset) + RECORD_HEADER_LEN,
        );
        self.read(addr, &mut buf)?;
        Ok(buf)
    }

    pub fn is_head(&self, sector: u32) -> bool {
        self.heads.contains(&Some(sector))
    }

    /// Free = not a head, not the GC victim, and holding no live byte.
    pub fn is_free(&self, s: u32) -> bool {
        if self.is_head(s) || self.gc_victim == Some(s) {
            return false;
        }
        match self.sectors.uses[s as usize] {
            SectorUse::NeedsErase | SectorUse::Erased => true,
            SectorUse::Written { .. } => self.sectors.live_bytes[s as usize] == 0,
        }
    }

    pub fn free_count(&self) -> u32 {
        (0..self.sector_count).filter(|&s| self.is_free(s)).count() as u32
    }

    /// Bytes left in a head (0 when it has none).
    pub fn head_remaining(&self, kind: HeadKind) -> u32 {
        match self.heads[kind.index()].map(|s| self.sectors.uses[s as usize]) {
            Some(SectorUse::Written { end, .. }) => self.sector_size - end,
            _ => 0,
        }
    }

    /// Append one encoded record to a head, opening a sector when it does not
    /// fit. Indexes it and counts it live.
    pub fn append(
        &mut self,
        kind: HeadKind,
        raw: &[u8],
        id: ObjectId,
        rkind: RecordKind,
        codec: ChunkCodec,
    ) -> R<RecordLoc, F> {
        let len = raw.len() as u32;
        if len > self.sector_capacity() {
            return Err(StoreError::TooLarge);
        }
        if self.head_remaining(kind) < len {
            self.open_sector(kind)?;
        }
        let s = self.heads[kind.index()].ok_or(StoreError::NoSpace)?;
        let SectorUse::Written { header, end } = self.sectors.uses[s as usize] else {
            return Err(StoreError::Corrupt("head is not written"));
        };
        let addr = self.addr(s, end);
        self.flash.program(addr, raw).map_err(StoreError::Flash)?;
        self.sectors.uses[s as usize] = SectorUse::Written {
            header,
            end: end + len,
        };
        self.sectors.live_bytes[s as usize] += len;
        let loc = RecordLoc {
            sector: s as u16,
            offset: end as u16,
            len: (len - RECORD_HEADER_LEN) as u16,
            kind: rkind,
            codec,
        };
        self.index.insert(id, loc);
        self.counters.records_written += 1;
        self.counters.record_bytes_written += u64::from(len);
        Ok(loc)
    }

    /// Make a free sector the `kind` head: kill + erase it unless it was
    /// erased this session, then program its header.
    pub fn open_sector(&mut self, kind: HeadKind) -> R<u32, F> {
        let pick = (0..self.sector_count)
            .filter(|&s| self.is_free(s))
            .min_by_key(|&s| {
                let erased = self.sectors.uses[s as usize] == SectorUse::Erased;
                (!erased, self.sectors.erase_counts[s as usize], s)
            })
            .ok_or(StoreError::NoSpace)?;
        // The previous head of this kind is closed by losing the role.
        self.heads[kind.index()] = None;
        if self.sectors.uses[pick as usize] != SectorUse::Erased {
            self.kill_and_erase(pick)?;
        }
        let header = SectorHeader {
            seq: self.next_sector_seq,
            erase_count: self.sectors.erase_counts[pick as usize],
            kind,
        };
        self.next_sector_seq = self.next_sector_seq.saturating_add(1);
        let addr = self.addr(pick, 0);
        self.flash
            .program(addr, &header.encode())
            .map_err(StoreError::Flash)?;
        self.sectors.uses[pick as usize] = SectorUse::Written {
            header,
            end: SECTOR_HEADER_LEN,
        };
        self.sectors.live_bytes[pick as usize] = 0;
        self.heads[kind.index()] = Some(pick);
        self.counters.sectors_opened += 1;
        Ok(pick)
    }

    /// Forget a sector's records, kill its header, erase it.
    pub fn kill_and_erase(&mut self, s: u32) -> R<(), F> {
        self.index.remove_sector(s);
        self.sectors.uses[s as usize] = SectorUse::NeedsErase;
        self.sectors.live_bytes[s as usize] = 0;
        let addr = self.addr(s, 0);
        self.flash
            .program(addr, &KILLED_SECTOR_HEADER)
            .map_err(StoreError::Flash)?;
        self.flash.erase_sector(s).map_err(StoreError::Flash)?;
        self.sectors.erase_counts[s as usize] =
            self.sectors.erase_counts[s as usize].wrapping_add(1);
        self.sectors.uses[s as usize] = SectorUse::Erased;
        self.counters.erases += 1;
        Ok(())
    }

    pub fn into_flash(self) -> F {
        self.flash
    }
}
