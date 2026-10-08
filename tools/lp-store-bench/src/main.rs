//! `lp-store-bench`: the storage testbed's command line. See the README.

use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use lp_nor_sim::TearModel;
use lp_store_bench::candidates::parse_candidate_spec;
use lp_store_bench::driver_double_cut::{DoubleCutParams, sweep_double_cut};
use lp_store_bench::driver_exhaustive::{
    FailureRecord, SweepParams, SweepSummary, sweep_exhaustive,
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
    Smoke(Common),
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
    },
    /// Double-cut sweep (sampled first cuts).
    Double {
        #[command(flatten)]
        common: Common,
        #[arg(long, default_value = "save:c13")]
        workloads: String,
        #[arg(long, default_value = "1")]
        seeds: String,
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
        Cmd::Smoke(c) => smoke(&c),
        Cmd::Sweep {
            common,
            workloads,
            seeds,
            max_cuts,
        } => {
            let ctx = Ctx::new(&common, "sweep");
            let params = SweepParams {
                seeds: parse_list(&seeds),
                max_cuts_per_step: max_cuts,
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
        } => {
            let ctx = Ctx::new(&common, "double");
            let params = SweepParams {
                seeds: parse_list(&seeds),
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
        Cmd::Overnight {
            until,
            candidates,
            corpus,
            out,
            threads,
            sectors,
            quick,
        } => {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build_global()
                .ok();
            let out = expand(&out);
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
                }),
            );
            let p = lp_store_bench::overnight::OvernightParams {
                deadline,
                candidates: candidates.split(',').map(String::from).collect(),
                sectors,
                quick,
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

fn smoke(c: &Common) {
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
            tears: vec![TearModel::RandomBits],
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
    println!(
        "  measure {:<10} {:<28} {:<5} sectors end/peak {:>3}/{:>3} used {:>4} wa {:>5.2} mount {:>7} B/{:>5} reads ram {:>6} erases {}..{} {}",
        m.candidate,
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
        m.erases_max,
        m.error
            .clone()
            .map(|e| format!("({e} at step {:?})", m.failed_step))
            .unwrap_or_default()
    );
}

fn print_sweeps(out: &[SweepSummary]) {
    for s in out {
        println!(
            "  {:<10} {:<10} {:<30} {:<11} cases {:>6} landed {:>6} failures {:>5} non-atomic {:>5} {:?}{}{}",
            s.driver,
            s.candidate,
            s.workload.as_ref().map(|w| w.label()).unwrap_or_default(),
            s.tear,
            s.cases,
            s.landed,
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
