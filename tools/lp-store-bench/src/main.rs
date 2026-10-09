//! `lp-store-bench`: the storage testbed's command line. See the README.

use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use lp_nor_sim::TearModel;
use lp_store_bench::candidates::parse_candidate_spec;
use lp_store_bench::driver_double_cut::{DoubleCutParams, sweep_double_cut};
use lp_store_bench::driver_endurance::{DayShape, endurance};
use lp_store_bench::driver_exhaustive::{
    FailureRecord, SweepParams, SweepSummary, sweep_exhaustive,
};
use lp_store_bench::driver_full_flash::{FullFlashParams, FullFlashSummary, full_flash};
use lp_store_bench::driver_fuzz::{FuzzParams, FuzzSummary, fuzz};
use lp_store_bench::driver_long::{
    LongCheckpoint, LongEnd, LongParams, LongSummary, long_walk, long_walk_resumable,
};
use lp_store_bench::driver_measure::{MeasureResult, measure, min_sectors};
use lp_store_bench::driver_random::{RandomParams, random_walk};
use lp_store_bench::{CorpusSet, Reproducer, Scoreboard, WorkloadSpec, replay};

const SPIKE_CORPUS: &str =
    "~/.photomancer/planning/lp2025/2026-10-07-1858-lpfs-fit-spike/measurements/corpus";

const RESULTS_DIR: &str = "~/.photomancer/planning/lp2025/2026-10-07-2337-storage-testbed/results";

#[derive(Parser)]
#[command(about = "Race on-device store candidates through simulated power cuts")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args, Clone)]
struct Common {
    /// Candidate specs, comma-separated: `name[@dial=v+dial=v]`.
    #[arg(long, default_value = "mem")]
    candidates: String,
    /// Directory of corpus directories (c13, c20, c40, …).
    #[arg(long, default_value = SPIKE_CORPUS)]
    corpus: String,
    /// Scoreboard directory (JSONL appended).
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long, default_value_t = 8)]
    threads: usize,
    /// Partition size in 4 KiB sectors.
    #[arg(long, default_value_t = 128)]
    sectors: u32,
}

