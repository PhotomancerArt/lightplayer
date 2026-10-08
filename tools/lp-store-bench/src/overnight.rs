//! The unattended overnight run: a priority list of work units, checked
//! against a deadline between units, every result appended to the scoreboard
//! as it lands (a crash or the deadline loses nothing). Rounds repeat the cut
//! sweeps and random walks with new seeds while time remains.

use std::time::{Duration, Instant};

use lp_nor_sim::TearModel;
use rayon::prelude::*;
use serde::Serialize;

use crate::candidates::parse_candidate_spec;
use crate::driver_double_cut::{DoubleCutParams, sweep_double_cut};
use crate::driver_endurance::{DayShape, endurance};
use crate::driver_exhaustive::{SweepParams, sweep_exhaustive};
use crate::driver_fill::{fill_slots, largest_project};
use crate::driver_measure::{measure, min_sectors};
use crate::driver_random::{RandomParams, random_walk};
use crate::{CandidateConfig, CorpusSet, Scoreboard, WorkloadKind, WorkloadSpec};

/// What to run, until when.
pub struct OvernightParams {
    pub deadline: Instant,
    /// Candidate specs (`f1`, `t1@codec=stored`, …).
    pub candidates: Vec<String>,
    pub sectors: u32,
    /// Shrink everything (smoke): fewer cut points, days, dial settings.
    pub quick: bool,
}

#[derive(Serialize)]
struct UnitRecord<'a> {
    priority: u32,
    round: u32,
    name: &'a str,
    status: &'a str,
    elapsed_s: f64,
}

/// A unit of work: priority, name, and the job.
type Unit<'a> = (u32, String, Box<dyn FnOnce() + Send + 'a>);

/// The corpora the workloads use, per candidate: F1 cannot hold c40, so it
/// runs every c40 workload on c20 instead.
fn big_corpus(cand: &str) -> &'static str {
    if cand == "f1" { "c20" } else { "c40" }
}

pub fn run_overnight(p: &OvernightParams, corpora: &CorpusSet, sink: &Scoreboard) {
    let started = Instant::now();
    log(&format!(
        "overnight: {} candidates, deadline in {:.0} min",
        p.candidates.len(),
        p.deadline.saturating_duration_since(started).as_secs_f64() / 60.0
    ));
    let mut not_run: Vec<String> = Vec::new();
    let mut round = 1;
    loop {
        let units = round_units(p, corpora, sink, round);
        if units.is_empty() {
            break;
        }
        let mut iter = units.into_iter();
        for (prio, name, job) in iter.by_ref() {
            if Instant::now() >= p.deadline {
                not_run.push(format!("round {round} P{prio} {name}"));
                break;
            }
            let t = Instant::now();
            log(&format!("round {round} P{prio} start {name}"));
            let status = match crate::catch_quiet(job) {
                Ok(()) => "done",
                Err(e) => {
                    log(&format!("  harness panic: {e}"));
                    "panicked"
                }
            };
            let el = t.elapsed().as_secs_f64();
            log(&format!(
                "round {round} P{prio} {status} {name} ({el:.1} s)"
            ));
            sink.write(
                "unit",
                &UnitRecord {
                    priority: prio,
                    round,
                    name: &name,
                    status,
                    elapsed_s: el,
                },
            );
        }
        not_run.extend(iter.map(|(prio, name, _)| format!("round {round} P{prio} {name}")));
        if Instant::now() >= p.deadline {
            break;
        }
        round += 1;
    }
    sink.write(
        "overnight_end",
        &serde_json::json!({
            "rounds": round,
            "elapsed_s": started.elapsed().as_secs_f64(),
            "not_run": not_run,
        }),
    );
    log(&format!(
        "overnight: finished after {round} round(s), {:.1} min; {} unit(s) not run",
        started.elapsed().as_secs_f64() / 60.0,
        not_run.len()
    ));
}

