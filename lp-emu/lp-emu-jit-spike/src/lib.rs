//! spike: region JIT to wasm, run natively under wasmtime.
//!
//! THROWAWAY. See `translate` for the module shape and the planning doc
//! (`2026-09-07-0827-emu-speed-ladder/spikes/jit-region-spike.md`) for what
//! it measured. The seam it plugs into is `lp_riscv_emu::mach::region_jit`.
//!
//! What it does at install: allocates one fixed-size wasmtime memory that
//! mirrors the guest address space above `0x4000_0000`, moves every bus
//! region into it (the bus aliases the bytes from then on, so the interpreter
//! and the regions see one memory), builds the 16 KiB-page permission table,
//! and hands the hart the entry table (every block start of every region).
//!
//! Translation is lazy — at the first entry into a region — because the
//! pinned images' shader code is written by the guest at run time, without a
//! `fence.i`. Every entry re-checks the bytes of the region's RAM-resident
//! blocks against what it translated (a few hundred bytes, one `memcmp`), and
//! an emulator-side invalidation makes the next entry check the read-only
//! ones too. Any change retranslates. That is what keeps the spike exact on an
//! image that predates the firmware fence; a product build would rely on the
//! fence contract instead and drop the per-entry check.

pub mod decode;
pub mod translate;

use std::collections::HashMap;
use std::time::Instant;

use lp_emu_core::{Bus, CycleModel};
use lp_emu_esp_common::SocBus;
use lp_riscv_emu::mach::MachineHart;
use lp_riscv_emu::mach::region_jit::{RegionCx, RegionJit, RunOutcome};
use wasmtime::{Caller, Config, Engine, Func, Instance, Memory, MemoryType, Module, Store, TypedFunc};

use translate::{BlockCode, BlockEnd, GUEST_BASE, PAGES, PERM, PERM_SHIFT, RegionCode, SCRATCH};

/// One region as the census emitted it.
#[derive(Clone, Debug)]
pub struct RegionSpec {
    pub name: String,
    pub share: f64,
    pub blocks: Vec<u32>,
    /// Observed indirect-jump targets, hottest first.
    pub targets: Vec<u32>,
}

/// `region <name> <share>` then one block start per line; `target <pc>`
/// lines list observed indirect-jump targets. `#` starts a comment.
pub fn parse_region_file(text: &str) -> Result<Vec<RegionSpec>, String> {
    let mut out: Vec<RegionSpec> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts[0] {
            "region" => out.push(RegionSpec {
                name: parts.get(1).unwrap_or(&"r").to_string(),
                share: parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0),
                blocks: Vec::new(),
                targets: Vec::new(),
            }),
            "target" => {
                let pc = parse_hex(parts.get(1).copied().unwrap_or(""))
                    .ok_or_else(|| format!("line {}: bad target", n + 1))?;
                out.last_mut()
                    .ok_or_else(|| format!("line {}: target before region", n + 1))?
                    .targets
                    .push(pc);
            }
            pc => {
                let pc = parse_hex(pc).ok_or_else(|| format!("line {}: bad pc `{pc}`", n + 1))?;
                out.last_mut()
                    .ok_or_else(|| format!("line {}: block before region", n + 1))?
                    .blocks
                    .push(pc);
            }
        }
    }
    for r in &mut out {
        r.blocks.sort_unstable();
        r.blocks.dedup();
        if r.targets.is_empty() {
            // No observed targets: every block start is a candidate.
            r.targets = r.blocks.clone();
        }
    }
    Ok(out)
}

fn parse_hex(s: &str) -> Option<u32> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    u32::from_str_radix(s, 16).ok()
}

struct Host {
    bus: *mut SocBus,
}

struct Translated {
    run: TypedFunc<(i32, i64, i64), i32>,
    _instance: Instance,
    entry_index: HashMap<u32, i32>,
    /// `(guest address, bytes)` of the blocks in writable regions — checked
    /// on every entry.
    ram_bytes: Vec<(u32, Vec<u8>)>,
    /// The same for read-only regions — checked after an invalidation.
    ro_bytes: Vec<(u32, Vec<u8>)>,
    instructions: usize,
    wasm_len: usize,
    translate_us: u128,
    compile_us: u128,
}