#[derive(Subcommand)]
enum Cmd {
    /// A 1–2 minute end-to-end run of every driver on small workloads.
    Smoke {
        #[command(flatten)]
        common: Common,
        /// Tear models for every driver, comma-separated, by name (see
        /// `sweep --tears`). Unset: the sweeps run the three guessed models,
        /// the double cut `random_bits`, and the walks draw from the three.
        #[arg(long)]
        tears: Option<String>,
    },
    /// Exhaustive single-cut sweep.
    Sweep {
        #[command(flatten)]
        common: Common,
        /// Workload specs `kind:corpus[@seed]`, separated by `;` (a switch
        /// names two corpora with a comma: `switch:c13,c40reuse`).
        #[arg(long, default_value = "push:c13")]
        workloads: String,
        #[arg(long, default_value = "1,2")]
        seeds: String,
        #[arg(long)]
        max_cuts: Option<u64>,
        /// Tear models, comma-separated, by name: `clean`, `byte_prefix`,
        /// `random_bits` (the default list), `calibrated` (CX1's measured
        /// tears, `lp-nor-sim`'s `calibrated_tear.rs`) and
        /// `calibrated_<zeroing|all_zero|erasing|reads_ff_weak|reads_ff>`
        /// (every torn erase forced to that measured shape).
        #[arg(long, default_value = "clean,byte_prefix,random_bits")]
        tears: String,
    },
    /// Double-cut sweep (sampled first cuts).
    Double {
        #[command(flatten)]
        common: Common,
        #[arg(long, default_value = "save:c13")]
        workloads: String,
        #[arg(long, default_value = "1")]
        seeds: String,
        /// Tear models, comma-separated, by name: `clean`, `byte_prefix`,
        /// `random_bits` (the default list), `calibrated` (CX1's measured
        /// tears, `lp-nor-sim`'s `calibrated_tear.rs`) and
        /// `calibrated_<zeroing|all_zero|erasing|reads_ff_weak|reads_ff>`
        /// (every torn erase forced to that measured shape).
        #[arg(long, default_value = "clean,byte_prefix,random_bits")]
        tears: String,
    },
    /// Model-based random walks with random cuts.
    Random {
        #[command(flatten)]
        common: Common,
        #[arg(long, default_value_t = 4)]
        seeds: u64,
        #[arg(long, default_value_t = 200)]
        steps: u64,
        #[arg(long, default_value_t = 3)]
        cut_one_in: u64,
        #[arg(long, default_value = "c13,c20")]
        corpora: String,
        /// Tear models a cut draws from, comma-separated, by name (see
        /// `sweep --tears`); default the three guessed ones.
        #[arg(long, default_value = "")]
        tears: String,
    },
    /// Long walks on one mounted store: a cut every N steps, full-state
    /// checks (and a remount) every M; GC runs per walk are reported.
    Long {
        #[command(flatten)]
        common: Common,
        /// Walks, seeds 1..=N (in parallel).
        #[arg(long, default_value_t = 4)]
        seeds: u64,
        #[arg(long, default_value_t = 100_000)]
        steps: u64,
        #[arg(long, default_value_t = 10)]
        cut_every: u64,
        #[arg(long, default_value_t = 1000)]
        check_every: u64,
        #[arg(long, default_value = "c13,c20,c40reuse,c40")]
        corpora: String,
        /// Tear models a cut draws from (see `sweep --tears`); default the
        /// three guessed ones.
        #[arg(long, default_value = "")]
        tears: String,
        /// First seed (walks run seeds `first_seed..first_seed+seeds`).
        #[arg(long, default_value_t = 1)]
        first_seed: u64,
        /// The edit mix: fill the three slots once, then saves, panel
        /// writes and re-pushes (keeps the flash near full: GC copies).
        #[arg(long)]
        edit_mix: bool,
        /// Steps per piece (draws reseeded at each multiple; a walk may
        /// pause there). 0 = one piece.
        #[arg(long, default_value_t = 0)]
        piece_steps: u64,
        /// Where paused walks keep their checkpoints (resumed when present).
        #[arg(long)]
        checkpoint_dir: Option<PathBuf>,
        /// Pause every walk at its first piece boundary past this
        /// (`+<n>[smh]` or `HH:MM`); run again to go on.
        #[arg(long)]
        until: Option<String>,
    },
    /// Fill the store with unique copies of a project until it refuses, work
    /// at the edge with cuts in every step, then free space and recover.
    FullFlash {
        #[command(flatten)]
        common: Common,
        /// Runs, seeds 1..=N (in parallel).
        #[arg(long, default_value_t = 2)]
        seeds: u64,
        #[arg(long, default_value = "c20")]
        corpus_name: String,
        #[arg(long, default_value_t = 60)]
        edge_steps: u64,
        #[arg(long, default_value_t = 16)]
        cuts_per_step: u64,
        /// Tear models, used in turn (see `sweep --tears`); default the
        /// three guessed ones.
        #[arg(long, default_value = "")]
        tears: String,
    },
    /// Mount fuzzing: garbage, mutated, cut and (T1) newer-version images.
    Fuzz {
        #[command(flatten)]
        common: Common,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value_t = 4000)]
        cases: u64,
        #[arg(long, default_value_t = 8)]
        histories: u64,
        #[arg(long, default_value_t = 30)]
        history_steps: u64,
        #[arg(long, default_value = "c13,c20")]
        corpora: String,
        /// Tear models a cut image draws from; default the three guessed
        /// ones and `calibrated`.
        #[arg(long, default_value = "")]
        tears: String,
    },
    /// Mutation testing: every T1 mutant (`lp-tree-store`'s `mutants`
    /// feature) through one fixed driver set; pass = each caught, the
    /// unmutated store clean. Without the feature, re-runs itself through
    /// `cargo run --release --features mutants`.
    Mutants {
        /// Mutant names, comma-separated (default: all; `none` = only the
        /// unmutated store, which always runs first).
        #[arg(long, default_value = "")]
        only: String,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value_t = 8)]
        threads: usize,
    },
    /// T1's GC dials (policy × reserve × codec) at partitions where c40
    /// meets GC: fault-free save/panel GC figures plus a save cut sweep.
    GcDials {
        #[command(flatten)]
        common: Common,
        #[arg(long)]
        quick: bool,
        #[arg(long, default_value_t = 32)]
        cuts_per_step: u64,
        /// Tear models (see `sweep --tears`).
        #[arg(long, default_value = "clean,byte_prefix,random_bits,calibrated")]
        tears: String,
    },
    /// Fault-free measures (and the smallest partition each workload fits).
    Measure {
        #[command(flatten)]
        common: Common,
        #[arg(long, default_value = "push:c13;push:c40;save:c40;panel:c40")]
        workloads: String,
        /// Also binary-search the smallest partition (sectors) for each.
        #[arg(long)]
        min_sectors: bool,
    },
    /// Simulated days of use (fault-free): erase spread and write amp.
    Endurance {
        #[command(flatten)]
        common: Common,
        /// Corpus pushed as project `a` (F1 cannot hold c40 at 128 sectors).
        #[arg(long, default_value = "c20")]
        corpus_name: String,
        #[arg(long, default_value_t = 30)]
        days: u32,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Re-pushes a day (0 = pushed once, never again).
        #[arg(long, default_value_t = 1)]
        pushes: u32,
        #[arg(long, default_value_t = 10)]
        saves: u32,
        #[arg(long, default_value_t = 1440)]
        panel_writes: u32,
    },
    /// The unattended overnight run (priority list until a deadline).
    Overnight {
        /// `HH:MM` local (tomorrow if already past) or `+<n>[smh]`.
        #[arg(long, default_value = "09:00")]
        until: String,
        #[arg(long, default_value = "f1,f2,s1,t1")]
        candidates: String,
        #[arg(long, default_value = SPIKE_CORPUS)]
        corpus: String,
        #[arg(long, default_value = RESULTS_DIR)]
        out: String,
        #[arg(long, default_value_t = 8)]
        threads: usize,
        #[arg(long, default_value_t = 128)]
        sectors: u32,
        /// Shrink every unit (a smoke of the runner itself).
        #[arg(long)]
        quick: bool,
        /// Tear models every cut sweep and walk uses (see `sweep --tears`).
        #[arg(long, default_value = "clean,byte_prefix,random_bits")]
        tears: String,
        /// Stop after this many rounds (default: repeat until the deadline).
        #[arg(long)]
        rounds: Option<u32>,
        /// Skip the units `--out`'s scoreboard already has done, and run
        /// every unit whole (the deadline only stops new units starting):
        /// the overnight run as foreground pieces.
        #[arg(long)]
        resume: bool,
    },
    /// Render `<out>/report.md` and `<out>/summary.json` from the scoreboard.
    Report {
        #[arg(long, default_value = RESULTS_DIR)]
        out: String,
    },
    /// Replay a failure record or reproducer (a JSON file, or `-` for stdin).
    Replay {
        file: String,
        #[arg(long, default_value = SPIKE_CORPUS)]
        corpus: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Smoke { common, tears } => smoke(&common, tears.as_deref().map(parse_tears)),
        Cmd::Sweep {
            common,
            workloads,
            seeds,
            max_cuts,
            tears,
        } => {
            let ctx = Ctx::new(&common, "sweep");
            let params = SweepParams {
                seeds: parse_list(&seeds),
                max_cuts_per_step: max_cuts,
                tears: parse_tears(&tears),
                ..Default::default()
            };
            for (cand, cfg) in ctx.candidates() {
                for spec in parse_workloads(&workloads) {
                    let wl = ctx.corpora.build(&spec).unwrap_or_else(|e| die(&e));
                    print_sweeps(&sweep_exhaustive(
                        cand.as_ref(),
                        &cfg,
                        &wl,
                        &params,
                        &ctx.sink,
                    ));
                }
            }
        }
        Cmd::Double {
            common,
            workloads,
            seeds,
            tears,
        } => {
            let ctx = Ctx::new(&common, "double");
            let params = SweepParams {
                seeds: parse_list(&seeds),
                tears: parse_tears(&tears),
                ..Default::default()
            };
            for (cand, cfg) in ctx.candidates() {
                for spec in parse_workloads(&workloads) {
                    let wl = ctx.corpora.build(&spec).unwrap_or_else(|e| die(&e));
                    print_sweeps(&sweep_double_cut(
                        cand.as_ref(),
                        &cfg,
                        &wl,
                        &params,
                        &DoubleCutParams::default(),
                        &ctx.sink,
                    ));
                }
            }
        }
        Cmd::Random {
            common,
            seeds,
            steps,
            cut_one_in,
            corpora,
            tears,
        } => {
            let ctx = Ctx::new(&common, "random");
            for (cand, cfg) in ctx.candidates() {
                let runs: Vec<_> = (1..=seeds)
                    .map(|seed| RandomParams {
                        candidate: spec_name(cand.name(), &cfg),
                        config: cfg.clone(),
                        corpora: corpora.split(',').map(String::from).collect(),
                        seed,
                        steps,
                        cut_one_in,
                        stop_at_cut: None,
                        tears: tear_names(&tears),
                    })
                    .collect();
                use rayon::prelude::*;
                let out: Vec<_> = runs
                    .par_iter()
                    .map(|p| random_walk(cand.as_ref(), p, &ctx.corpora, &ctx.sink))
                    .collect();
                for s in out {
                    println!(
                        "random {:<12} seed {:>3}: steps {:>4} cuts {:>4} failures {:>3} non-atomic {:>3} {:?} {}",
                        s.candidate,
                        s.seed,
                        s.steps_run,
                        s.cuts,
                        s.failures,
                        s.non_atomic,
                        s.kinds,
                        s.error.unwrap_or_default()
                    );
                }
            }
        }
        Cmd::Long {
            common,
            seeds,
            steps,
            cut_every,
            check_every,
            corpora,
            tears,
            first_seed,
            edit_mix,
            piece_steps,
            checkpoint_dir,
            until,
        } => {
            let ctx = Ctx::new(&common, "long");
            let deadline = until
                .as_deref()
                .map(|u| lp_store_bench::overnight::parse_deadline(u).unwrap_or_else(|e| die(&e)));
            if deadline.is_some() && (checkpoint_dir.is_none() || piece_steps == 0) {
                die("--until needs --checkpoint-dir and --piece-steps");
            }
            for (cand, cfg) in ctx.candidates() {
                let runs: Vec<_> = (first_seed..first_seed + seeds)
                    .map(|seed| LongParams {
                        candidate: cand.name().into(),
                        config: cfg.clone(),
                        corpora: corpora.split(',').map(String::from).collect(),
                        seed,
                        steps,
                        cut_every,
                        check_every,
                        tears: tear_names(&tears),
                        wear: vec![],
                        edit_mix,
                        piece_steps,
                    })
                    .collect();
                use rayon::prelude::*;
                let out: Vec<_> = runs
                    .par_iter()
                    .map(|p| {
                        long_piece(cand.as_ref(), p, &ctx, checkpoint_dir.as_deref(), deadline)
                    })
                    .collect();
                for s in out.into_iter().flatten() {
                    print_long(&s);
                }
            }
        }
        Cmd::FullFlash {
            common,
            seeds,
            corpus_name,
            edge_steps,
            cuts_per_step,
            tears,
        } => {
            let ctx = Ctx::new(&common, "full-flash");
            for (cand, cfg) in ctx.candidates() {
                use rayon::prelude::*;
                let out: Vec<_> = (1..=seeds)
                    .into_par_iter()
                    .map(|seed| {
                        let p = FullFlashParams {
                            candidate: cand.name().into(),
                            config: cfg.clone(),
                            corpus: corpus_name.clone(),
                            seed,
                            edge_steps,
                            cuts_per_step,
                            tears: tear_names(&tears),
                        };
                        full_flash(cand.as_ref(), &p, &ctx.corpora, &ctx.sink)
                    })
                    .collect();
                for s in out {
                    print_full_flash(&s);
                }
            }
        }
        Cmd::Fuzz {
            common,
            seed,
            cases,
            histories,
            history_steps,
            corpora,
            tears,
        } => {
            let ctx = Ctx::new(&common, "fuzz");
            for (cand, cfg) in ctx.candidates() {
                let p = FuzzParams {
                    candidate: cand.name().into(),
                    config: cfg.clone(),
                    corpora: corpora.split(',').map(String::from).collect(),
                    seed,
                    cases,
                    histories,
                    history_steps,
                    tears: tear_names(&tears),
                    only_case: None,
                };
                print_fuzz(&fuzz(cand.as_ref(), &p, &ctx.corpora, &ctx.sink));
            }
        }
        Cmd::Mutants { only, out, threads } => mutants(&only, out, threads),
        Cmd::GcDials {
            common,
            quick,
            cuts_per_step,
            tears,
        } => {
            let ctx = Ctx::new(&common, "gc-dials");
            let rows = lp_store_bench::driver_gc_dials::gc_dial_sweep(
                &ctx.corpora,
                &ctx.sink,
                &lp_store_bench::driver_gc_dials::gc_dial_settings(quick),
                &parse_tears(&tears),
                cuts_per_step,
            );
            println!(
                "{:<42} {:<10} {:>4} {:>8} {:>9} {:>6} {:>12} {:>9} {:>8}",
                "t1 setting [sectors]",
                "workload",
                "ok",
                "gc runs",
                "gc copies",
                "wa",
                "erases med/max",
                "cut cases",
                "failures"
            );
            for r in rows {
                println!(
                    "{:<42} {:<10} {:>4} {:>8} {:>9} {:>6.2} {:>12} {:>9} {:>8} {}",
                    format!("t1@{}[{}]", r.config.dials_label(), r.config.sectors),
                    r.workload,
                    if r.ok { "ok" } else { "FAIL" },
                    r.gc_runs,
                    r.gc_copies,
                    r.write_amp,
                    format!("{}/{}", r.erases_median, r.erases_max),
                    r.cut_cases,
                    r.cut_failures,
                    if r.cut_kinds.is_empty() {
                        r.error.clone().unwrap_or_default()
                    } else {
                        format!("{:?}", r.cut_kinds)
                    }
                );
            }
        }
        Cmd::Measure {
            common,
            workloads,
            min_sectors: search,
        } => {
            let ctx = Ctx::new(&common, "measure");
            for (cand, cfg) in ctx.candidates() {
                for spec in parse_workloads(&workloads) {
                    let wl = ctx.corpora.build(&spec).unwrap_or_else(|e| die(&e));
                    let m = measure(cand.as_ref(), &cfg, &wl);
                    ctx.sink.write("measure", &m);
                    print_measure(&m);
                    if search {
                        let min = min_sectors(cand.as_ref(), &cfg, &wl, 4, 512);
                        ctx.sink.write(
                            "min_sectors",
                            &serde_json::json!({
                                "candidate": cand.name(), "config": cfg, "workload": spec, "min_sectors": min
                            }),
                        );
                        println!("    min sectors: {min:?}");
                    }
                }
            }
        }
        Cmd::Endurance {
            common,
            corpus_name,
            days,
            seed,
            pushes,
            saves,
            panel_writes,
        } => {
            let shape = DayShape {
                pushes,
                saves,
                panel_writes,
            };
            use rayon::prelude::*;
            let ctx = Ctx::new(&common, "endurance");
            let corpus = ctx.corpora.get(&corpus_name).unwrap_or_else(|e| die(&e));
            let specs: Vec<&str> = common
                .candidates
                .split(',')
                .filter(|s| !s.is_empty())
                .collect();
            let out: Vec<(String, MeasureResult)> = specs
                .par_iter()
                .map(|spec| {
                    let (cand, cfg) =
                        parse_candidate_spec(spec, common.sectors).unwrap_or_else(|e| die(&e));
                    let m = endurance(cand.as_ref(), &cfg, &corpus, days, shape, seed);
                    (spec.to_string(), m)
                })
                .collect();
            for (spec, m) in out {
                ctx.sink.write(
                    "endurance",
                    &serde_json::json!({"days": days, "corpus": corpus.name, "spec": spec, "result": m}),
                );
                println!("{spec}:");
                print_measure(&m);
            }
        }
        Cmd::Overnight {
            until,
            candidates,
            corpus,
            out,
            threads,
            sectors,
            quick,
            tears,
            rounds,
            resume,
        } => {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build_global()
                .ok();
            let out = expand(&out);
            // Units an earlier piece finished (`--resume`).
            let done: std::collections::BTreeSet<(u32, String)> = if resume {
                lp_store_bench::read_scoreboard(&out)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|r| r["type"] == "unit" && r["status"] == "done")
                    .filter_map(|r| {
                        Some((r["round"].as_u64()? as u32, r["name"].as_str()?.to_string()))
                    })
                    .collect()
            } else {
                Default::default()
            };
            if resume {
                eprintln!("resume: {} unit(s) already done", done.len());
            }
            let sink =
                Scoreboard::open(&out).unwrap_or_else(|e| die(&format!("{}: {e}", out.display())));
            let deadline =
                lp_store_bench::overnight::parse_deadline(&until).unwrap_or_else(|e| die(&e));
            let cmd = |args: &[&str]| {
                std::process::Command::new(args[0])
                    .args(&args[1..])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default()
            };
            sink.write(
                "run_start",
                &serde_json::json!({
                    "commit": cmd(&["git", "rev-parse", "--short=10", "HEAD"]),
                    "started": cmd(&["date", "+%Y-%m-%d %H:%M:%S %Z"]),
                    "until": until, "candidates": candidates, "threads": threads,
                    "sectors": sectors, "quick": quick, "pid": std::process::id(),
                    "tears": tears, "rounds": rounds, "resume": resume,
                }),
            );
            let p = lp_store_bench::overnight::OvernightParams {
                deadline,
                candidates: candidates.split(',').map(String::from).collect(),
                sectors,
                quick,
                tears: parse_tears(&tears),
                max_round: rounds,
                done,
                whole_units: resume,
            };
            let corpora = CorpusSet::new(Some(expand(&corpus)));
            lp_store_bench::overnight::run_overnight(&p, &corpora, &sink);
            match lp_store_bench::report::render_report(&out) {
                Ok(_) => eprintln!("report: {}", out.join("report.md").display()),
                Err(e) => die(&format!("report: {e}")),
            }
        }
        Cmd::Report { out } => {
            let out = expand(&out);
            match lp_store_bench::report::render_report(&out) {
                Ok(md) => {
                    let head: String = md.lines().take(14).collect::<Vec<_>>().join("\n");
                    println!("{head}\n…\nreport: {}", out.join("report.md").display());
                }
                Err(e) => die(&format!("report: {e}")),
            }
        }
        Cmd::Replay { file, corpus } => {
            let text = if file == "-" {
                std::io::read_to_string(std::io::stdin()).unwrap_or_else(|e| die(&e.to_string()))
            } else {
                std::fs::read_to_string(&file).unwrap_or_else(|e| die(&e.to_string()))
            };
            let v: serde_json::Value =
                serde_json::from_str(&text).unwrap_or_else(|e| die(&e.to_string()));
            let r: Reproducer = match serde_json::from_value::<FailureRecord>(v.clone()) {
                Ok(f) => {
                    println!("recorded: {} — {}", f.failure.kind, f.failure.detail);
                    f.reproducer
                }
                Err(_) => serde_json::from_value(v).unwrap_or_else(|e| die(&e.to_string())),
            };
            let corpora = CorpusSet::new(Some(expand(&corpus)));
            match replay(&r, &corpora) {
                Ok(Some(f)) => {
                    println!("replayed: FAILS — {} — {}", f.kind, f.detail);
                    std::process::exit(1);
                }
                Ok(None) => println!("replayed: passes"),
                Err(e) => die(&e),
            }
        }
    }
}