fn round_units<'a>(
    p: &'a OvernightParams,
    corpora: &'a CorpusSet,
    sink: &'a Scoreboard,
    round: u32,
) -> Vec<Unit<'a>> {
    let mut units: Vec<Unit<'a>> = Vec::new();
    let cands = &p.candidates;
    let deadline = p.deadline;
    let seeds = vec![2 * round as u64 - 1, 2 * round as u64];
    // Round 1 samples at most ROUND1_CUTS cut points a step so every
    // priority gets a turn (one candidate's exhaustive sweep can take hours);
    // round 2 and every odd round after it sweep every cut point.
    let max_cuts = |cand: &str| {
        if p.quick {
            Some(12)
        } else if round == 1 {
            Some(if cand == "s1" {
                ROUND1_CUTS_S1
            } else {
                ROUND1_CUTS
            })
        } else {
            None
        }
    };
    let steps_for = move |wl: &crate::Workload| {
        if p.quick {
            quick_steps(p, wl)
        } else if round == 1 {
            round1_steps(wl)
        } else {
            None
        }
    };
    let sweeping = round <= 2 || round % 2 == 1;
    // Uncapped rounds run the fastest candidates first.
    let mut cands = cands.clone();
    if round > 1 {
        cands.sort_by_key(|c| speed_rank(c));
    }
    let cands = &cands;
    if round == 1 {
        // P1: fault-free measures.
        for c in cands {
            let c = c.clone();
            units.push((
                1,
                format!("measure {c}"),
                Box::new(move || measures(p, corpora, sink, &c)),
            ));
        }
    }
    if sweeping {
        // P2: single cuts; workloads interleaved across candidates.
        for kind in WorkloadKind::ALL {
            for c in cands {
                let spec = workload_for(kind, c, round as u64);
                for tear in TearModel::ALL {
                    let (c, spec, seeds) = (c.clone(), spec.clone(), seeds.clone());
                    let max_cuts = max_cuts(&c);
                    units.push((
                        2,
                        format!("sweep {c} {} {}", spec.label(), tear.name()),
                        Box::new(move || {
                            let Ok((cand, cfg)) = parse_candidate_spec(&c, p.sectors) else {
                                return;
                            };
                            let Ok(wl) = corpora.build(&spec) else { return };
                            let params = SweepParams {
                                tears: vec![tear],
                                seeds,
                                max_cuts_per_step: max_cuts,
                                steps: steps_for(&wl),
                                deadline: Some(deadline),
                                ..Default::default()
                            };
                            sweep_exhaustive(cand.as_ref(), &cfg, &wl, &params, sink);
                        }),
                    ));
                }
            }
        }
        // P2b: the same cuts at a tight partition, so they land in GC.
        for kind in [WorkloadKind::Save, WorkloadKind::Panel] {
            for c in cands {
                let Some(tight) = tight_sectors(c) else {
                    continue;
                };
                let spec = workload_for(kind, c, round as u64);
                for tear in TearModel::ALL {
                    let (c, spec, seeds) = (c.clone(), spec.clone(), seeds.clone());
                    let max_cuts = max_cuts(&c);
                    units.push((
                        2,
                        format!("sweep {c}[{tight}] {} {}", spec.label(), tear.name()),
                        Box::new(move || {
                            let Ok((cand, cfg)) = parse_candidate_spec(&c, tight) else {
                                return;
                            };
                            let Ok(wl) = corpora.build(&spec) else { return };
                            let params = SweepParams {
                                tears: vec![tear],
                                seeds,
                                max_cuts_per_step: max_cuts,
                                steps: steps_for(&wl),
                                deadline: Some(deadline),
                                ..Default::default()
                            };
                            sweep_exhaustive(cand.as_ref(), &cfg, &wl, &params, sink);
                        }),
                    ));
                }
            }
        }
    }
    if round == 1 || (round > 2 && round % 2 == 1) {
        // P3: double cuts.
        for kind in WorkloadKind::ALL {
            for c in cands {
                let spec = workload_for(kind, c, round as u64);
                let (c, seed) = (c.clone(), round as u64);
                units.push((
                    3,
                    format!("double {c} {}", spec.label()),
                    Box::new(move || {
                        let Ok((cand, cfg)) = parse_candidate_spec(&c, p.sectors) else {
                            return;
                        };
                        let Ok(wl) = corpora.build(&spec) else { return };
                        let params = SweepParams {
                            seeds: vec![seed],
                            steps: double_steps(p, &wl),
                            deadline: Some(deadline),
                            ..Default::default()
                        };
                        let dc = if p.quick {
                            DoubleCutParams {
                                first_cuts: 2,
                                mount_cuts: 4,
                                rerun_cuts: 2,
                            }
                        } else {
                            DoubleCutParams::default()
                        };
                        sweep_double_cut(cand.as_ref(), &cfg, &wl, &params, &dc, sink);
                    }),
                ));
            }
        }
    }
    if round == 1 {
        // P4: the T1 dial sweep.
        if cands.iter().any(|c| c == "t1") {
            units.push((
                4,
                "t1 dial sweep".into(),
                Box::new(move || dial_sweep(p, corpora, sink)),
            ));
        }
        // P5: endurance, every candidate at once (one can take an hour).
        let all = cands.clone();
        units.push((
            5,
            format!("endurance {}", all.join(",")),
            Box::new(move || {
                all.par_iter().for_each(|c| {
                    let Ok((cand, cfg)) = parse_candidate_spec(c, p.sectors) else {
                        return;
                    };
                    let Ok(corpus) = corpora.get(big_corpus(cand.name())) else {
                        return;
                    };
                    let days = if p.quick { 1 } else { 30 };
                    let shape = if p.quick {
                        DayShape {
                            pushes: 1,
                            saves: 3,
                            panel_writes: 40,
                        }
                    } else {
                        DayShape::default()
                    };
                    let m = endurance(cand.as_ref(), &cfg, &corpus, days, shape, 1);
                    sink.write(
                        "endurance",
                        &serde_json::json!({"days": days, "corpus": corpus.name, "result": m}),
                    );
                    log(&format!("  endurance {c} done"));
                });
            }),
        ));
        // P6: fill to full, every candidate at once.
        let all = cands.clone();
        units.push((
            6,
            format!("fill {}", all.join(",")),
            Box::new(move || {
                all.par_iter().for_each(|c| {
                    let Ok((cand, cfg)) = parse_candidate_spec(c, p.sectors) else {
                        return;
                    };
                    let (Ok(c20), Ok(c40)) = (corpora.get("c20"), corpora.get("c40")) else {
                        return;
                    };
                    let max = if p.quick { 4 } else { 40 };
                    sink.write("fill", &fill_slots(cand.as_ref(), &cfg, &c20, max));
                    let base = if cand.name() == "f1" { &c20 } else { &c40 };
                    let extra = if p.quick { 16 } else { 400 };
                    sink.write(
                        "largest",
                        &largest_project(cand.as_ref(), &cfg, base, extra),
                    );
                    log(&format!("  fill {c} done"));
                });
            }),
        ));
    }
    // P7: random model-based sequences, every round.
    for c in cands {
        let c = c.clone();
        let base = (round as u64) * 1000;
        units.push((
            7,
            format!("random {c} seeds {base}+"),
            Box::new(move || {
                let Ok((cand, cfg)) = parse_candidate_spec(&c, p.sectors) else {
                    return;
                };
                let n = if p.quick { 2 } else { 16 };
                (0..n).into_par_iter().for_each(|i| {
                    let rp = RandomParams {
                        candidate: cand.name().into(),
                        config: cfg.clone(),
                        corpora: vec!["c13".into(), "c20".into(), "c40reuse".into(), "c40".into()],
                        seed: base + i,
                        steps: if p.quick { 30 } else { 300 },
                        cut_one_in: 3,
                        stop_at_cut: None,
                    };
                    random_walk(cand.as_ref(), &rp, corpora, sink);
                });
            }),
        ));
    }
    units
}

