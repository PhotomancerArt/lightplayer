//! The `flash-tears` payload's own boot flow, run on `lp-nor-sim` instead of a
//! board: what a tear model would have shown the sitting.
//!
//! ```text
//! cargo run -q -p fw-checks --features check-flash-tears --example flash_tears_on_nor_sim -- \
//!     --tear calibrated --cuts 200 --seed 1 --out target/flash-tears-sim/calibrated-1.txt
//! scripts/emu/flash-tears-analyze.py target/flash-tears-sim/calibrated-1.txt
//! ```
//! (`just flash-tears-sim` does both, for every tear model.)
//!
//! It runs exactly the firmware's flow — `runner::scan`, `runner::prepare`,
//! one `runner::timed_cycle`, `SCAN DONE`, then `runner::run_cycles` until
//! the cut — and prints the same `[fw-check-json]` records the board does,
//! into a file `scripts/emu/flash-tears-analyze.py` reads like any committed
//! transcript, with a `.meta.json` sidecar naming the configuration
//! `lp-nor-sim:<tear>`. So the analysis's classifier, not a second one, judges
//! whether a model reproduces the silicon histogram.
//!
//! **Where the cut lands mirrors the sitting:** 10–97 work cycles after the
//! scan (uniform; the sitting's spread), and within that cycle, an operation
//! drawn with probability proportional to how long it took on CX1 (the
//! report's medians: erase 24,960 µs, a page 354 µs, a journal entry 47 µs).
//! The silicon cut was uniform in time; the simulator's cut is an operation
//! index, so this weighting is what makes the two phase shares comparable.
//! The shape *inside* the torn operation is the tear model's alone.
//!
//! The output is a simulator transcript: it is never committed beside the
//! silicon ones, and no number from it is a measurement.

use std::fmt::Write as _;
use std::path::PathBuf;

use fw_checks::FW_CHECK_JSON_PREFIX;
use fw_checks::checks::flash_tears::journal;
use fw_checks::checks::flash_tears::layout::TearsLayout;
use fw_checks::checks::flash_tears::runner::{self, BootFacts, ScanBuffers, TearsFlash};
use fw_checks::checks::flash_tears::{
    JOURNAL_COPIES, LAYOUT_SECTORS, PAGES_PER_SECTOR, SCAN_DONE_MARKER, SECTOR_SIZE,
};
use lp_nor_sim::{FaultPlan, NorError, NorFlashSim, NorGeometry, SimRng, TearModel};

/// CX1's medians (µs), from the calibration report's timed cycles.
const ERASE_US: u64 = 24_960;
const PAGE_US: u64 = 354;
const JOURNAL_ENTRY_US: u64 = 47;

fn main() {
    let args = Args::parse();
    let mut flash = Sim(NorFlashSim::new(NorGeometry::c6(LAYOUT_SECTORS)));
    let layout = TearsLayout::new(0, LAYOUT_SECTORS * SECTOR_SIZE as u32).expect("layout");
    let mut bufs = Box::new(ScanBuffers::new());
    let mut rng = SimRng::new(args.seed ^ 0xF1A5_7EA2_5111_0000);
    let mut out = String::new();

    let mut next = boot(&mut flash, &layout, &mut bufs, &mut out);
    for cut in 0..args.cuts {
        let before = 10 + rng.below(88) as u32;
        let cut_after = pick_cut(&flash.0, &layout, next, before, &mut rng);
        let seed = rng.next_u64();
        flash.0.set_plan(FaultPlan::cut(cut_after, args.tear, seed));
        let err = runner::run_cycles(&mut flash, &layout, next, u32::MAX - next, &mut bufs.new);
        assert_eq!(err, Err(NorError::PowerLost), "cut {cut} never landed");
        flash.0.power_cycle(FaultPlan::none());
        next = boot(&mut flash, &layout, &mut bufs, &mut out);
    }

    if let Some(dir) = args.out.parent() {
        std::fs::create_dir_all(dir).expect("create the output directory");
    }
    std::fs::write(&args.out, out).expect("write the transcript");
    let meta = format!(
        "{{\"schema\":1,\"payload\":\"flash-tears\",\"chip\":\"esp32c6\",\
         \"configuration\":\"lp-nor-sim:{}\",\"date\":\"simulated\",\
         \"firmware_commit\":\"seed {}\",\"note\":\"lp-nor-sim, not a measurement: \
         fw-checks/examples/flash_tears_on_nor_sim.rs\"}}\n",
        args.tear.name(),
        args.seed
    );
    let mut meta_path = args.out.clone().into_os_string();
    meta_path.push(".meta.json");
    std::fs::write(meta_path, meta).expect("write the sidecar");
    eprintln!(
        "{} cuts under `{}` (seed {}) -> {}",
        args.cuts,
        args.tear.name(),
        args.seed,
        args.out.display()
    );
}