struct Ctx {
    common: Common,
    corpora: CorpusSet,
    sink: Scoreboard,
}

impl Ctx {
    fn new(c: &Common, cmd: &str) -> Self {
        rayon::ThreadPoolBuilder::new()
            .num_threads(c.threads)
            .build_global()
            .ok();
        let out = c
            .out
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("target/lp-store-bench/{cmd}")));
        let sink =
            Scoreboard::open(&out).unwrap_or_else(|e| die(&format!("{}: {e}", out.display())));
        eprintln!("scoreboard: {}", out.join("scoreboard.jsonl").display());
        Self {
            common: c.clone(),
            corpora: CorpusSet::new(Some(expand(&c.corpus))),
            sink,
        }
    }

    fn candidates(
        &self,
    ) -> Vec<(
        Box<dyn lp_store_bench::Candidate>,
        lp_store_bench::CandidateConfig,
    )> {
        self.common
            .candidates
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| parse_candidate_spec(s, self.common.sectors).unwrap_or_else(|e| die(&e)))
            .collect()
    }
}

/// `name@dials` as a reproducer stores it (the candidate is rebuilt by name).
fn spec_name(name: &str, _cfg: &lp_store_bench::CandidateConfig) -> String {
    name.to_string()
}

fn smoke(c: &Common, tears: Option<Vec<TearModel>>) {
    let ctx = Ctx::new(c, "smoke");
    let have_corpus = ctx.corpora.get("c13").is_ok();
    let t0 = std::time::Instant::now();
    for (cand, cfg) in ctx.candidates() {
        println!("== {} {}", cand.name(), cfg.dials_label());
        let mut specs = vec![
            "push:syn:6:800",
            "save:syn:4:500",
            "panel:syn:4:500",
            "switch:syn:3:400,syn:5:300",
        ];
        if have_corpus {
            specs.push("push:c13");
        }
        for s in &specs {
            let spec = WorkloadSpec::parse(s).unwrap();
            let wl = ctx.corpora.build(&spec).unwrap_or_else(|e| die(&e));
            print_measure(&measure(cand.as_ref(), &cfg, &wl));
        }
        let quick = SweepParams {
            seeds: vec![1],
            max_cuts_per_step: Some(48),
            tears: tears.clone().unwrap_or_else(|| TearModel::ALL.to_vec()),
            ..Default::default()
        };
        for (s, steps) in [
            ("push:syn:6:800", None),
            ("repush:syn:4:500", None),
            ("save:syn:4:500", Some(vec![2, 3, 4])),
            ("panel:syn:4:500", Some(vec![2, 3, 4])),
            ("switch:syn:3:400,syn:5:300", None),
        ] {
            let wl = ctx.corpora.build(&WorkloadSpec::parse(s).unwrap()).unwrap();
            let p = SweepParams {
                steps,
                ..quick.clone()
            };
            print_sweeps(&sweep_exhaustive(cand.as_ref(), &cfg, &wl, &p, &ctx.sink));
        }
        let wl = ctx
            .corpora
            .build(&WorkloadSpec::parse("save:syn:4:500").unwrap())
            .unwrap();
        let p = SweepParams {
            steps: Some(vec![2]),
            seeds: vec![1],
            tears: tears.clone().unwrap_or_else(|| vec![TearModel::RandomBits]),
            ..Default::default()
        };
        print_sweeps(&sweep_double_cut(
            cand.as_ref(),
            &cfg,
            &wl,
            &p,
            &DoubleCutParams {
                first_cuts: 4,
                mount_cuts: 8,
                rerun_cuts: 4,
            },
            &ctx.sink,
        ));
        for seed in 1..=2 {
            let s = random_walk(
                cand.as_ref(),
                &RandomParams {
                    candidate: cand.name().into(),
                    config: cfg.clone(),
                    corpora: vec!["syn:3:400".into(), "syn:6:600".into()],
                    seed,
                    steps: 40,
                    cut_one_in: 3,
                    stop_at_cut: None,
                    tears: tears
                        .iter()
                        .flatten()
                        .map(|t| t.name().to_string())
                        .collect(),
                },
                &ctx.corpora,
                &ctx.sink,
            );
            println!(
                "  random seed {seed}: steps {} cuts {} failures {} {:?} {}",
                s.steps_run,
                s.cuts,
                s.failures,
                s.kinds,
                s.error.unwrap_or_default()
            );
        }
    }
    println!("smoke done in {:.1} s", t0.elapsed().as_secs_f64());
}