struct RegionState {
    spec: RegionSpec,
    tr: Option<Translated>,
    entries: u64,
    ran: u64,
    retranslations: u64,
    refused: u64,
}

#[derive(Default)]
struct Stats {
    entries: u64,
    refused_pending: u64,
    refused_watch: u64,
    refused_no_entry: u64,
    after_store_exits: u64,
    ran: u64,
    time_ns: u128,
    verify_failures: u64,
}

pub struct Jit {
    engine: Engine,
    store: Store<Host>,
    memory: Memory,
    mmio_load: Func,
    mmio_store: Func,
    model: CycleModel,
    regions: Vec<RegionState>,
    by_entry: HashMap<u32, usize>,
    writable_spans: Vec<(u32, u32)>,
    suspect: bool,
    stats: Stats,
    timed: bool,
}

fn kind_read(bus: &mut SocBus, addr: u32, kind: i32) -> Result<i32, lp_emu_core::MemoryError> {
    Ok(match kind {
        0 => i32::from(bus.read_byte(addr)?),
        1 => i32::from(bus.read_halfword(addr)?),
        2 => bus.read_word(addr)?,
        4 => i32::from(bus.read_byte(addr)? as u8),
        _ => i32::from(bus.read_halfword(addr)? as u16),
    })
}

/// Install the JIT on `hart`, aliasing `bus`'s regions into its memory.
pub fn install(hart: &mut MachineHart<SocBus>, bus: &mut SocBus, specs: Vec<RegionSpec>, model: CycleModel) {
    let mut config = Config::new();
    config.cranelift_opt_level(wasmtime::OptLevel::Speed);
    let engine = Engine::new(&config).expect("wasmtime engine");
    let mut store = Store::new(
        &engine,
        Host {
            bus: std::ptr::null_mut(),
        },
    );
    let memory = Memory::new(&mut store, MemoryType::new(PAGES as u32, Some(PAGES as u32))).expect("memory");

    // Alias the bus's regions into the wasm memory.
    let base = memory.data_ptr(&store);
    bus.rebase_regions_into(base, GUEST_BASE);

    // The permission table: a page is plain RAM only when regions cover all
    // of it with one writability.
    let spans = bus.region_spans();
    let pages = 1usize << (32 - PERM_SHIFT);
    let mut covered = vec![0u32; pages];
    let mut writable_bytes = vec![0u32; pages];
    for &(b, len, w) in &spans {
        let mut at = u64::from(b);
        let end = u64::from(b) + u64::from(len);
        while at < end {
            let page_end = (at | ((1 << PERM_SHIFT) - 1)) + 1;
            let n = page_end.min(end) - at;
            let p = (at >> PERM_SHIFT) as usize;
            covered[p] += n as u32;
            if w {
                writable_bytes[p] += n as u32;
            }
            at = page_end;
        }
    }
    {
        let data = memory.data_mut(&mut store);
        let full = 1u32 << PERM_SHIFT;
        for p in 0..pages {
            data[PERM as usize + p] = if covered[p] != full {
                0
            } else if writable_bytes[p] == full {
                2
            } else if writable_bytes[p] == 0 {
                1
            } else {
                0
            };
        }
    }
    let writable_spans: Vec<(u32, u32)> = spans
        .iter()
        .filter(|s| s.2)
        .map(|s| (s.0, s.0.wrapping_add(s.1)))
        .collect();

    let mmio_load = Func::wrap(
        &mut store,
        |caller: Caller<'_, Host>, pc: i32, cyc: i64, addr: i32, kind: i32| -> i64 {
            // SAFETY: `Host::bus` is set by `run` for the duration of one call.
            let bus = unsafe { &mut *caller.data().bus };
            bus.set_issuing(pc as u32, cyc as u64);
            match kind_read(bus, addr as u32, kind) {
                Ok(v) => {
                    let st: i64 = if bus.sideband_or_yield_pending() { 2 } else { 0 };
                    (st << 32) | i64::from(v as u32)
                }
                Err(_) => 1 << 32,
            }
        },
    );
    let mmio_store = Func::wrap(
        &mut store,
        |caller: Caller<'_, Host>, pc: i32, cyc: i64, addr: i32, kind: i32, value: i32| -> i32 {
            // SAFETY: as above.
            let bus = unsafe { &mut *caller.data().bus };
            bus.set_issuing(pc as u32, cyc as u64);
            let r = match kind {
                0 => bus.write_byte(addr as u32, value as i8),
                1 => bus.write_halfword(addr as u32, value as i16),
                _ => bus.write_word(addr as u32, value),
            };
            match r {
                Ok(()) => {
                    if bus.sideband_or_yield_pending() {
                        2
                    } else {
                        0
                    }
                }
                Err(_) => 1,
            }
        },
    );

    let mut by_entry = HashMap::new();
    let mut entries = Vec::new();
    let mut regions = Vec::new();
    for (i, spec) in specs.into_iter().enumerate() {
        for &pc in &spec.blocks {
            by_entry.entry(pc).or_insert(i);
            entries.push(pc);
        }
        regions.push(RegionState {
            spec,
            tr: None,
            entries: 0,
            ran: 0,
            retranslations: 0,
            refused: 0,
        });
    }

    let jit = Jit {
        engine,
        store,
        memory,
        mmio_load,
        mmio_store,
        model,
        regions,
        by_entry,
        writable_spans,
        suspect: false,
        stats: Stats::default(),
        timed: std::env::var_os("LP_EMU_JIT_TIME").is_some(),
    };
    hart.set_region_jit(Box::new(jit), &entries);
}

