//! Emulator seams on the C6 machine: engaging, arming, answering (plan
//! `lp2025/2026-10-05-1026-emulator-seams`, M0 spike).
//!
//! # A separate table from the ROM hooks
//!
//! [`crate::rom::HookTable`] ships empty and stays empty; seams are their
//! own list, named in the firmware's own descriptor table and engaged only
//! when a run asks (`--seams led=fast`). With no seam asked for, nothing in
//! this file runs: no scan, no patch, no per-slice check.
//!
//! # Arming: late, exact, and again after a refill
//!
//! A seam's entry instruction is patched to `ebreak` (or `c.ebreak` when the
//! entry is a compressed instruction, so no half of a displaced instruction
//! is left behind) **in the cache window**, never in the flash chip: a
//! ROM-up boot's bootloader verifies the app image's hash, and the flash is
//! what it hashes. The window is refilled from flash whenever the MMU table
//! moves, which silently erases a patch, so arming is re-checked after every
//! fill. A patch is planted only when the cache maps the seam's address to
//! the **app's** flash bytes for it (read from the image: partition table,
//! then the ESP image header) and those bytes are what the window holds —
//! never into a page the bootloader mapped for something else.
//!
//! A switch-shape seam's **engaged byte** lives in flash `.rodata` (0 on
//! silicon) and is armed exactly like code: one byte patched to 1 in the
//! window, re-armed after a fill.

use std::collections::VecDeque;

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::seam::{
    self, ScanOutcome, ScannedTable, SeamImpl, SeamRequest, esp_app_image,
};

use crate::memmap;

/// `ebreak`, the 4-byte form.
const EBREAK: u32 = 0x0010_0073;
/// `c.ebreak`.
const C_EBREAK: u32 = 0x9002;

/// What answers when the patched site is reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// `led=fast`'s wait step: return, then park until an interrupt.
    Park,
    /// The spike's hooked engaged query (B3 mechanism (i)): `a0 = 1` for an
    /// engaged seam id.
    EngagedQuery,
    /// The wake probe's take: copy pending events into the guest's buffer.
    ProbeTake,
    /// Not code: a switch-shape seam's engaged byte (B3 mechanism (ii)).
    EngagedByte,
}

/// One patch site.
#[derive(Clone, Debug)]
pub struct SeamArm {
    pub imp: SeamImpl,
    pub answer: Answer,
    /// The patched address (a seam function's entry, or an engaged byte).
    pub vaddr: u32,
    /// Where the app's bytes for `vaddr` live in flash.
    pub paddr: u32,
    /// The displaced bytes: 1 (a byte), 2 (compressed entry) or 4.
    pub original: u32,
    pub patch: u32,
    pub patch_len: u8,
    pub armed: bool,
    /// Planted at least once before (so the next plant is a re-arm).
    pub ever_armed: bool,
}

/// What the machine holds about seams for one run.
#[derive(Clone, Debug, Default)]
pub struct SeamState {
    pub request: SeamRequest,
    pub scan: Option<ScanOutcome>,
    pub arms: Vec<SeamArm>,
    /// Calls answered (the snapshot's `seam_calls`).
    pub calls: u64,
    /// Patches planted, first arm and re-arms alike (`seam_arms`).
    pub arms_planted: u64,
    /// Of those, the ones after a fill had erased an earlier patch.
    pub rearms: u64,
    /// The guest cycle of every plant, in order (spike diagnostics: proves
    /// a ROM-up arm landed after the bootloader mapped the app).
    pub plant_cycles: Vec<u64>,
    /// Answers that parked the hart (the rest found an interrupt already
    /// pending and returned at once, exactly as `wfi` would).
    pub parks: u64,
    /// The hart is parked in a seam until an interrupt wakes it.
    pub parked: bool,
    /// Scheduler events skipped through while parked (each one a slice the
    /// guest did not run).
    pub park_events: u64,
    /// `LP_EMU_SEAM_SHALLOW_PARK`: return at the next event, as the
    /// emulator's `wfi` does, instead of at the next interrupt.
    pub shallow_park: bool,
    /// The spike's wake probe, when `probe=host` is engaged.
    pub probe: Option<ProbeDriver>,
    /// ROM-up only: nothing is armed until the hart first executes app code
    /// from the flash window. Before that the window is the bootloader's,
    /// and it **reads the app through it** to checksum and hash the image —
    /// a patch planted then fails the bootloader's checksum (found by this
    /// spike: `esp_image: Checksum failed. Calculated 0xa6 read 0xa7`, from a
    /// one-byte engaged-flag patch the mapping check alone had allowed).
    pub waiting_for_app: bool,
    /// The cycle the app was first seen running (ROM-up).
    pub app_started_at: Option<Cycles>,
}