fn print_measure(m: &MeasureResult) {
    let w = m.workload.as_ref().map(|w| w.label()).unwrap_or_default();
    let dials = m
        .config
        .as_ref()
        .map(|c| c.dials_label())
        .filter(|d| !d.is_empty())
        .map(|d| format!("@{d}"))
        .unwrap_or_default();
    println!(
        "  measure {:<10} {:<28} {:<5} sectors end/peak {:>3}/{:>3} used {:>4} wa {:>5.2} mount {:>7} B/{:>5} reads ram {:>6} erases {}/{}/{} {}",
        format!("{}{dials}", m.candidate),
        w,
        if m.ok { "ok" } else { "FAIL" },
        m.sectors_nonblank_end,
        m.sectors_nonblank_peak,
        m.used_sectors_end
            .map(|u| u.to_string())
            .unwrap_or("-".into()),
        m.write_amp,
        m.mount_read_bytes,
        m.mount_read_calls,
        m.report.as_ref().map(|r| r.ram_bytes).unwrap_or(0),
        m.erases_min,
        m.erases_median,
        m.erases_max,
        m.error
            .clone()
            .map(|e| format!("({e} at step {:?})", m.failed_step))
            .unwrap_or_default()
    );
}

/// One walk's piece: resume from `dir`'s checkpoint if there is one, run to
/// the deadline (pausing at a piece boundary) or the end. `None` = paused.
fn long_piece(
    cand: &dyn lp_store_bench::Candidate,
    p: &LongParams,
    ctx: &Ctx,
    dir: Option<&Path>,
    deadline: Option<std::time::Instant>,
) -> Option<LongSummary> {
    let Some(dir) = dir else {
        return Some(long_walk(cand, p, &ctx.corpora, &ctx.sink));
    };
    std::fs::create_dir_all(dir).unwrap_or_else(|e| die(&e.to_string()));
    let stem = format!(
        "long-{}{}-{}-{}{}",
        p.candidate,
        p.config
            .dials_label()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>(),
        p.config.sectors,
        p.seed,
        if p.edit_mix { "-edit" } else { "" }
    );
    let (json, img) = (
        dir.join(format!("{stem}.ckpt.json")),
        dir.join(format!("{stem}.img")),
    );
    if dir.join(format!("{stem}.done.json")).exists() {
        eprintln!("{stem}: already done");
        return None;
    }
    let resume = match (std::fs::read_to_string(&json), std::fs::read(&img)) {
        (Ok(j), Ok(i)) => {
            let ck: LongCheckpoint =
                serde_json::from_str(&j).unwrap_or_else(|e| die(&e.to_string()));
            if ck.params != *p {
                die(&format!("{}: made by other params", json.display()));
            }
            eprintln!("{stem}: resuming at step {}", ck.next_step);
            Some((ck, i))
        }
        _ => None,
    };
    match long_walk_resumable(cand, p, &ctx.corpora, &ctx.sink, resume, deadline) {
        LongEnd::Paused(ck, image) => {
            eprintln!(
                "{stem}: paused at step {} (cuts {} gc runs {:?} failures {})",
                ck.next_step, ck.summary.cuts, ck.summary.gc.gc_runs, ck.summary.failures
            );
            std::fs::write(&img, image).unwrap_or_else(|e| die(&e.to_string()));
            std::fs::write(&json, serde_json::to_string(&ck).unwrap())
                .unwrap_or_else(|e| die(&e.to_string()));
            None
        }
        LongEnd::Done(s) => {
            let _ = std::fs::remove_file(&img);
            let _ = std::fs::remove_file(&json);
            std::fs::write(
                dir.join(format!("{stem}.done.json")),
                serde_json::to_string(&s).unwrap(),
            )
            .unwrap_or_else(|e| die(&e.to_string()));
            Some(s)
        }
    }
}