/// Cut points a step in round 1 (see `round_units`).
pub const ROUND1_CUTS: u64 = 256;

/// …and for S1, whose cut cases cost ~100× the others' (no key cache: every
/// read scans the flash).
pub const ROUND1_CUTS_S1: u64 = 48;

/// Round 1's steps: the first five focus steps and the last (a long save or
/// panel run repeats itself); later rounds sweep every step.
fn round1_steps(wl: &crate::Workload) -> Option<Vec<usize>> {
    let mut v: Vec<usize> = (wl.focus..wl.steps.len().min(wl.focus + 5)).collect();
    let last = wl.steps.len() - 1;
    if !v.contains(&last) {
        v.push(last);
    }
    Some(v)
}

/// Sweep order for the uncapped rounds: cheapest cut case first.
fn speed_rank(cand: &str) -> u32 {
    match cand {
        "t1" => 0,
        "f2" => 1,
        "f1" => 2,
        "s1" => 9,
        _ => 5,
    }
}

/// A partition small enough that the save and panel workloads on c40 run
/// the store's garbage collection (or compaction) under the cuts: a little
/// above each candidate's measured editable minimum. `None` where the
/// candidate has no headroom to tighten (F1 cannot hold c40 at all).
pub fn tight_sectors(cand: &str) -> Option<u32> {
    match cand {
        "f2" => Some(48),
        "s1" => Some(96),
        "t1" => Some(28),
        _ => None,
    }
}