impl SeamState {
    pub fn engaged(&self) -> bool {
        !self.arms.is_empty()
    }

    /// The armed code site claiming `pc`, if any.
    pub fn at(&self, pc: u32) -> Option<&SeamArm> {
        self.arms
            .iter()
            .find(|a| a.armed && a.vaddr == pc && a.answer != Answer::EngagedByte)
    }

    /// One `SEAM … engaged` line per engaged seam: what a run prints first.
    pub fn engaged_lines(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for imp in &self.request.engaged {
            let decl = lp_seam::SeamDecl::by_id(imp.decl_id);
            let sites: Vec<String> = self
                .arms
                .iter()
                .filter(|a| a.imp == *imp)
                .map(|a| format!("{:?}@{:#010x}", a.answer, a.vaddr))
                .collect();
            out.push(format!(
                "SEAM {}={} engaged ({}, abi {:016x}, {}; {})",
                imp.label,
                imp.implementation,
                decl.map_or("?", |d| d.kind.as_str()),
                lp_seam::SEAM_ABI_ID,
                decl.map_or("?", |d| d.symbol),
                sites.join(", ")
            ));
        }
        out
    }
}

/// Resolve `request` against the flash image. `Err` is PD5's hard error: a
/// seam explicitly asked for that cannot engage.
pub fn engage(
    request: &SeamRequest,
    flash: &[u8],
    probe_spec: Option<&str>,
) -> Result<SeamState, String> {
    let mut state = SeamState {
        request: request.clone(),
        shallow_park: std::env::var_os("LP_EMU_SEAM_SHALLOW_PARK").is_some(),
        ..SeamState::default()
    };
    if request.is_empty() {
        if probe_spec.is_some() {
            return Err("--seam-probe needs --seams probe=host".into());
        }
        if request.auto {
            // PD5, the default half: nothing is asked for explicitly, so a
            // table that cannot be read is one loud line, never an error.
            let outcome = seam::scan(flash);
            if !matches!(outcome, ScanOutcome::Found(_)) {
                eprintln!("SEAM none engaged: {outcome}");
            }
            state.scan = Some(outcome);
        }
        return Ok(state);
    }
    let outcome = seam::scan(flash);
    let table = match &outcome {
        ScanOutcome::Found(t) => t.clone(),
        other => {
            return Err(format!(
                "--seams {request} cannot engage: {other} (emulator abi {:016x})",
                lp_seam::SEAM_ABI_ID
            ));
        }
    };
    let cannot = |what: String| format!("--seams {request} cannot engage: {what}");
    for imp in &request.engaged {
        let entry_for = |id: u16| {
            table.entries.iter().find(|e| e.id == id).ok_or_else(|| {
                cannot(format!(
                    "the image's table (abi {:016x}, firmware {}) has no entry for seam {id:#06x} \
                     ({}={})",
                    table.abi, table.version, imp.label, imp.implementation
                ))
            })
        };
        let entry = entry_for(imp.decl_id)?;
        let mut sites = Vec::new();
        match imp.label {
            "probe" => {
                sites.push((Answer::ProbeTake, entry.function));
                if entry.engaged == 0 {
                    return Err(cannot("the probe entry names no engaged byte".into()));
                }
                sites.push((Answer::EngagedByte, entry.engaged));
                let query = entry_for(lp_seam::engaged::ID)?;
                sites.push((Answer::EngagedQuery, query.function));
                if table.pending == 0 {
                    return Err(cannot("the table names no pending word".into()));
                }
                state.probe = Some(ProbeDriver::new(
                    table.pending,
                    ProbeModes::parse(probe_spec.unwrap_or("steady"))?,
                ));
            }
            _ => sites.push((Answer::Park, entry.function)),
        }
        for (answer, vaddr) in sites {
            let paddr = flash_offset(flash, &table, vaddr)
                .ok_or_else(|| cannot(format!("no flash bytes for {vaddr:#010x}")))?;
            let first = flash
                .get(paddr as usize..paddr as usize + 4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                .ok_or_else(|| cannot(format!("{paddr:#x} is past the chip")))?;
            let (original, patch, patch_len) = match answer {
                Answer::EngagedByte => (first & 0xff, 1, 1),
                _ if first & 0b11 != 0b11 => (first & 0xffff, C_EBREAK, 2),
                _ => (first, EBREAK, 4),
            };
            state.arms.push(SeamArm {
                imp: *imp,
                answer,
                vaddr,
                paddr,
                original,
                patch,
                patch_len,
                armed: false,
                ever_armed: false,
            });
        }
    }
    state.scan = Some(outcome);
    Ok(state)
}

/// The flash offset of `vaddr`: through the image's own headers on a flashed
/// image, or the direct loader's linear staging when there is no header.
fn flash_offset(flash: &[u8], table: &ScannedTable, vaddr: u32) -> Option<u32> {
    if let Some((app, _)) = esp_app_image::app_partition_containing(flash, table.offset)
        && let Some(at) = esp_app_image::flash_offset_of(flash, app, vaddr)
    {
        return Some(at);
    }
    // `loader::stage_image_in_flash`: paddr = factory + (vaddr - window base).
    let offset = vaddr.checked_sub(memmap::FLASH_CACHE_BASE)?;
    Some(crate::flash::FACTORY_OFFSET + offset)
}

// ---- the spike's wake probe (part B) --------------------------------------

/// The software interrupt the probe firmware binds (`WAKE_SWI` there).
pub const PROBE_SWI: u32 = 3;
const CHANNELS: usize = 2;
const MS: Cycles = 1_000 * memmap::CYCLES_PER_US;

/// Which adversarial schedules a run injects on (B5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProbeModes {
    /// 1. one event every 1 ms, alternating channels.
    pub steady: bool,
    /// 2. every 100 ms, 64 events on 64 consecutive slice boundaries.
    pub burst: bool,
    /// 3. at a boundary where `mstatus.MIE = 0` (at most one per ms).
    pub masked: bool,
    /// 4. at a boundary where the matrix threshold masks the wake line —
    ///    inside a `Priority1` critical section such as `with_link` (≤ 1/ms).
    pub locked: bool,
    /// 5. right after a take returned events, before the next take.
    pub between: bool,
    /// 6. while the hart is parked (L1's wait or `wfi`) (≤ 1/ms).
    pub parked: bool,
}