fn print_long(s: &LongSummary) {
    let opt = |v: Option<u64>| v.map(|v| v.to_string()).unwrap_or("-".into());
    println!(
        "long {:<10} [{}] seed {:>3}: steps {:>6} no-space {:>5} cuts {:>5} landed {:>5} torn erases {:>4} checks {:>3} failures {} non-atomic {:>4} gc runs {:>6} gc copies {:>7} retired {} erases {:>7} (max/sector {}) {:?}{}",
        format!(
            "{}{}",
            s.candidate,
            s.config
                .as_ref()
                .map(|c| c.dials_label())
                .filter(|d| !d.is_empty())
                .map(|d| format!("@{d}"))
                .unwrap_or_default()
        ),
        s.config.as_ref().map(|c| c.sectors).unwrap_or(0),
        s.seed,
        s.steps_run,
        s.steps_no_space,
        s.cuts,
        s.landed,
        s.torn_erases,
        s.checks,
        s.failures,
        s.non_atomic,
        opt(s.gc.gc_runs),
        opt(s.gc.gc_copies),
        opt(s.gc.retired_sectors),
        s.gc.erases_total,
        s.gc.erases_max,
        s.kinds,
        s.first_failure
            .as_ref()
            .map(|f| format!(" FIRST {}: {}", f.kind, f.detail))
            .or(s.error.as_ref().map(|e| format!(" ERROR {e}")))
            .unwrap_or_default()
    );
}