impl Jit {
    fn is_writable(&self, pc: u32) -> bool {
        self.writable_spans.iter().any(|&(lo, hi)| pc >= lo && pc < hi)
    }

    /// Decode the region's blocks from the current guest bytes.
    fn build_code(&mut self, ri: usize, bus: &mut SocBus) -> RegionCode {
        let spec = &self.regions[ri].spec;
        let starts: std::collections::BTreeSet<u32> = spec.blocks.iter().copied().collect();
        let mut blocks = Vec::new();
        for &pc in &spec.blocks {
            let mut insts = Vec::new();
            let mut at = pc;
            let end;
            loop {
                if at != pc && starts.contains(&at) {
                    end = BlockEnd::Fall(at);
                    break;
                }
                if insts.len() >= 512 {
                    end = BlockEnd::Undecodable(at);
                    break;
                }
                let Ok(word) = bus.fetch_instruction(at) else {
                    end = BlockEnd::Undecodable(at);
                    break;
                };
                let Some(d) = decode::decode(word) else {
                    end = BlockEnd::Undecodable(at);
                    break;
                };
                insts.push((at, d));
                at = at.wrapping_add(u32::from(d.width));
                if d.is_control() {
                    end = BlockEnd::Term;
                    break;
                }
            }
            blocks.push(BlockCode {
                pc,
                insts,
                end,
                bytes: at.wrapping_sub(pc),
            });
        }
        let index = blocks.iter().enumerate().map(|(i, b)| (b.pc, i)).collect();
        RegionCode {
            blocks,
            index,
            targets: spec.targets.clone(),
        }
    }

