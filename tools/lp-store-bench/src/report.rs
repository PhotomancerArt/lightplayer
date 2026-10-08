//! Render `<out>/report.md` and `<out>/summary.json` from the scoreboard.
//! Re-runnable at any time: the JSONL is the source of truth.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::{Value, json};

/// Code size per candidate, from outside the run (the run measures no code):
/// T1 from its RV32 probe (crate README), littlefs and sequential-storage from
/// the spike.
pub fn code_size_note(cand: &str) -> &'static str {
    match cand {
        "f1" | "f2" => "~25 KB (littlefs in the shipped image; spike)",
        "s1" => "8.9–11.7 KB (spike)",
        "t1" => "see lp-tree-store README (RV32 probe)",
        _ => "—",
    }
}

fn s(v: &Value) -> String {
    match v {
        Value::Null => "—".into(),
        Value::String(x) => x.clone(),
        other => other.to_string(),
    }
}

fn u(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

/// `t1`, `t1@codec=stored`, `f2[96]` …
fn label(r: &Value) -> String {
    let cand = s(&r["candidate"]);
    let cfg = &r["config"];
    let dials: Vec<String> = cfg["dials"]
        .as_object()
        .map(|m| m.iter().map(|(k, v)| format!("{k}={}", s(v))).collect())
        .unwrap_or_default();
    let mut l = cand;
    if !dials.is_empty() {
        l = format!("{l}@{}", dials.join("+"));
    }
    if let Some(n) = cfg["sectors"].as_u64()
        && n != 128
    {
        l = format!("{l}[{n}]");
    }
    l
}

fn is_default(r: &Value) -> bool {
    !label(r).contains(['@', '['])
}

fn wl(r: &Value) -> String {
    let w = &r["workload"];
    if w.is_null() {
        return "—".into();
    }
    format!("{}:{}", s(&w["kind"]), s(&w["corpus"]))
}

/// Render the report into `out`.
pub fn render_report(out: &Path) -> std::io::Result<String> {
    let text = std::fs::read_to_string(out.join("scoreboard.jsonl"))?;
    let recs: Vec<(usize, Value)> = text
        .lines()
        .enumerate()
        .filter_map(|(i, l)| serde_json::from_str(l).ok().map(|v| (i + 1, v)))
        .collect();
    let of = |t: &'static str| recs.iter().filter(move |(_, r)| r["type"] == t);
    let mut md = String::new();
    let start = of("run_start")
        .last()
        .map(|(_, r)| r.clone())
        .unwrap_or(Value::Null);
    let end = of("overnight_end").last().map(|(_, r)| r.clone());
    writeln!(md, "# Storage candidate race — scoreboard\n").unwrap();
    writeln!(
        md,
        "**Simulator numbers** (`lp-nor-sim`, a NOR model on the host) — not emulator or silicon \
         measurements; no timing anywhere. Commit `{}`, started {}, {}.\n",
        s(&start["commit"]),
        s(&start["started"]),
        match &end {
            Some(e) => format!(
                "finished after {} round(s), {:.0} min",
                s(&e["rounds"]),
                e["elapsed_s"].as_f64().unwrap_or(0.0) / 60.0
            ),
            None => "still running (or stopped without finishing)".into(),
        }
    )
    .unwrap();

    // Candidates seen at their default config.
    let mut cands: Vec<String> = recs
        .iter()
        .filter(|(_, r)| r["candidate"].is_string() && is_default(r))
        .map(|(_, r)| s(&r["candidate"]))
        .collect();
    cands.sort();
    cands.dedup();

    // Cut results per (driver, label, workload, tear).
    let mut sweeps: BTreeMap<(String, String, String, String), [u64; 5]> = BTreeMap::new();
    let mut kinds: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut truncated: Vec<String> = Vec::new();
    for (_, r) in of("sweep_summary") {
        let key = (s(&r["driver"]), label(r), wl(r), s(&r["tear"]));
        let e = sweeps.entry(key.clone()).or_default();
        e[0] += u(&r["cases"]);
        e[1] += u(&r["landed"]);
        e[2] += u(&r["failures"]);
        e[3] += u(&r["non_atomic"]);
        e[4] += 1;
        if let Some(m) = r["kinds"].as_object() {
            let k = kinds.entry(label(r)).or_default();
            for (name, n) in m {
                *k.entry(name.clone()).or_default() += u(n);
            }
        }
        if r["truncated"] == true {
            truncated.push(format!("{} {} {} {}", key.0, key.1, key.2, key.3));
        }
        if r["error"].is_string() {
            truncated.push(format!(
                "{} {} {} {}: ERROR {}",
                key.0,
                key.1,
                key.2,
                key.3,
                s(&r["error"])
            ));
        }
    }
    let mut random: BTreeMap<String, [u64; 5]> = BTreeMap::new();
    for (_, r) in of("random_summary") {
        let e = random.entry(label(r)).or_default();
        e[0] += 1;
        e[1] += u(&r["steps_run"]);
        e[2] += u(&r["cuts"]);
        e[3] += u(&r["failures"]);
        e[4] += u(&r["non_atomic"]);
        if let Some(m) = r["kinds"].as_object() {
            let k = kinds.entry(label(r)).or_default();
            for (name, n) in m {
                *k.entry(name.clone()).or_default() += u(n);
            }
        }
    }
    let cut_totals = |l: &str| {
        let (mut cases, mut fails) = (0, 0);
        for ((_, lab, _, _), v) in &sweeps {
            if lab == l {
                cases += v[0];
                fails += v[2];
            }
        }
        if let Some(v) = random.get(l) {
            cases += v[2];
            fails += v[3];
        }
        (cases, fails)
    };
    let measure_of = |cand: &str, w: &str| {
        of("measure")
            .filter(|(_, r)| s(&r["candidate"]) == cand && is_default(r) && wl(r) == w)
            .last()
            .map(|(_, r)| r.clone())
    };
    let min_of = |cand: &str, w: &str| {
        of("min_sectors")
            .filter(|(_, r)| s(&r["candidate"]) == cand && is_default(r) && wl(r) == w)
            .last()
            .map(|(_, r)| s(&r["min_sectors"]))
            .unwrap_or_else(|| "—".into())
    };
    let endurance_of = |cand: &str| {
        of("endurance")
            .filter(|(_, r)| s(&r["result"]["candidate"]) == cand && is_default(&r["result"]))
            .last()
            .map(|(_, r)| r.clone())
    };

    writeln!(md, "## Headline (128-sector partition, default dials)\n").unwrap();
    writeln!(
        md,
        "| candidate | eligible (zero cut failures) | c40 sectors at rest / flash peak | smallest partition: push c40 / editable c40 | mount reads (save c40) | RAM | code | write amp (30 days) | erases per sector after 30 days min/median/max |"
    )
    .unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|---|").unwrap();
    let mut summary = Vec::new();
    for c in &cands {
        let (cases, fails) = cut_totals(c);
        let elig = if cases == 0 {
            "not run".to_string()
        } else if fails == 0 {
            format!("**yes** ({cases} cut cases)")
        } else {
            format!("**NO** — {fails} of {cases}")
        };
        let big = if c == "f1" { "c40" } else { "c40" };
        let push = measure_of(c, &format!("push:{big}"));
        let rest = match &push {
            Some(m) if m["ok"] == true => format!(
                "{} / {}",
                if m["used_sectors_end"].is_null() {
                    s(&m["sectors_nonblank_end"])
                } else {
                    s(&m["used_sectors_end"])
                },
                s(&m["sectors_nonblank_peak"])
            ),
            Some(m) => format!("does not fit ({})", s(&m["error"])),
            None => "—".into(),
        };
        let save = measure_of(c, &format!("save:{big}")).or_else(|| measure_of(c, "save:c20"));
        let (mount, ram) = match &save {
            Some(m) => (
                format!(
                    "{:.1} KB / {} calls",
                    u(&m["mount_read_bytes"]) as f64 / 1024.0,
                    s(&m["mount_read_calls"])
                ),
                format!("{:.1} KB", u(&m["report"]["ram_bytes"]) as f64 / 1024.0),
            ),
            None => ("—".into(), "—".into()),
        };
        let end = endurance_of(c);
        let (wa, spread) = match &end {
            Some(e) => {
                let m = &e["result"];
                (
                    format!(
                        "{:.2}{}",
                        m["write_amp"].as_f64().unwrap_or(0.0),
                        if m["ok"] == true {
                            String::new()
                        } else {
                            format!(" (stopped: {})", s(&m["error"]))
                        }
                    ),
                    format!(
                        "{} / {} / {} ({})",
                        s(&m["erases_min"]),
                        s(&m["erases_median"]),
                        s(&m["erases_max"]),
                        s(&e["corpus"])
                    ),
                )
            }
            None => ("—".into(), "—".into()),
        };
        let mins = format!("{} / {}", min_of(c, "push:c40"), min_of(c, "save:c40"));
        writeln!(
            md,
            "| {c} | {elig} | {rest} | {mins} | {mount} | {ram} | {} | {wa} | {spread} |",
            code_size_note(c)
        )
        .unwrap();
        summary.push(json!({
            "candidate": c, "cut_cases": cases, "cut_failures": fails, "eligible": cases > 0 && fails == 0,
            "c40_at_rest_peak": rest, "min_partition_push_editable": mins, "mount": mount, "ram": ram,
            "write_amp_30d": wa, "erase_spread_30d": spread,
        }));
    }
    writeln!(
        md,
        "\nSectors at rest = the store's own count where it reports one (littlefs `fs_size`, T1's live sectors), else non-blank sectors; flash peak = the most non-blank sectors at once during the push. Smallest partition = binary search over partition size for the workload to complete (\"editable\" = push + 20 saves). F1 cannot hold c40 at 128 sectors, so its cut sweeps and endurance run on c20.\n"
    )
    .unwrap();

    writeln!(md, "## Cut sweeps\n").unwrap();
    writeln!(
        md,
        "| driver | candidate | workload | tear | cases | landed | failures | non-atomic |"
    )
    .unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|").unwrap();
    for ((d, l, w, t), v) in &sweeps {
        if l.contains('@') {
            continue; // the dial sweep's own table below
        }
        writeln!(
            md,
            "| {d} | {l} | {w} | {t} | {} | {} | {} | {} |",
            v[0],
            v[1],
            if v[2] > 0 {
                format!("**{}**", v[2])
            } else {
                "0".into()
            },
            v[3]
        )
        .unwrap();
    }
    writeln!(md, "\n### Random walks\n").unwrap();
    writeln!(
        md,
        "| candidate | walks | steps | cuts | failures | non-atomic |"
    )
    .unwrap();
    writeln!(md, "|---|---|---|---|---|---|").unwrap();
    for (l, v) in &random {
        writeln!(
            md,
            "| {l} | {} | {} | {} | {} | {} |",
            v[0], v[1], v[2], v[3], v[4]
        )
        .unwrap();
    }
    writeln!(md, "\n### Failure kinds\n").unwrap();
    for (l, k) in &kinds {
        if !k.is_empty() {
            writeln!(md, "- **{l}**: {k:?}").unwrap();
        }
    }
    writeln!(
        md,
        "\n### Failures (first 8 per candidate) and how to replay them\n"
    )
    .unwrap();
    let mut per: BTreeMap<String, usize> = BTreeMap::new();
    for (line, r) in of("failure") {
        let c = s(&r["reproducer"]["case"]["candidate"]).replace('—', "");
        let c = if c.is_empty() {
            s(&r["reproducer"]["random"]["candidate"])
        } else {
            c
        };
        let n = per.entry(c.clone()).or_default();
        *n += 1;
        if *n > 8 {
            continue;
        }
        writeln!(
            md,
            "- {c} ({}) `{}`: {}\n  `sed -n '{line}p' {}/scoreboard.jsonl | lp-store-bench replay -`",
            s(&r["driver"]),
            s(&r["failure"]["kind"]),
            s(&r["failure"]["detail"]).replace('|', "/"),
            out.display()
        )
        .unwrap();
    }

    writeln!(
        md,
        "\n## Fault-free measures (default dials, 128 sectors)\n"
    )
    .unwrap();
    writeln!(md, "| candidate | workload | ok | sectors used end/max | flash non-blank end/peak | write amp | mount KB/calls | RAM KB | erases max | live KB |").unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|---|---|").unwrap();
    for (_, r) in of("measure").filter(|(_, r)| is_default(r)) {
        writeln!(
            md,
            "| {} | {} | {} | {}/{} | {}/{} | {:.2} | {:.1}/{} | {:.1} | {} | {:.1} |",
            s(&r["candidate"]),
            wl(r),
            if r["ok"] == true {
                "ok".to_string()
            } else {
                format!("**{}**", s(&r["error"]))
            },
            s(&r["used_sectors_end"]),
            s(&r["used_sectors_max"]),
            s(&r["sectors_nonblank_end"]),
            s(&r["sectors_nonblank_peak"]),
            r["write_amp"].as_f64().unwrap_or(0.0),
            u(&r["mount_read_bytes"]) as f64 / 1024.0,
            s(&r["mount_read_calls"]),
            u(&r["report"]["ram_bytes"]) as f64 / 1024.0,
            s(&r["erases_max"]),
            u(&r["live_logical_bytes"]) as f64 / 1024.0,
        )
        .unwrap();
    }
    writeln!(md, "\n### Smallest partitions\n").unwrap();
    writeln!(md, "| candidate | workload | min sectors |").unwrap();
    writeln!(md, "|---|---|---|").unwrap();
    for (_, r) in of("min_sectors").filter(|(_, r)| is_default(r)) {
        writeln!(
            md,
            "| {} | {} | {} |",
            s(&r["candidate"]),
            wl(r),
            s(&r["min_sectors"])
        )
        .unwrap();
    }
    writeln!(md, "\n### Fill to full (128 sectors)\n").unwrap();
    writeln!(md, "| candidate | c20 copies held | saves after (of 20) | largest c40-style project (modules) | its size KB |").unwrap();
    writeln!(md, "|---|---|---|---|---|").unwrap();
    for c in &cands {
        let f = of("fill")
            .filter(|(_, r)| s(&r["candidate"]) == *c)
            .last()
            .map(|(_, r)| r.clone());
        let l = of("largest")
            .filter(|(_, r)| s(&r["candidate"]) == *c)
            .last()
            .map(|(_, r)| r.clone());
        if f.is_none() && l.is_none() {
            continue;
        }
        let f = f.unwrap_or(Value::Null);
        let l = l.unwrap_or(Value::Null);
        writeln!(
            md,
            "| {c} | {} | {} | {} ({} base) | {:.1} |",
            s(&f["slots_held"]),
            s(&f["saves_ok"]),
            s(&l["modules"]),
            s(&l["base"]),
            u(&l["logical_bytes"]) as f64 / 1024.0
        )
        .unwrap();
    }

    dial_table(&mut md, &recs, &sweeps);

    writeln!(md, "\n## What did not run\n").unwrap();
    match &end {
        Some(e) => {
            let nr = e["not_run"].as_array().cloned().unwrap_or_default();
            if nr.is_empty() {
                writeln!(md, "Everything in round 1 ran.").unwrap();
            } else {
                writeln!(md, "{} unit(s) were still queued at the deadline (later rounds repeat earlier work with new seeds, so only round-1 entries are new ground):\n", nr.len()).unwrap();
                for x in nr.iter().take(60) {
                    writeln!(md, "- {}", s(x)).unwrap();
                }
            }
        }
        None => writeln!(md, "The run has not finished; this report is partial.").unwrap(),
    }
    if !truncated.is_empty() {
        writeln!(md, "\nSweeps cut short by the deadline or an error:\n").unwrap();
        for t in truncated.iter().take(40) {
            writeln!(md, "- {t}").unwrap();
        }
    }
    std::fs::write(out.join("report.md"), &md)?;
    std::fs::write(
        out.join("summary.json"),
        serde_json::to_string_pretty(&json!({"headline": summary, "end": end}))?,
    )?;
    Ok(md)
}

fn dial_table(
    md: &mut String,
    recs: &[(usize, Value)],
    sweeps: &BTreeMap<(String, String, String, String), [u64; 5]>,
) {
    // label → (push used, save used max, panel write amp, RAM, mount KB, ok)
    let mut rows: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    for (_, r) in recs.iter().filter(|(_, r)| r["type"] == "dial_measure") {
        rows.entry(label(r))
            .or_default()
            .insert(s(&r["workload"]["kind"]), r.clone());
    }
    if rows.is_empty() {
        return;
    }
    writeln!(md, "\n## T1 dial sweep\n").unwrap();
    writeln!(md, "Per setting: c40 push / save / panel fault-free, plus an exhaustive cut sweep of 2 save steps on c13 (all tears, ≤32 cuts a step).\n").unwrap();
    struct Row {
        label: String,
        sectors: f64,
        wa: f64,
        ram: f64,
        mount: f64,
        fails: u64,
        cases: u64,
        ok: bool,
    }
    let mut table = Vec::new();
    for (l, m) in &rows {
        let used = |k: &str| {
            m.get(k)
                .map(|r| {
                    r["used_sectors_max"]
                        .as_f64()
                        .or(r["sectors_nonblank_peak"].as_f64())
                        .unwrap_or(f64::NAN)
                })
                .unwrap_or(f64::NAN)
        };
        let ok = m.values().all(|r| r["ok"] == true);
        let (mut cases, mut fails) = (0, 0);
        for ((_, lab, _, _), v) in sweeps {
            if lab == l {
                cases += v[0];
                fails += v[2];
            }
        }
        table.push(Row {
            label: l.clone(),
            sectors: used("save"),
            wa: m
                .get("panel")
                .and_then(|r| r["write_amp"].as_f64())
                .unwrap_or(f64::NAN),
            ram: m
                .get("save")
                .map(|r| u(&r["report"]["ram_bytes"]) as f64 / 1024.0)
                .unwrap_or(f64::NAN),
            mount: m
                .get("save")
                .map(|r| u(&r["mount_read_bytes"]) as f64 / 1024.0)
                .unwrap_or(f64::NAN),
            fails,
            cases,
            ok,
        });
    }
    // Pareto front (lower is better on all three) among clean, fitting settings.
    let clean: Vec<&Row> = table
        .iter()
        .filter(|r| r.ok && r.fails == 0 && r.cases > 0)
        .collect();
    let dominated = |a: &Row| {
        clean.iter().any(|b| {
            b.sectors <= a.sectors
                && b.wa <= a.wa
                && b.ram <= a.ram
                && (b.sectors < a.sectors || b.wa < a.wa || b.ram < a.ram)
        })
    };
    writeln!(md, "{} settings measured, {} with zero cut failures and every workload fitting; {} with failures.\n", table.len(), clean.len(), table.iter().filter(|r| r.fails > 0).count()).unwrap();
    writeln!(
        md,
        "### Pareto front (save-c40 sectors × panel write amp × RAM)\n"
    )
    .unwrap();
    writeln!(
        md,
        "| setting | save c40 sectors (max) | panel write amp | RAM KB | mount KB | cut cases |"
    )
    .unwrap();
    writeln!(md, "|---|---|---|---|---|---|").unwrap();
    let mut front: Vec<&&Row> = clean.iter().filter(|r| !dominated(r)).collect();
    front.sort_by(|a, b| a.sectors.total_cmp(&b.sectors).then(a.wa.total_cmp(&b.wa)));
    for r in front.iter().take(30) {
        writeln!(
            md,
            "| {} | {} | {:.2} | {:.1} | {:.1} | {} |",
            r.label, r.sectors, r.wa, r.ram, r.mount, r.cases
        )
        .unwrap();
    }
    writeln!(md, "\n### Best per axis value (best achievable with that value, clean settings at 128 sectors)\n").unwrap();
    writeln!(
        md,
        "| dial = value | min save-c40 sectors | min panel write amp | min RAM KB |"
    )
    .unwrap();
    writeln!(md, "|---|---|---|---|").unwrap();
    let mut axes: BTreeMap<String, (f64, f64, f64)> = BTreeMap::new();
    for r in &clean {
        if r.label.contains('[') {
            continue;
        }
        let dials = r.label.split_once('@').map(|x| x.1).unwrap_or("");
        for kv in dials.split('+') {
            let e =
                axes.entry(kv.to_string())
                    .or_insert((f64::INFINITY, f64::INFINITY, f64::INFINITY));
            e.0 = e.0.min(r.sectors);
            e.1 = e.1.min(r.wa);
            e.2 = e.2.min(r.ram);
        }
    }
    for (k, (a, b, c)) in &axes {
        writeln!(md, "| {k} | {a} | {b:.2} | {c:.1} |").unwrap();
    }
    let bad: Vec<&Row> = table.iter().filter(|r| r.fails > 0 || !r.ok).collect();
    if !bad.is_empty() {
        writeln!(
            md,
            "\n### Settings with cut failures or a workload that did not fit\n"
        )
        .unwrap();
        for r in bad.iter().take(40) {
            writeln!(
                md,
                "- {}: {} failure(s) of {} cases{}",
                r.label,
                r.fails,
                r.cases,
                if r.ok { "" } else { "; a workload did not fit" }
            )
            .unwrap();
        }
    }
}