#[cfg(feature = "mutants")]
fn mutants(only: &str, out: Option<PathBuf>, threads: usize) {
    use lp_store_bench::driver_mutants::run_mutants;
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .ok();
    let out = out.unwrap_or_else(|| PathBuf::from("target/lp-store-bench/mutants"));
    let sink = Scoreboard::open(&out).unwrap_or_else(|e| die(&format!("{}: {e}", out.display())));
    eprintln!("scoreboard: {}", out.join("scoreboard.jsonl").display());
    let names: Vec<String> = only
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let t0 = std::time::Instant::now();
    let results = run_mutants(&names, &sink).unwrap_or_else(|e| die(&e));
    let drivers: Vec<String> = results[0]
        .drivers
        .iter()
        .map(|d| d.driver.clone())
        .collect();
    println!(
        "{:<28} {:<10} {}",
        "mutant",
        "caught by",
        drivers
            .iter()
            .map(|d| format!("{d:>11}"))
            .collect::<String>()
    );
    let mut ok = true;
    for r in &results {
        let unmutated = r.mutant == "none";
        ok &= if unmutated {
            r.failures == 0
        } else {
            r.caught_by.is_some()
        };
        println!(
            "{:<28} {:<10} {}",
            r.mutant,
            r.caught_by
                .as_deref()
                .unwrap_or(if unmutated { "(clean)" } else { "SURVIVED" }),
            r.drivers
                .iter()
                .map(|d| format!("{:>11}", format!("{}/{}", d.failures, d.cases)))
                .collect::<String>()
        );
    }
    println!(
        "(failures/cases per driver; a catch is a failure kind the unmutated store does not show in that driver; lp-nor-sim simulator)"
    );
    for r in &results {
        for d in r.drivers.iter().filter(|d| d.failures > 0) {
            println!(
                "  {} {}: {:?} new {:?} first: {}",
                r.mutant,
                d.driver,
                d.kinds,
                d.new_kinds,
                d.first.as_deref().unwrap_or("-")
            );
        }
    }
    println!(
        "mutants: {} in {:.0} s",
        if ok { "PASS" } else { "FAIL" },
        t0.elapsed().as_secs_f64()
    );
    if !ok {
        std::process::exit(1);
    }
}