    fn translate(&mut self, ri: usize, bus: &mut SocBus) {
        let t0 = Instant::now();
        let code = self.build_code(ri, bus);
        let wasm = translate::emit(&code, self.model);
        let translate_us = t0.elapsed().as_micros();
        if std::env::var_os("LP_EMU_JIT_DUMP").is_some() {
            let path = format!("target/emu-spike/{}.wasm", self.regions[ri].spec.name);
            let _ = std::fs::create_dir_all("target/emu-spike");
            let _ = std::fs::write(&path, &wasm);
        }
        let t1 = Instant::now();
        let module = match Module::new(&self.engine, &wasm) {
            Ok(m) => m,
            Err(e) => panic!(
                "jit-spike: region {} failed to compile: {e:?}",
                self.regions[ri].spec.name
            ),
        };
        let instance = Instance::new(
            &mut self.store,
            &module,
            &[self.mmio_load.into(), self.mmio_store.into(), self.memory.into()],
        )
        .expect("instantiate");
        let run = instance
            .get_typed_func::<(i32, i64, i64), i32>(&mut self.store, "run")
            .expect("run export");
        let compile_us = t1.elapsed().as_micros();

        let data = self.memory.data(&self.store);
        let mut ram_bytes = Vec::new();
        let mut ro_bytes = Vec::new();
        let mut entry_index = HashMap::new();
        let mut instructions = 0;
        for (i, b) in code.blocks.iter().enumerate() {
            let off = b.pc.wrapping_sub(GUEST_BASE) as usize;
            let bytes = data[off..off + b.bytes as usize].to_vec();
            if self.is_writable(b.pc) {
                ram_bytes.push((b.pc, bytes));
            } else {
                ro_bytes.push((b.pc, bytes));
            }
            if !b.insts.is_empty() {
                entry_index.insert(b.pc, i as i32);
            }
            instructions += b.insts.len();
        }
        log::info!(
            "jit-spike: region {} translated: {} blocks, {} instructions, {} wasm bytes, translate {} us, compile+instantiate {} us",
            self.regions[ri].spec.name,
            code.blocks.len(),
            instructions,
            wasm.len(),
            translate_us,
            compile_us
        );
        self.regions[ri].tr = Some(Translated {
            run,
            _instance: instance,
            entry_index,
            ram_bytes,
            ro_bytes,
            instructions,
            wasm_len: wasm.len(),
            translate_us,
            compile_us,
        });
    }

    fn bytes_match(&self, snapshot: &[(u32, Vec<u8>)]) -> bool {
        let data = self.memory.data(&self.store);
        snapshot.iter().all(|(pc, bytes)| {
            let off = pc.wrapping_sub(GUEST_BASE) as usize;
            &data[off..off + bytes.len()] == bytes.as_slice()
        })
    }
}