/// One boot: scan, repair, one timed cycle, `SCAN DONE`. Returns where the
/// work loop resumes.
fn boot(flash: &mut Sim, layout: &TearsLayout, bufs: &mut ScanBuffers, out: &mut String) -> u32 {
    let facts = BootFacts {
        reset: "poweron",
        mac: [0; 6],
        flash_id: 0,
    };
    let mut emit = |r: &dyn std::fmt::Display| {
        let _ = write!(out, "{FW_CHECK_JSON_PREFIX}{r}\r\n");
    };
    let found = runner::scan(flash, layout, bufs, facts, &mut emit).expect("scan");
    let next = runner::prepare(flash, layout, &found, bufs, &mut emit).expect("prepare");
    let mut t = 0u64;
    let mut now_us = || {
        t += 1;
        t
    };
    let timing =
        runner::timed_cycle(flash, layout, next, &mut bufs.new, &mut now_us).expect("timed cycle");
    emit(&timing);
    let _ = write!(out, "{SCAN_DONE_MARKER} next={}\r\n", next + 1);
    next + 1
}

/// The op index (counted from the plan) to cut: `before` whole cycles from
/// `next`, then one op of the following cycle, weighted by its silicon time.
fn pick_cut(
    sim: &NorFlashSim,
    layout: &TearsLayout,
    next: u32,
    before: u32,
    rng: &mut SimRng,
) -> u64 {
    let mut probe = Sim(sim.clone());
    probe.0.set_plan(FaultPlan::none());
    let mut buf = [0u8; SECTOR_SIZE];
    runner::run_cycles(&mut probe, layout, next, before, &mut buf).expect("probe");
    let base = probe.0.ops_since_plan();

    let target = next + before;
    let mut weights = Vec::new();
    for copy in 0..JOURNAL_COPIES {
        if journal::slot_of(copy, target) == 0 {
            weights.push(ERASE_US);
        }
        weights.push(JOURNAL_ENTRY_US);
    }
    weights.push(ERASE_US);
    weights.extend(std::iter::repeat_n(PAGE_US, PAGES_PER_SECTOR));
    let mut r = rng.below(weights.iter().sum());
    let mut op = 0;
    for (i, &w) in weights.iter().enumerate() {
        if r < w {
            op = i;
            break;
        }
        r -= w;
    }
    base + op as u64
}

struct Sim(NorFlashSim);

impl TearsFlash for Sim {
    type Error = NorError;
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), NorError> {
        self.0.read(addr, buf)
    }
    fn erase_sector(&mut self, addr: u32) -> Result<(), NorError> {
        self.0.erase_sector(addr / SECTOR_SIZE as u32)
    }
    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), NorError> {
        self.0.program(addr, data)
    }
}

struct Args {
    tear: TearModel,
    cuts: u32,
    seed: u64,
    out: PathBuf,
}

impl Args {
    fn parse() -> Self {
        let mut a = Args {
            tear: TearModel::Calibrated,
            cuts: 200,
            seed: 1,
            out: PathBuf::from("target/flash-tears-sim/calibrated-1.txt"),
        };
        let mut it = std::env::args().skip(1);
        while let Some(flag) = it.next() {
            let mut value = || {
                it.next()
                    .unwrap_or_else(|| usage(&format!("{flag} needs a value")))
            };
            match flag.as_str() {
                "--tear" => {
                    let v = value();
                    a.tear = TearModel::from_name(&v)
                        .unwrap_or_else(|| usage(&format!("unknown tear model `{v}`")));
                }
                "--cuts" => a.cuts = value().parse().unwrap_or_else(|_| usage("--cuts N")),
                "--seed" => a.seed = value().parse().unwrap_or_else(|_| usage("--seed N")),
                "--out" => a.out = PathBuf::from(value()),
                _ => usage(&format!("unknown argument `{flag}`")),
            }
        }
        a
    }
}

fn usage(msg: &str) -> ! {
    eprintln!(
        "flash_tears_on_nor_sim: {msg}\n\
         usage: flash_tears_on_nor_sim [--tear clean|byte_prefix|random_bits|calibrated] \
         [--cuts N] [--seed N] [--out PATH]"
    );
    std::process::exit(2)
}