/// Without the feature: build and run this command again with it.
#[cfg(not(feature = "mutants"))]
fn mutants(only: &str, out: Option<PathBuf>, threads: usize) {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let mut cmd = std::process::Command::new(cargo);
    cmd.args([
        "run",
        "--release",
        "--manifest-path",
        manifest,
        "--features",
        "mutants",
        "--",
        "mutants",
        "--threads",
        &threads.to_string(),
    ]);
    if !only.is_empty() {
        cmd.args(["--only", only]);
    }
    if let Some(o) = out {
        cmd.arg("--out").arg(o);
    }
    eprintln!("mutants: re-running with --features mutants");
    let status = cmd.status().unwrap_or_else(|e| die(&format!("cargo: {e}")));
    std::process::exit(status.code().unwrap_or(2));
}

fn label_of(cand: &str, cfg: &Option<lp_store_bench::CandidateConfig>) -> String {
    let dials = cfg
        .as_ref()
        .map(|c| c.dials_label())
        .filter(|d| !d.is_empty())
        .map(|d| format!("@{d}"))
        .unwrap_or_default();
    format!(
        "{cand}{dials}[{}]",
        cfg.as_ref().map(|c| c.sectors).unwrap_or(0)
    )
}

fn print_full_flash(s: &FullFlashSummary) {
    let opt = |v: Option<u64>| v.map(|v| v.to_string()).unwrap_or("-".into());
    println!(
        "full-flash {:<24} {} seed {}: fill {} copies, refusals {}, edge {} ({} refused), cut cases {} ({} in refused steps) landed {} torn erases {} failures {} non-atomic {} recovered {} gc runs {} erases {} {:?}{}",
        label_of(&s.candidate, &s.config),
        s.corpus,
        s.seed,
        s.fill_slots,
        s.refusals,
        s.edge_steps,
        s.edge_refused,
        s.cases,
        s.refused_cases,
        s.landed,
        s.torn_erases,
        s.failures,
        s.non_atomic,
        s.recovered,
        opt(s.gc.gc_runs),
        s.gc.erases_total,
        s.kinds,
        s.first_failure
            .as_ref()
            .map(|f| format!(" FIRST {}: {}", f.kind, f.detail))
            .or(s.error.as_ref().map(|e| format!(" ERROR {e}")))
            .unwrap_or_default()
    );
    for f in s.failure_samples.iter().skip(1) {
        println!("    also {f}");
    }
}