impl RegionJit<SocBus> for Jit {
    fn run(&mut self, cx: RegionCx<'_>, bus: &mut SocBus) -> RunOutcome {
        if bus.sideband_or_yield_pending() {
            self.stats.refused_pending += 1;
            return RunOutcome::Refused;
        }
        let (wlo, whi) = match bus.store_watch() {
            lp_emu_core::StoreWatch::None => (0u64, 0u64),
            lp_emu_core::StoreWatch::One { lo, hi } => (lo, hi),
            lp_emu_core::StoreWatch::Many => {
                self.stats.refused_watch += 1;
                return RunOutcome::Refused;
            }
        };
        if bus.load_watchpoints_armed() {
            self.stats.refused_watch += 1;
            return RunOutcome::Refused;
        }
        let Some(&ri) = self.by_entry.get(&cx.pc) else {
            self.stats.refused_no_entry += 1;
            return RunOutcome::Refused;
        };
        if self.suspect {
            self.suspect = false;
            for i in 0..self.regions.len() {
                let stale = match &self.regions[i].tr {
                    Some(tr) => !(self.bytes_match(&tr.ro_bytes) && self.bytes_match(&tr.ram_bytes)),
                    None => false,
                };
                if stale {
                    self.stats.verify_failures += 1;
                    self.regions[i].tr = None;
                }
            }
        }
        if self.regions[ri].tr.is_none() {
            self.translate(ri, bus);
        } else if !self.bytes_match(&self.regions[ri].tr.as_ref().unwrap().ram_bytes) {
            self.regions[ri].retranslations += 1;
            self.translate(ri, bus);
        }

        let Jit {
            store,
            memory,
            regions,
            stats,
            timed,
            ..
        } = self;
        let region = &mut regions[ri];
        let tr = region.tr.as_ref().unwrap();
        let Some(&idx) = tr.entry_index.get(&cx.pc) else {
            region.refused += 1;
            stats.refused_no_entry += 1;
            return RunOutcome::Refused;
        };

        let t0 = if *timed { Some(Instant::now()) } else { None };
        {
            let data = memory.data_mut(&mut *store);
            let s = SCRATCH as usize;
            for (r, v) in cx.regs.iter().enumerate() {
                data[s + 4 * r..s + 4 * r + 4].copy_from_slice(&v.to_le_bytes());
            }
            data[s + 140..s + 144].copy_from_slice(&0i32.to_le_bytes());
            data[s + 144..s + 152].copy_from_slice(&wlo.to_le_bytes());
            data[s + 152..s + 160].copy_from_slice(&whi.to_le_bytes());
        }
        store.data_mut().bus = bus as *mut SocBus;
        let pc = tr
            .run
            .call(&mut *store, (idx, cx.cycle_count as i64, cx.end as i64))
            .unwrap_or_else(|e| panic!("jit-spike: region {} trapped: {e:?}", region.spec.name));
        store.data_mut().bus = std::ptr::null_mut();
        let (cycle_count, ran, flag) = {
            let data = memory.data(&*store);
            let s = SCRATCH as usize;
            for (r, v) in cx.regs.iter_mut().enumerate().skip(1) {
                *v = i32::from_le_bytes(data[s + 4 * r..s + 4 * r + 4].try_into().unwrap());
            }
            (
                u64::from_le_bytes(data[s + 128..s + 136].try_into().unwrap()),
                u32::from_le_bytes(data[s + 136..s + 140].try_into().unwrap()),
                i32::from_le_bytes(data[s + 140..s + 144].try_into().unwrap()),
            )
        };
        if let Some(t0) = t0 {
            stats.time_ns += t0.elapsed().as_nanos();
        }
        stats.entries += 1;
        stats.ran += u64::from(ran);
        region.entries += 1;
        region.ran += u64::from(ran);
        if flag != 0 {
            stats.after_store_exits += 1;
        }
        RunOutcome::Ran {
            pc: pc as u32,
            cycle_count,
            instruction_count: cx.instruction_count + u64::from(ran),
            after_store: flag != 0,
        }
    }

    fn invalidate(&mut self) {
        self.suspect = true;
    }

    fn report(&self) -> String {
        let s = &self.stats;
        let mut out = format!(
            "jit-spike: {} entries, {} instructions in regions ({:.1} per entry), {} after-store exits, \
             refused {} (pending) + {} (watchpoints) + {} (no entry), {} stale-after-invalidate",
            s.entries,
            s.ran,
            if s.entries > 0 {
                s.ran as f64 / s.entries as f64
            } else {
                0.0
            },
            s.after_store_exits,
            s.refused_pending,
            s.refused_watch,
            s.refused_no_entry,
            s.verify_failures,
        );
        if self.timed {
            out.push_str(&format!(
                "; {:.3} s inside regions ({:.1} ns/instruction)",
                s.time_ns as f64 / 1e9,
                if s.ran > 0 {
                    s.time_ns as f64 / s.ran as f64
                } else {
                    0.0
                }
            ));
        }
        for r in &self.regions {
            out.push_str(&format!(
                "\n  region {} (census share {:.2}%): {} blocks, {} entries, {} instructions, {} retranslation(s), {} refused",
                r.spec.name,
                r.spec.share * 100.0,
                r.spec.blocks.len(),
                r.entries,
                r.ran,
                r.retranslations,
                r.refused
            ));
            if let Some(tr) = &r.tr {
                out.push_str(&format!(
                    "; translated {} instructions to {} wasm bytes, translate {} us, compile+instantiate {} us",
                    tr.instructions, tr.wasm_len, tr.translate_us, tr.compile_us
                ));
            } else {
                out.push_str("; never translated");
            }
        }
        out
    }
}