/// `kind` on the candidate's big corpus (switch: c13 ↔ c40reuse).
fn workload_for(kind: WorkloadKind, cand: &str, seed: u64) -> WorkloadSpec {
    let corpus = match kind {
        WorkloadKind::Switch => "c13,c40reuse",
        _ => big_corpus(cand.split('@').next().unwrap_or(cand)),
    };
    WorkloadSpec::new(kind, corpus, seed)
}

fn quick_steps(p: &OvernightParams, wl: &crate::Workload) -> Option<Vec<usize>> {
    p.quick
        .then(|| (wl.focus..wl.steps.len().min(wl.focus + 2)).collect())
}

/// Double cuts on the first few focus steps (the long workloads repeat
/// themselves: one save or panel write is like the next).
fn double_steps(p: &OvernightParams, wl: &crate::Workload) -> Option<Vec<usize>> {
    let n = if p.quick { 1 } else { 4 };
    Some((wl.focus..wl.steps.len().min(wl.focus + n)).collect())
}

/// P1 for one candidate: every corpus × workload, plus the smallest partitions.
fn measures(p: &OvernightParams, corpora: &CorpusSet, sink: &Scoreboard, spec: &str) {
    let Ok((cand, cfg)) = parse_candidate_spec(spec, p.sectors) else {
        return;
    };
    let corpus_names: &[&str] = if p.quick {
        &["c13", "c40"]
    } else {
        &["c13", "c20", "c40", "c40reuse", "c40-min-z"]
    };
    let mut jobs: Vec<WorkloadSpec> = Vec::new();
    for c in corpus_names {
        for kind in [
            WorkloadKind::Push,
            WorkloadKind::Repush,
            WorkloadKind::Save,
            WorkloadKind::Panel,
        ] {
            jobs.push(WorkloadSpec::new(kind, c, 1));
        }
    }
    jobs.push(WorkloadSpec::new(WorkloadKind::Switch, "c13,c40reuse", 1));
    jobs.par_iter().for_each(|spec| {
        let Ok(wl) = corpora.build(spec) else { return };
        sink.write("measure", &measure(cand.as_ref(), &cfg, &wl));
        if matches!(spec.kind, WorkloadKind::Push | WorkloadKind::Save) {
            let min = min_sectors(cand.as_ref(), &cfg, &wl, 4, 512);
            sink.write(
                "min_sectors",
                &serde_json::json!({"candidate": cand.name(), "config": cfg, "workload": spec, "min_sectors": min}),
            );
        }
    });
}

