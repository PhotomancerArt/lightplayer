//! The reference candidate: the whole store as one blob in RAM, written to
//! flash on commit. Deliberately naive — it exists to test the harness.
//!
//! `PingPong` keeps two slots (each half the sectors) and writes the next blob
//! into the slot it is not using, with a sequence number and a CRC: correct
//! under every cut. `InPlace` (the broken twin, `mem-broken`) erases its one
//! slot and rewrites it: a cut between erase and write loses everything.

use std::collections::BTreeMap;

use lp_nor_sim::{NorError, NorFlashSim};

use crate::{Candidate, CandidateConfig, CandidateReport, CandidateStore, StoreError};

const MAGIC: u32 = 0x4D45_4D31; // "MEM1"
const HEADER: usize = 16; // magic, seq, len, crc

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemLayout {
    PingPong,
    InPlace,
}

pub struct MemCandidate {
    layout: MemLayout,
}

impl MemCandidate {
    pub fn new(layout: MemLayout) -> Self {
        Self { layout }
    }

    fn slots(&self, cfg: &CandidateConfig) -> Vec<(u32, u32)> {
        match self.layout {
            MemLayout::PingPong => {
                let half = cfg.sectors / 2;
                vec![(0, half), (half, half)]
            }
            MemLayout::InPlace => vec![(0, cfg.sectors)],
        }
    }
}

impl Candidate for MemCandidate {
    fn name(&self) -> &str {
        match self.layout {
            MemLayout::PingPong => "mem",
            MemLayout::InPlace => "mem-broken",
        }
    }

    fn format(&self, flash: &mut NorFlashSim, cfg: &CandidateConfig) -> Result<(), StoreError> {
        // Every slot erased: an older blob with a higher sequence must not
        // outlive a format (the mount fuzz formats over arbitrary images).
        for s in 0..cfg.sectors {
            flash.erase_sector(s).map_err(flash_err)?;
        }
        let mut store = MemStore {
            flash: flash.clone(),
            slots: self.slots(cfg),
            active: None,
            seq: 0,
            map: BTreeMap::new(),
        };
        store.commit()?;
        *flash = store.flash;
        Ok(())
    }

    fn mount(
        &self,
        mut flash: NorFlashSim,
        cfg: &CandidateConfig,
    ) -> Result<Box<dyn CandidateStore>, (StoreError, NorFlashSim)> {
        let slots = self.slots(cfg);
        let mut best: Option<(usize, u32, BTreeMap<String, Vec<u8>>)> = None;
        for (i, &(start, _)) in slots.iter().enumerate() {
            match read_slot(&mut flash, start, slot_bytes(&slots[i])) {
                Ok(Some((seq, map))) => {
                    if best.as_ref().is_none_or(|b| seq > b.1) {
                        best = Some((i, seq, map));
                    }
                }
                Ok(None) => {}
                Err(e) => return Err((e, flash)),
            }
        }
        let Some((active, seq, map)) = best else {
            return Err((StoreError::Corrupt("no valid slot".into()), flash));
        };
        Ok(Box::new(MemStore {
            flash,
            slots,
            active: Some(active),
            seq,
            map,
        }))
    }
}

struct MemStore {
    flash: NorFlashSim,
    slots: Vec<(u32, u32)>,
    active: Option<usize>,
    seq: u32,
    map: BTreeMap<String, Vec<u8>>,
}

fn slot_bytes(slot: &(u32, u32)) -> usize {
    slot.1 as usize * 4096
}

fn flash_err(e: NorError) -> StoreError {
    match e {
        NorError::PowerLost => StoreError::PowerLost,
        e => StoreError::Other(format!("{e:?}")),
    }
}

fn read_slot(
    flash: &mut NorFlashSim,
    start: u32,
    cap: usize,
) -> Result<Option<(u32, BTreeMap<String, Vec<u8>>)>, StoreError> {
    let mut h = [0u8; HEADER];
    flash.read(start * 4096, &mut h).map_err(flash_err)?;
    let word = |i: usize| u32::from_le_bytes(h[i * 4..i * 4 + 4].try_into().unwrap());
    let (magic, seq, len, crc) = (word(0), word(1), word(2) as usize, word(3));
    if magic != MAGIC || len > cap - HEADER {
        return Ok(None);
    }
    let mut body = vec![0u8; len];
    flash
        .read(start * 4096 + HEADER as u32, &mut body)
        .map_err(flash_err)?;
    if lp_crc32::crc32(&body) != crc {
        return Ok(None);
    }
    Ok(decode(&body).map(|m| (seq, m)))
}

fn encode(map: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut out = Vec::new();
    for (p, b) in map {
        out.extend_from_slice(&(p.len() as u16).to_le_bytes());
        out.extend_from_slice(p.as_bytes());
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    }
    out
}

fn decode(mut b: &[u8]) -> Option<BTreeMap<String, Vec<u8>>> {
    let mut map = BTreeMap::new();
    while !b.is_empty() {
        let pl = u16::from_le_bytes(b.get(..2)?.try_into().ok()?) as usize;
        let p = String::from_utf8(b.get(2..2 + pl)?.to_vec()).ok()?;
        b = &b[2 + pl..];
        let dl = u32::from_le_bytes(b.get(..4)?.try_into().ok()?) as usize;
        let d = b.get(4..4 + dl)?.to_vec();
        b = &b[4 + dl..];
        map.insert(p, d);
    }
    Some(map)
}

impl CandidateStore for MemStore {
    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.map.insert(path.into(), bytes.to_vec());
        Ok(())
    }

    fn get(&mut self, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.map.get(path).cloned())
    }

    fn delete_prefix(&mut self, prefix: &str) -> Result<(), StoreError> {
        self.map.retain(|p, _| !p.starts_with(prefix));
        Ok(())
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StoreError> {
        Ok(self
            .map
            .keys()
            .filter(|p| p.starts_with(prefix))
            .cloned()
            .collect())
    }

    fn commit(&mut self) -> Result<(), StoreError> {
        let target = match self.active {
            Some(a) => (a + 1) % self.slots.len(),
            None => 0,
        };
        let (start, count) = self.slots[target];
        let body = encode(&self.map);
        if body.len() + HEADER > slot_bytes(&self.slots[target]) {
            return Err(StoreError::NoSpace);
        }
        let used = (body.len() + HEADER).div_ceil(4096) as u32;
        for s in start..start + used.min(count) {
            self.flash.erase_sector(s).map_err(flash_err)?;
        }
        let base = start * 4096;
        self.flash
            .program(base + HEADER as u32, &body)
            .map_err(flash_err)?;
        let seq = self.seq.wrapping_add(1);
        let mut h = Vec::with_capacity(HEADER);
        for w in [MAGIC, seq, body.len() as u32, lp_crc32::crc32(&body)] {
            h.extend_from_slice(&w.to_le_bytes());
        }
        self.flash.program(base, &h).map_err(flash_err)?;
        self.seq = seq;
        self.active = Some(target);
        Ok(())
    }

    fn into_flash(self: Box<Self>) -> NorFlashSim {
        self.flash
    }

    fn flash_snapshot(&self) -> NorFlashSim {
        self.flash.clone()
    }

    fn report(&self) -> CandidateReport {
        CandidateReport {
            ram_bytes: self
                .map
                .iter()
                .map(|(p, b)| (p.len() + b.len()) as u64)
                .sum(),
            used_sectors: None,
            step_atomic: true,
            extra: Default::default(),
        }
    }
}