impl ProbeModes {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut m = ProbeModes::default();
        for word in text.split(',').filter(|w| !w.is_empty()) {
            match word {
                "steady" => m.steady = true,
                "burst" => m.burst = true,
                "masked" => m.masked = true,
                "locked" => m.locked = true,
                "between" => m.between = true,
                "parked" => m.parked = true,
                "idle" => {}
                "all" => {
                    m = ProbeModes {
                        steady: true,
                        burst: true,
                        masked: true,
                        locked: true,
                        between: true,
                        parked: true,
                    }
                }
                other => return Err(format!("--seam-probe: unknown mode `{other}`")),
            }
        }
        Ok(m)
    }
}

/// The host side of the probe: a queue per channel, the injection schedule,
/// and the latency record (raise → take, emulated).
#[derive(Clone, Debug)]
pub struct ProbeDriver {
    pub pending_addr: u32,
    pub modes: ProbeModes,
    next_seq: [u32; CHANNELS],
    queues: [VecDeque<(u32, Cycles, &'static str)>; CHANNELS],
    next_steady: Cycles,
    next_burst: Cycles,
    burst_left: u32,
    next_cond: Cycles,
    between_due: bool,
    next_between: Cycles,
    rr: usize,
    /// Per mode: (injected, latency µs samples).
    pub per_mode: Vec<(&'static str, u64, Vec<f64>)>,
    pub taken: u64,
    pub takes: u64,
    pub empty_takes: u64,
    pub raises: u64,
    /// Per channel: raise → take latencies (µs).
    pub per_channel: [Vec<f64>; CHANNELS],
    /// Raise → the guest's ISR swapped the pending word to zero (µs),
    /// observed at slice boundaries: the wake itself, apart from how long
    /// the consumer's executor then took to run.
    pub isr_latency: Vec<f64>,
    /// The worst wakes: (raise cycle, µs, pc at the raise, MIE, threshold).
    pub worst: Vec<(Cycles, f64, String)>,
    raised_ctx: String,
    /// The oldest raise the guest's ISR has not yet consumed.
    raised_at: Option<Cycles>,
    /// No injection before this cycle: set 1 ms after the guest's engaged
    /// query, so the wake handler is bound and the consumers are spawned
    /// (and never into the bootloader's RAM on a ROM-up boot).
    pub start_at: Option<Cycles>,
}

impl ProbeDriver {
    fn new(pending_addr: u32, modes: ProbeModes) -> Self {
        Self {
            pending_addr,
            modes,
            next_seq: [0; CHANNELS],
            queues: Default::default(),
            next_steady: 0,
            next_burst: 0,
            burst_left: 0,
            next_cond: 0,
            between_due: false,
            next_between: 0,
            rr: 0,
            per_mode: Vec::new(),
            taken: 0,
            takes: 0,
            empty_takes: 0,
            raises: 0,
            start_at: None,
            per_channel: Default::default(),
            isr_latency: Vec::new(),
            worst: Vec::new(),
            raised_ctx: String::new(),
            raised_at: None,
        }
    }

    fn mode_slot(&mut self, mode: &'static str) -> usize {
        if let Some(i) = self.per_mode.iter().position(|(m, _, _)| *m == mode) {
            return i;
        }
        self.per_mode.push((mode, 0, Vec::new()));
        self.per_mode.len() - 1
    }

    /// Events still queued (never taken).
    pub fn left(&self) -> usize {
        self.queues.iter().map(|q| q.len()).sum()
    }

    /// The end-of-run summary.
    pub fn report(&self) -> Vec<String> {
        let mut out = vec![
            format!(
                "probe: injections began at cycle {:?} (1 ms after the guest's engaged query)",
                self.start_at
            ),
            format!(
                "probe: raises {}, takes {} ({} empty), events taken {}, left in queue {}",
                self.raises,
                self.takes,
                self.empty_takes,
                self.taken,
                self.left()
            ),
        ];
        let mut all: Vec<f64> = Vec::new();
        for (mode, injected, lat) in &self.per_mode {
            let mut l = lat.clone();
            l.sort_by(|a, b| a.partial_cmp(b).unwrap());
            all.extend(&l);
            out.push(format!(
                "probe {mode}: injected {injected}, taken {}, latency p50 {:.1} us, p99 {:.1} us, \
                 max {:.1} us",
                l.len(),
                pct(&l, 0.5),
                pct(&l, 0.99),
                l.last().copied().unwrap_or(0.0)
            ));
        }
        for (ch, lat) in self.per_channel.iter().enumerate() {
            let mut l = lat.clone();
            l.sort_by(|a, b| a.partial_cmp(b).unwrap());
            out.push(format!(
                "probe channel {ch} ({}): {} events, raise->take p50 {:.1} us, p99 {:.1} us, max {:.1} us",
                if ch == 0 { "main executor" } else { "link IO thread executor" },
                l.len(),
                pct(&l, 0.5),
                pct(&l, 0.99),
                l.last().copied().unwrap_or(0.0)
            ));
        }
        for (at, us, ctx) in &self.worst {
            out.push(format!(
                "probe worst wake: raised at {:.3} ms, consumed {us:.1} us later; at the raise: {ctx}",
                *at as f64 / (1000.0 * memmap::CYCLES_PER_US as f64)
            ));
        }
        let mut l = self.isr_latency.clone();
        l.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out.push(format!(
            "probe wake (raise -> ISR swapped the word): {} raises seen consumed, p50 {:.1} us, p99 {:.1} us, max {:.1} us",
            l.len(),
            pct(&l, 0.5),
            pct(&l, 0.99),
            l.last().copied().unwrap_or(0.0)
        ));
        all.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out.push(format!(
            "probe all: latency p50 {:.1} us, p99 {:.1} us, max {:.1} us over {} events",
            pct(&all, 0.5),
            pct(&all, 0.99),
            all.last().copied().unwrap_or(0.0),
            all.len()
        ));
        out
    }
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

impl crate::machine::Esp32C6Machine {
    /// Plant every engaged seam's patch that can be planted now, and notice
    /// any a refill erased. Idempotent; cheap (a translate and a peek per
    /// site). Called once at build, after every cache fill, and after a
    /// restore.
    pub(crate) fn arm_seams(&mut self) {
        if !self.seams.engaged() || self.seams.waiting_for_app {
            return;
        }
        for i in 0..self.seams.arms.len() {
            let arm = self.seams.arms[i].clone();
            let mapped = self.cache().lock().unwrap().translate(arm.vaddr) == Some(arm.paddr);
            let len = arm.patch_len as usize;
            let Some(now) = self.peek_code(arm.vaddr, len) else {
                self.seams.arms[i].armed = false;
                continue;
            };
            if now == arm.patch && (arm.armed || arm.ever_armed) {
                // Still ours (a fill that did not cover it, or a restore of
                // a patched window). Claimed whatever the mapping says: an
                // `ebreak` we planted must never reach the guest as its own.
                self.seams.arms[i].armed = true;
                continue;
            }
            self.seams.arms[i].armed = false;
            if !mapped || now != arm.original {
                continue;
            }
            let bytes = arm.patch.to_le_bytes();
            if self.bus.load_image(arm.vaddr, &bytes[..len]).is_ok() {
                self.seams.arms[i].armed = true;
                self.seams.arms_planted += 1;
                let now = self.cycles();
                self.seams.plant_cycles.push(now);
                if arm.ever_armed {
                    self.seams.rearms += 1;
                }
                self.seams.arms[i].ever_armed = true;
            }
        }
    }

    /// ROM-up: at a slice boundary, has the app started? It has once the
    /// hart is executing from the flash window — the ROM and the IDF
    /// bootloader never do (they run from ROM and IRAM). One compare a slice
    /// until then, nothing after.
    pub(crate) fn seams_watch_for_app(&mut self) {
        let pc = self.harts[0].pc();
        let window = memmap::FLASH_CACHE_BASE..memmap::FLASH_CACHE_BASE + crate::cache::WINDOW_LEN;
        if window.contains(&pc) {
            self.seams.waiting_for_app = false;
            self.seams.app_started_at = Some(self.cycles());
            self.arm_seams();
        }
    }

    /// Read `len` (1, 2 or 4) bytes at `vaddr` straight from the region
    /// behind it: no trace, no grade, no cost.
    fn peek_code(&self, vaddr: u32, len: usize) -> Option<u32> {
        for region in self.bus.regions() {
            if region.contains(vaddr) && region.contains(vaddr + len as u32 - 1) {
                let at = (vaddr - region.base) as usize;
                let b = &self.bus.region_bytes(region)[at..at + len];
                let mut word = [0u8; 4];
                word[..len].copy_from_slice(b);
                return Some(u32::from_le_bytes(word));
            }
        }
        None
    }

    /// The seam state, for reports and tests.
    pub fn seams(&self) -> &SeamState {
        &self.seams
    }

    /// Answer a non-park seam call (`a0..a2` in, `a0` out). The caller has
    /// already set `pc = ra`.
    pub(crate) fn answer_seam_call(&mut self, answer: Answer) {
        let regs = self.harts[0].regs();
        let (a0, a1, a2) = (regs[10] as u32, regs[11] as u32, regs[12] as u32);
        let result = match answer {
            Answer::EngagedQuery => {
                let now = self.cycles();
                if let Some(p) = self.seams.probe.as_mut()
                    && p.start_at.is_none()
                {
                    p.start_at = Some(now + MS);
                }
                let engaged = self
                    .seams
                    .request
                    .engaged
                    .iter()
                    .any(|i| u32::from(i.decl_id) == a0);
                u32::from(engaged)
            }
            Answer::ProbeTake => self.probe_take(a0 as usize, a1, a2),
            Answer::Park | Answer::EngagedByte => return,
        };
        self.harts[0].regs_mut()[10] = result as i32;
    }

    fn probe_take(&mut self, channel: usize, buf: u32, cap: u32) -> u32 {
        let now = self.cycles();
        let Some(probe) = self.seams.probe.as_mut() else {
            return 0;
        };
        probe.takes += 1;
        if channel >= CHANNELS {
            return 0;
        }
        let mut bytes = Vec::new();
        while bytes.len() + 4 <= cap as usize {
            let Some((seq, raised, mode)) = probe.queues[channel].pop_front() else {
                break;
            };
            bytes.extend_from_slice(&seq.to_le_bytes());
            probe.taken += 1;
            let slot = probe.mode_slot(mode);
            let us = (now - raised) as f64 / memmap::CYCLES_PER_US as f64;
            probe.per_mode[slot].2.push(us);
            probe.per_channel[channel].push(us);
        }
        if bytes.is_empty() {
            probe.empty_takes += 1;
            return 0;
        }
        // At most one per ms: an injection after EVERY take is a flood the
        // drain-until-0 loop never leaves (the first M0 run of this mode did
        // exactly that and starved the render), not the race it is after.
        if probe.modes.between && now >= probe.next_between {
            probe.between_due = true;
            probe.next_between = now + MS;
        }
        // The buffer the call handed us: the only guest memory a seam writes.
        if !self.poke_bytes(buf, &bytes) {
            return 0;
        }
        bytes.len() as u32
    }

    /// The probe's host side, at a slice boundary: decide whether to inject
    /// now, and if so set the pending bit and raise the wake line.
    pub(crate) fn seam_probe_tick(&mut self, now: Cycles) {
        let Some(mut probe) = self.seams.probe.take() else {
            return;
        };
        if probe.start_at.is_none_or(|at| now < at) {
            self.seams.probe = Some(probe);
            return;
        }
        // Did the guest's ISR consume the last raise? (The word reads 0.)
        if let Some(at) = probe.raised_at
            && self.peek_word(probe.pending_addr) == Some(0)
        {
            let us = (now - at) as f64 / memmap::CYCLES_PER_US as f64;
            probe.isr_latency.push(us);
            probe
                .worst
                .push((at, us, std::mem::take(&mut probe.raised_ctx)));
            probe.worst.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            probe.worst.truncate(8);
            probe.raised_at = None;
        }
        let masked = !self.harts[0].csr().mie_enabled();
        let parked = self.seams.parked || self.harts[0].is_wfi();
        let locked = self.wake_line_masked();
        let mut inject: Vec<&'static str> = Vec::new();
        let m = probe.modes;
        if m.steady && now >= probe.next_steady {
            probe.next_steady = now + MS;
            inject.push("steady");
        }
        if m.burst {
            if probe.burst_left == 0 && now >= probe.next_burst {
                probe.next_burst = now + 100 * MS;
                probe.burst_left = 64;
            }
            if probe.burst_left > 0 {
                probe.burst_left -= 1;
                inject.push("burst");
            }
        }
        if probe.between_due {
            probe.between_due = false;
            inject.push("between");
        }
        if now >= probe.next_cond {
            let cond = if m.masked && masked {
                Some("masked")
            } else if m.locked && locked {
                Some("locked")
            } else if m.parked && parked {
                Some("parked")
            } else {
                None
            };
            if let Some(mode) = cond {
                probe.next_cond = now + MS;
                inject.push(mode);
            }
        }
        let mut bits = 0u32;
        for mode in inject {
            let ch = probe.rr % CHANNELS;
            probe.rr += 1;
            let seq = probe.next_seq[ch];
            probe.next_seq[ch] += 1;
            probe.queues[ch].push_back((seq, now, mode));
            let slot = probe.mode_slot(mode);
            probe.per_mode[slot].1 += 1;
            bits |= 1 << ch;
        }
        let addr = probe.pending_addr;
        if bits != 0 {
            probe.raises += 1;
            if probe.raised_at.is_none() {
                probe.raised_at = Some(now);
                let pc = self.harts[0].pc();
                probe.raised_ctx = format!(
                    "pc {pc:#010x} {} MIE={} masked-by-threshold={locked} parked={parked}",
                    self.symbolize(pc).unwrap_or_default(),
                    !masked
                );
            }
        }
        self.seams.probe = Some(probe);
        if bits != 0 {
            // Set the bits (read-modify-write between two guest instructions
            // is atomic: the guest is not running), then raise the line the
            // way a CPU would, through INTPRI's `cpu_intr_from_cpu_3`.
            let word = self.peek_word(addr).unwrap_or(0);
            self.poke_word(addr, word | bits);
            self.poke_word(memmap::periph::INTPRI + 0x90 + 4 * PROBE_SWI, 1);
        }
    }

    /// Whether the matrix's threshold currently masks the CPU line
    /// `FROM_CPU_INTR3` routes to (a `Priority1` critical section).
    fn wake_line_masked(&self) -> bool {
        let Some(m) = self
            .bus
            .matrix()
            .as_any()
            .downcast_ref::<crate::intmatrix::Esp32C6IntMatrix>()
        else {
            return false;
        };
        let Some(line) = m.map(crate::regs::source::FROM_CPU_INTR0 + PROBE_SWI as u16) else {
            return false;
        };
        if line == 0 || line >= 32 {
            return false;
        }
        m.priority(line) < m.threshold()
    }

    /// B1: the interrupt lines in use, as the firmware configured them.
    pub fn interrupt_audit(&self) -> Vec<String> {
        let Some(m) = self
            .bus
            .matrix()
            .as_any()
            .downcast_ref::<crate::intmatrix::Esp32C6IntMatrix>()
        else {
            return vec!["interrupt audit: no C6 matrix".into()];
        };
        let mut out = vec![format!(
            "irq audit: enabled CPU lines {:#010x}, threshold {}, edge {:#010x}, mstatus.MIE {}",
            m.enable(),
            m.threshold(),
            m.kind(),
            self.harts[0].csr().mie_enabled()
        )];
        for line in 1u8..32 {
            let sources: Vec<String> = crate::regs::INTERRUPT_SOURCES
                .iter()
                .filter(|(s, _)| m.map(*s) == Some(line))
                .map(|(s, n)| format!("{n}({s})"))
                .collect();
            let enabled = m.enable() & (1 << line) != 0;
            if !enabled && sources.is_empty() {
                continue;
            }
            out.push(format!(
                "irq line {line:2}: {} priority {}  <- {}",
                if enabled { "enabled " } else { "disabled" },
                m.priority(line),
                if sources.is_empty() {
                    "-".to_string()
                } else {
                    sources.join(" ")
                }
            ));
        }
        out
    }
}