/// P4: T1's dials, each setting measured fault-free plus a reduced cut sweep.
fn dial_sweep(p: &OvernightParams, corpora: &CorpusSet, sink: &Scoreboard) {
    let mut settings: Vec<CandidateConfig> = Vec::new();
    let record_max: &[&str] = if p.quick {
        &["512", "1024"]
    } else {
        &["256", "512", "1024", "2048"]
    };
    let gc: &[&str] = &["greedy", "cost_benefit"];
    let reserve: &[&str] = if p.quick { &["3"] } else { &["2", "3", "4"] };
    let codecs: &[&str] = &["stored", "host_deflate"];
    let parts: &[u32] = if p.quick { &[128] } else { &[96, 128, 176] };
    for rm in record_max {
        for g in gc {
            for r in reserve {
                for codec in codecs {
                    for &sectors in parts {
                        let c = CandidateConfig::new(sectors)
                            .with_dial("record_max", rm)
                            .with_dial("gc_policy", g)
                            .with_dial("reserve", r)
                            .with_dial("codec", codec);
                        settings.push(c);
                    }
                }
            }
        }
    }
    log(&format!("  t1 dial sweep: {} settings", settings.len()));
    let Ok((cand, _)) = parse_candidate_spec("t1", p.sectors) else {
        return;
    };
    let deadline = p.deadline;
    settings.par_iter().for_each(|cfg| {
        if Instant::now() >= deadline {
            return;
        }
        for spec in [
            WorkloadSpec::new(WorkloadKind::Push, "c40", 1),
            WorkloadSpec::new(WorkloadKind::Save, "c40", 1),
            WorkloadSpec::new(WorkloadKind::Panel, "c40", 1),
        ] {
            let Ok(wl) = corpora.build(&spec) else { return };
            let m = measure(cand.as_ref(), cfg, &wl);
            sink.write("dial_measure", &m);
        }
        let Ok(save) = corpora.build(&WorkloadSpec::new(WorkloadKind::Save, "c13", 1)) else {
            return;
        };
        let params = SweepParams {
            seeds: vec![1],
            max_cuts_per_step: Some(if p.quick { 8 } else { 32 }),
            steps: Some(vec![2, 3]),
            deadline: Some(deadline),
            ..Default::default()
        };
        // Written as `sweep_summary` with the setting's config: the report
        // groups them under the dial table.
        sweep_exhaustive(cand.as_ref(), cfg, &save, &params, sink);
    });
}

/// `HH:MM` local today (tomorrow if already past), or `+<n>[smh]` from now.
pub fn parse_deadline(s: &str) -> Result<Instant, String> {
    if let Some(rel) = s.strip_prefix('+') {
        let (num, unit) = rel.split_at(rel.len().saturating_sub(1));
        let n: u64 = num.parse().map_err(|_| format!("bad duration {s:?}"))?;
        let secs = match unit {
            "s" => n,
            "m" => n * 60,
            "h" => n * 3600,
            _ => return Err(format!("bad duration unit in {s:?}")),
        };
        return Ok(Instant::now() + Duration::from_secs(secs));
    }
    let (h, m) = s.split_once(':').ok_or_else(|| format!("bad time {s:?}"))?;
    let (h, m): (u64, u64) = (
        h.parse().map_err(|_| format!("bad hour {s:?}"))?,
        m.parse().map_err(|_| format!("bad minute {s:?}"))?,
    );
    let now = local_seconds_of_day()?;
    let target = h * 3600 + m * 60;
    let wait = if target > now {
        target - now
    } else {
        target + 86_400 - now
    };
    Ok(Instant::now() + Duration::from_secs(wait))
}

/// Seconds since local midnight, from `date` (a host tool; no time zone crate).
fn local_seconds_of_day() -> Result<u64, String> {
    let out = std::process::Command::new("date")
        .arg("+%H:%M:%S")
        .output()
        .map_err(|e| format!("date: {e}"))?;
    let t = String::from_utf8_lossy(&out.stdout);
    let v: Vec<u64> = t.trim().split(':').filter_map(|x| x.parse().ok()).collect();
    match v.as_slice() {
        [h, m, s] => Ok(h * 3600 + m * 60 + s),
        _ => Err(format!("date printed {t:?}")),
    }
}

fn log(msg: &str) {
    let t = std::process::Command::new("date")
        .arg("+%H:%M:%S")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    eprintln!("[{t}] {msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadlines_parse() {
        let now = Instant::now();
        let d = parse_deadline("+3m").unwrap();
        assert!(d >= now + Duration::from_secs(179));
        assert!(parse_deadline("07:00").unwrap() > now);
        assert!(parse_deadline("nope").is_err());
    }

    #[test]
    fn a_quick_overnight_runs_every_priority_on_the_reference() {
        // The real corpora are absent here: units whose corpus is missing
        // return early, so this proves the plumbing and the deadline only.
        let sink = Scoreboard::memory();
        let p = OvernightParams {
            deadline: Instant::now() + Duration::from_secs(1),
            candidates: vec!["mem".into()],
            sectors: 16,
            quick: true,
        };
        run_overnight(&p, &CorpusSet::new(None), &sink);
        let recs = sink.records();
        assert!(recs.iter().any(|r| r["type"] == "unit"));
        assert!(recs.iter().any(|r| r["type"] == "overnight_end"));
    }
}