fn print_fuzz(s: &FuzzSummary) {
    println!(
        "fuzz {} seed {}: cases {} failures {} {:?}{}",
        label_of(&s.candidate, &s.config),
        s.seed,
        s.cases,
        s.failures,
        s.kinds,
        s.first_failure
            .as_ref()
            .map(|f| format!(" FIRST {}: {}", f.kind, f.detail))
            .or(s.error.as_ref().map(|e| format!(" ERROR {e}")))
            .unwrap_or_default()
    );
    for (k, c) in &s.by_kind {
        println!(
            "  {k:<14} cases {:>6} mounted {:>6} refused {:>6} failures {}",
            c.cases, c.mounted, c.refused, c.failures
        );
    }
    println!("  mutations {:?}", s.mutations);
}

fn print_sweeps(out: &[SweepSummary]) {
    for s in out {
        println!(
            "  {:<10} {:<10} {:<30} {:<11} cases {:>6} landed {:>6} torn erases {:>5} failures {:>5} non-atomic {:>5} {:?}{}{}",
            s.driver,
            s.candidate,
            s.workload.as_ref().map(|w| w.label()).unwrap_or_default(),
            s.tear,
            s.cases,
            s.landed,
            s.torn_erases,
            s.failures,
            s.non_atomic,
            s.kinds,
            if s.steps_skipped.is_empty() {
                String::new()
            } else {
                format!(" skipped {:?}", s.steps_skipped)
            },
            s.error
                .as_ref()
                .map(|e| format!(" ERROR {e}"))
                .unwrap_or_default()
        );
    }
}

fn parse_list(s: &str) -> Vec<u64> {
    s.split(',').filter_map(|v| v.trim().parse().ok()).collect()
}

/// `--tears` for a walk: validated names (empty = the guessed three).
fn tear_names(s: &str) -> Vec<String> {
    parse_tears(s)
        .iter()
        .map(|t| t.name().to_string())
        .collect()
}

fn parse_tears(s: &str) -> Vec<TearModel> {
    s.split(',')
        .filter(|t| !t.trim().is_empty())
        .map(|t| {
            TearModel::from_name(t.trim())
                .unwrap_or_else(|| die(&format!("unknown tear model {t:?}")))
        })
        .collect()
}

fn parse_workloads(s: &str) -> Vec<WorkloadSpec> {
    s.split(';')
        .filter(|s| !s.is_empty())
        .map(|w| WorkloadSpec::parse(w).unwrap_or_else(|| die(&format!("bad workload {w:?}"))))
        .collect()
}

fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => Path::new(&std::env::var("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(p),
    }
}

fn die(msg: &str) -> ! {
    eprintln!("lp-store-bench: {msg}");
    std::process::exit(2)
}
