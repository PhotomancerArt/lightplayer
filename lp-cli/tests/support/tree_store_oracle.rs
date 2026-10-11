//! The cut walks' oracle and runner (plan
//! `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`, P7, D3/D6).
//!
//! **The model.** A scenario is a script of steps (requests a client sends,
//! or emulated time let pass). A **dry run** — the same board, the same
//! flash, no cut — records, after every emulated slice in which an `lpfs`
//! command ran, what the flash holds: for the tree store, the committed
//! root's sequence and its file set (mounted on the host with
//! `lp-tree-store`); for the littlefs control, the file set (mounted with
//! `lpa-link`'s littlefs reader). It also records the in-range op indices
//! (the cut's coordinates), which ops are erases, which slices committed a
//! root, and each step's first and last op.
//!
//! **The oracle, two layers (D3), both required:**
//!
//! 1. *Store.* After a cut at op `i` inside step `k`, the cut image mounted on
//!    the host is a state the dry run committed between the end of step
//!    `k − 1` (the last acknowledged) and the end of step `k` (the one in
//!    flight): its root sequence is in that window and its files are exactly
//!    that root's files. After the power cycle the board boots `mounted`
//!    (the first-boot scenario: `formatted` or `mounted`), its files read
//!    over the wire are exactly the host's mount of its flash, and they are
//!    the cut image's files. A refused or `NoStore` mount after a first boot
//!    is a failure.
//! 2. *App.* The board serves; the project `/lightplayer.json` names loads,
//!    and its package hash is one the dry run had for that slot (the pushed
//!    one, if the switch committed, or the one before); a panel value reads
//!    back as one written at or before the cut.
//!
//! The littlefs control has no root sequence: a state passes when it is one
//! the dry run observed inside the window (a commit the slice sampling
//! missed reads as a mismatch, so the control's count is an upper bound).
//! Report-only.

#![allow(dead_code, reason = "each test file uses part of this")]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::engine::flash_cut::{FlashOpKind, TearModel};
use lp_emu_esp32c6::flash::{LPFS_LEN, LPFS_OFFSET};
use lp_emu_esp32c6::flash_cut_spec::FlashCutSpec;
use lpa_client::LpClient;
use lpa_link::layout_migration::{LpfsGeometry, LpfsTree};
use lpc_model::LpValue;
use lpfs::LpPath;

use super::editor_reads::block_on;
use super::tree_store_board::*;

/// The flash a walk runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fs {
    /// The `fs-tree` image: the tree store.
    Tree,
    /// Today's shipped image: littlefs (the report-only control).
    Littlefs,
}

/// One step of a scenario.
#[derive(Clone, Debug)]
pub enum Step {
    /// `StopAllProjects`.
    Stop,
    /// Remove `/projects/<id>` (`DeleteDir`).
    DeleteDir(String),
    /// The project's files into `/projects/<id>/`, one request per write
    /// (chunked as a push chunks them).
    WriteProject(String, Vec<(String, Vec<u8>)>),
    /// One file, written whole (Studio's save).
    WriteFile(String, Vec<u8>),
    /// `LoadProject projects/<id>` (it writes `/lightplayer.json`).
    Load(String),
    /// A panel write to the clock's `time` channel of the loaded project.
    PanelTime(f32),
    /// Let `us` emulated microseconds pass (the panel auto-save).
    Wait(u64),
}

/// A scenario: where it starts and what it does.
pub struct Scenario {
    pub name: &'static str,
    /// The chip the scenario starts on (empty: blank).
    pub chip: Vec<u8>,
    /// The cut counts from power-on and the boot is the scenario (the
    /// first-boot format).
    pub from_power_on: bool,
    pub steps: Vec<Step>,
    /// `(slot, package hash)` pairs the app layer accepts for a loaded slot.
    pub hashes: BTreeSet<(String, String)>,
    /// Panel values written by the scenario (and the one before it, if any).
    pub panel_values: Vec<f32>,
}

/// What a dry run saw.
#[derive(Clone, Debug, Default)]
pub struct DryRun {
    /// In-range commands the scenario ran (`T`).
    pub total_ops: u64,
    /// The committed states, by root sequence (tree store).
    pub roots: BTreeMap<u64, Files>,
    /// Every distinct state observed, in order, with the op count at which
    /// it was seen (the littlefs control).
    pub seen: Vec<(u64, Files)>,
    /// `(ops before, ops after, seq before, seq after)` per step.
    pub steps: Vec<(u64, u64, u64, u64)>,
    /// Op indices that were erases.
    pub erases: Vec<u64>,
    /// Op indices of the last programs before each new root was seen.
    pub root_ops: Vec<u64>,
}

/// A cut's replay key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CutKey {
    pub index: u64,
    pub tear: TearModel,
    pub seed: u64,
}

/// What a cut came to.
#[derive(Clone, Debug)]
pub enum CutVerdict {
    Pass {
        /// The step the cut landed in.
        step: usize,
        /// The state it left: `old` (the last acknowledged), `new` (the step
        /// in flight landed) or `between` (a commit inside the step).
        landed: &'static str,
    },
    /// The plan never fired (the index is past the scenario's last op).
    NotReached,
    Fail(String),
}

// ---- the dry run -----------------------------------------------------------

struct Recorder {
    fs: Fs,
    seen_ops: u64,
    last_seq: u64,
    last_files: Option<Files>,
    out: DryRun,
}

/// Run `scenario` with no cut on `elf`, recording the model.
pub fn dry_run(elf: &Path, scenario: &Scenario, fs: Fs, nonce: u32) -> DryRun {
    let mut host = board(elf, scenario.chip.clone(), nonce);
    let start_seq;
    let start_files;
    if scenario.from_power_on {
        start_seq = 0;
        start_files = Files::new();
    } else {
        boot(&mut host);
        let (files, seq) =
            state_of(fs, lpfs(&chip(&host))).expect("the scenario starts on a store");
        start_seq = seq;
        start_files = files;
    }
    let range = LPFS_OFFSET..LPFS_OFFSET + LPFS_LEN;
    host.board
        .machine
        .flash()
        .lock()
        .unwrap()
        .watch_flash_ops(range, true);
    let rec = Rc::new(RefCell::new(Recorder {
        fs,
        seen_ops: 0,
        last_seq: start_seq,
        last_files: Some(start_files.clone()),
        out: DryRun::default(),
    }));
    rec.borrow_mut()
        .out
        .roots
        .insert(start_seq, start_files.clone());
    rec.borrow_mut().out.seen.push((0, start_files));
    let observer = rec.clone();
    host.set_slice_observer(Some(Box::new(move |board: &C6Board| {
        observe(&mut observer.borrow_mut(), board);
    })));
    if scenario.from_power_on {
        let before = rec.borrow().seen_ops;
        boot(&mut host);
        observe_now(&rec, &host);
        let r = rec.borrow();
        let (ops, seq) = (r.seen_ops, r.last_seq);
        drop(r);
        rec.borrow_mut().out.steps.push((before, ops, 0, seq));
    }
    for (k, step) in scenario.steps.iter().enumerate() {
        let (before, seq_before) = {
            let r = rec.borrow();
            (r.seen_ops, r.last_seq)
        };
        run_step(&mut host, step, 10_000 + k as u64 * 100)
            .unwrap_or_else(|e| panic!("{}: dry run step {k} {step:?}: {e}", scenario.name));
        observe_now(&rec, &host);
        let r = rec.borrow();
        let (after, seq_after) = (r.seen_ops, r.last_seq);
        drop(r);
        rec.borrow_mut()
            .out
            .steps
            .push((before, after, seq_before, seq_after));
    }
    host.set_slice_observer(None);
    let census = host
        .board
        .machine
        .flash()
        .lock()
        .unwrap()
        .flash_op_census()
        .cloned()
        .expect("the census");
    let mut out = rec.borrow().out.clone();
    out.total_ops = census.ops();
    out.erases = census
        .trace()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind != FlashOpKind::Program)
        .map(|(i, _)| i as u64)
        .collect();
    out
}

fn observe_now(rec: &Rc<RefCell<Recorder>>, host: &EmuLinkHost<C6Board>) {
    observe(&mut rec.borrow_mut(), &host.board);
}

fn observe(rec: &mut Recorder, board: &C6Board) {
    let flash = board.machine.flash().lock().unwrap();
    let ops = flash.flash_op_census().map_or(0, |c| c.ops());
    if ops == rec.seen_ops {
        return;
    }
    rec.seen_ops = ops;
    let region = lpfs(flash.bytes()).to_vec();
    drop(flash);
    let Ok((files, seq)) = state_of(rec.fs, &region) else {
        return;
    };
    if rec.fs == Fs::Tree && seq != rec.last_seq {
        rec.last_seq = seq;
        rec.out.roots.insert(seq, files.clone());
        rec.out.root_ops.push(ops.saturating_sub(1));
        rec.out.root_ops.push(ops.saturating_sub(2));
    }
    if rec.last_files.as_ref() != Some(&files) {
        rec.out.seen.push((ops, files.clone()));
        rec.last_files = Some(files);
    }
}

/// The files (and, for the tree store, the root sequence) on `region`.
pub fn state_of(fs: Fs, region: &[u8]) -> Result<(Files, u64), String> {
    match fs {
        Fs::Tree => host_mount(region)
            .map(|(files, summary)| (files, summary.root_seq))
            .map_err(|e| format!("{e:?}")),
        Fs::Littlefs => {
            let geometry = LpfsGeometry {
                offset: LPFS_OFFSET,
                block_count: LPFS_LEN / 4096,
            };
            LpfsTree::from_image(region, geometry)
                .map(|(tree, _)| {
                    (
                        tree.files()
                            .map(|(p, b)| (p.to_string(), b.to_vec()))
                            .collect(),
                        0,
                    )
                })
                .map_err(|e| format!("{e:?}"))
        }
    }
}

// ---- one step ------------------------------------------------------------------

/// Run one step on the board; `Err` when the board stopped or refused it.
pub fn run_step(host: &mut EmuLinkHost<C6Board>, step: &Step, ids: u64) -> Result<(), String> {
    match step {
        Step::Wait(us) => {
            let until = host.board.machine.micros() + us;
            host.run_until(until, None)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        Step::PanelTime(value) => panel_time(host, *value, ids),
        _ => {
            let mut client = LpClient::new(host).with_request_ids_from(ids);
            match step {
                Step::Stop => block_on(client.stop_all_projects()).map(drop),
                Step::DeleteDir(id) => block_on(client.delete_project_dir(id)).map(drop),
                Step::Load(id) => block_on(
                    client.project_load(&lpa_client::project_deploy::project_load_path(id)),
                )
                .map(drop),
                Step::WriteFile(path, bytes) => {
                    block_on(client.fs_write(LpPath::new(path), bytes.clone())).map(drop)
                }
                Step::WriteProject(id, files) => {
                    let writes = lpa_client::project_deploy::project_write_requests(
                        id,
                        files
                            .iter()
                            .map(|(p, b)| lpa_client::ProjectDeployFile::new(p.clone(), b.clone())),
                    );
                    for request in writes {
                        let out = block_on(client.send_request(request.clone()))
                            .map_err(|e| e.to_string())?;
                        lpa_client::project_deploy::validate_project_deploy_response(
                            &request,
                            &out.value.msg,
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    Ok(())
                }
                Step::Wait(_) | Step::PanelTime(_) => unreachable!(),
            }
            .map_err(|e| e.to_string())
        }
    }
}

/// A panel write to the loaded project's `time` channel.
fn panel_time(host: &mut EmuLinkHost<C6Board>, value: f32, ids: u64) -> Result<(), String> {
    let mut client = LpClient::new(host).with_request_ids_from(ids);
    let loaded = block_on(client.project_list_loaded())
        .map_err(|e| e.to_string())?
        .value;
    let handle = loaded.first().ok_or("no project loaded")?.handle;
    let read: lpc_wire::ProjectReadRequest = serde_json::from_value(serde_json::json!({
        "since": null,
        "probes": [{"binding_graph": {"structure": "always", "include_values": false}}]
    }))
    .expect("a binding-graph read");
    let events = block_on(client.project_read(handle, read))
        .map_err(|e| e.to_string())?
        .value;
    let json = serde_json::to_value(&events).expect("events as JSON");
    let scope = find_time_scope(&json).ok_or("no `time` channel")?;
    let request = lpc_wire::WirePanelWriteRequest {
        scope: serde_json::from_value(scope).map_err(|e| e.to_string())?,
        channel: "time".to_string(),
        value: LpValue::F32(value),
        ttl_ms: None,
    };
    block_on(client.project_panel_write(handle, request))
        .map(drop)
        .map_err(|e| e.to_string())
}

/// The `scope` of the channel named `time`, anywhere in a read's JSON.
pub fn find_time_scope(v: &serde_json::Value) -> Option<serde_json::Value> {
    match v {
        serde_json::Value::Object(map) => {
            if map.get("name").and_then(|n| n.as_str()) == Some("time")
                && let Some(scope) = map.get("scope")
                && !scope.is_null()
            {
                return Some(scope.clone());
            }
            map.values().find_map(find_time_scope)
        }
        serde_json::Value::Array(items) => items.iter().find_map(find_time_scope),
        _ => None,
    }
}

// ---- one cut -------------------------------------------------------------------

/// Cut `scenario` at `key` on `elf`, power-cycle, and judge it against `dry`.
/// A failing cut's chip is kept at `keep_dir/<name>.bin`.
pub fn cut(
    elf: &Path,
    scenario: &Scenario,
    fs: Fs,
    dry: &DryRun,
    key: CutKey,
    nonce: u32,
    keep_dir: &Path,
) -> CutVerdict {
    let mut host = board(elf, scenario.chip.clone(), nonce);
    let spec = FlashCutSpec::new(key.index, key.tear, key.seed);
    let mut step_at = 0usize;
    if !scenario.from_power_on {
        boot(&mut host);
    }
    host.board
        .machine
        .arm_flash_cut(&spec)
        .expect("a machine that can power-cycle");
    let mut stopped = false;
    if scenario.from_power_on {
        stopped = host.wait_for_line("\"hello\":{", BOOT_BUDGET_US).is_err()
            || host.board.machine.last_flash_cut().is_some();
    } else {
        for (k, step) in scenario.steps.iter().enumerate() {
            step_at = k;
            if run_step(&mut host, step, 10_000 + k as u64 * 100).is_err()
                || host.board.machine.last_flash_cut().is_some()
            {
                stopped = true;
                break;
            }
        }
    }
    let Some(report) = host.board.machine.last_flash_cut() else {
        return if stopped {
            CutVerdict::Fail(format!(
                "the scenario stopped at step {step_at} with no cut"
            ))
        } else {
            CutVerdict::NotReached
        };
    };
    // Which step the cut landed in, by the dry run's op ranges.
    let step = dry
        .steps
        .iter()
        .position(|&(before, after, _, _)| key.index >= before && key.index < after)
        .unwrap_or(step_at);
    let (_, _, seq_lo, seq_hi) = dry.steps[step];
    let keep = |chip: &[u8], why: String| -> CutVerdict {
        let _ = std::fs::create_dir_all(keep_dir);
        let path = keep_dir.join(format!(
            "{}-{:?}-{}-{}.bin",
            scenario.name,
            fs,
            key.tear.name(),
            key.index
        ));
        let _ = std::fs::write(&path, chip);
        CutVerdict::Fail(format!(
            "{why} — replay key ({}, seed {}, index {}, {}); chip kept at {}; the cut: {report}",
            scenario.name,
            key.seed,
            key.index,
            key.tear.name(),
            path.display()
        ))
    };
    let cut_chip = chip(&host);

    // Store layer, on the cut image itself.
    let cut_state = state_of(fs, lpfs(&cut_chip));
    let landed = match (&cut_state, fs) {
        (Err(e), _) if scenario.from_power_on && e.contains("NoStore") => "old",
        (Err(e), _) => return keep(&cut_chip, format!("the cut image does not mount: {e}")),
        (Ok((files, seq)), Fs::Tree) => {
            if *seq < seq_lo || *seq > seq_hi {
                return keep(
                    &cut_chip,
                    format!(
                        "the cut image's root {seq} is outside the step's window {seq_lo}..={seq_hi}"
                    ),
                );
            }
            match dry.roots.get(seq) {
                Some(want) if want == files => {}
                Some(want) => {
                    return keep(
                        &cut_chip,
                        format!(
                            "root {seq}'s files differ from the dry run's: {}",
                            diff(want, files)
                        ),
                    );
                }
                None => {
                    return keep(
                        &cut_chip,
                        format!("root {seq} was never seen in the dry run"),
                    );
                }
            }
            if *seq == seq_lo {
                "old"
            } else if *seq == seq_hi {
                "new"
            } else {
                "between"
            }
        }
        (Ok((files, _)), Fs::Littlefs) => {
            // The states seen from the last one before the step to the last
            // one by its end.
            let (before, after, _, _) = dry.steps[step];
            let lo = dry
                .seen
                .iter()
                .rposition(|(at, _)| *at <= before)
                .unwrap_or(0);
            let hi = dry
                .seen
                .iter()
                .rposition(|(at, _)| *at <= after)
                .unwrap_or(lo);
            match (lo..=hi).find(|&i| &dry.seen[i].1 == files) {
                Some(i) if i == lo => "old",
                Some(i) if i == hi => "new",
                Some(_) => "between",
                None => {
                    return keep(
                        &cut_chip,
                        format!(
                            "littlefs: the cut image's files are no state the dry run saw in the step ({} files)",
                            files.len()
                        ),
                    );
                }
            }
        }
    };

    // The power cycle, and the board's own reading.
    let mut host = power_cycle(host, nonce ^ 0x5A5A);
    let hello = match host.wait_for_line("\"hello\":{", BOOT_BUDGET_US) {
        Ok(Some(h)) => h,
        other => {
            return keep(
                &cut_chip,
                format!("no hello after the power cycle: {other:?}\n{}", tail(&host)),
            );
        }
    };
    let fs_word = hello_fs(&hello);
    let ok_words: &[&str] = if scenario.from_power_on {
        &["formatted", "mounted"]
    } else {
        &["mounted"]
    };
    if !ok_words.contains(&fs_word.as_str()) {
        return keep(
            &cut_chip,
            format!("the board came back `{fs_word}`\n{}", tail(&host)),
        );
    }
    let after_chip = chip(&host);
    let host_files = match state_of(fs, lpfs(&after_chip)) {
        Ok((files, _)) => files,
        Err(e) => {
            return keep(
                &cut_chip,
                format!("the rebooted board's flash does not mount on the host: {e}"),
            );
        }
    };
    let wire = match wire_files(&mut host) {
        Ok(files) => files,
        Err(e) => {
            return keep(
                &cut_chip,
                format!("the rebooted board did not serve its files: {e}"),
            );
        }
    };
    if wire != host_files {
        return keep(
            &cut_chip,
            format!(
                "the board serves files its flash does not hold: {}",
                diff(&host_files, &wire)
            ),
        );
    }
    if let Ok((files, _)) = &cut_state
        && *files != host_files
    {
        return keep(
            &cut_chip,
            format!("the reboot changed the files: {}", diff(files, &host_files)),
        );
    }

    // App layer.
    if let Err(why) = app_layer(&mut host, scenario, &host_files) {
        return keep(&cut_chip, why);
    }
    CutVerdict::Pass { step, landed }
}

/// Every file the board serves, read over the wire.
fn wire_files(host: &mut EmuLinkHost<C6Board>) -> Result<Files, String> {
    let mut client = LpClient::new(host).with_request_ids_from(90_000);
    let listed = block_on(client.fs_list_dir(LpPath::new("/"), true))
        .map_err(|e| e.to_string())?
        .value;
    let mut out = Files::new();
    for path in listed {
        let path = path.as_str().to_string();
        match block_on(client.fs_read(LpPath::new(&path))) {
            Ok(read) => {
                out.insert(path, read.value);
            }
            // A directory listed recursively reads as an error: skip it.
            Err(_) => {}
        }
    }
    Ok(out)
}

/// The app layer (D3 layer 2): the project `/lightplayer.json` names loads
/// and holds a package the scenario had in that slot; a panel value is one
/// that was written.
fn app_layer(
    host: &mut EmuLinkHost<C6Board>,
    scenario: &Scenario,
    files: &Files,
) -> Result<(), String> {
    let mut client = LpClient::new(host).with_request_ids_from(95_000);
    let loaded = block_on(client.project_list_loaded())
        .map_err(|e| format!("list loaded: {e}"))?
        .value;
    let named = files.get("/lightplayer.json").and_then(|b| {
        let v: serde_json::Value = serde_json::from_slice(b).ok()?;
        v.get("startup_project")?.as_str().map(str::to_string)
    });
    if let Some(named) = &named {
        let slot = named
            .trim_start_matches('/')
            .trim_start_matches("projects/")
            .to_string();
        let running = loaded.first().map(|p| p.path.as_str().to_string());
        if !running.as_deref().is_some_and(|p| p.ends_with(&slot)) {
            return Err(format!(
                "`/lightplayer.json` names {named} but the board runs {running:?}"
            ));
        }
        if !scenario.hashes.is_empty() {
            let hash = block_on(client.hash_package(&slot))
                .map_err(|e| format!("hash {slot}: {e}"))?
                .value;
            if !scenario.hashes.contains(&(slot.clone(), hash.clone())) {
                return Err(format!(
                    "{slot} runs a package the scenario never had: {hash}"
                ));
            }
        }
    }
    for (path, bytes) in files {
        if path.ends_with("/.lp/panel.json") {
            let text = String::from_utf8_lossy(bytes);
            let ok = scenario.panel_values.is_empty()
                || scenario
                    .panel_values
                    .iter()
                    .any(|v| text.contains(&format!("{{\"f32\":{v}}}")));
            if !ok {
                return Err(format!("{path} holds a value never written: {text}"));
            }
        }
    }
    Ok(())
}

fn diff(want: &Files, got: &Files) -> String {
    let mut out = Vec::new();
    for (p, b) in want {
        match got.get(p) {
            None => out.push(format!("-{p}")),
            Some(g) if g != b => out.push(format!("~{p} ({} → {} B)", b.len(), g.len())),
            _ => {}
        }
    }
    for p in got.keys().filter(|p| !want.contains_key(*p)) {
        out.push(format!("+{p}"));
    }
    out.join(", ")
}

fn tail(host: &EmuLinkHost<C6Board>) -> String {
    let c = host.console();
    c[c.len().saturating_sub(30)..].join("\n")
}

/// The cut indices for a scenario (D6): the forced set — first and last op,
/// every erase, the ops that completed a root — plus `sample` seeded ones.
pub fn cut_indices(dry: &DryRun, seed: u64, sample: usize) -> Vec<u64> {
    let t = dry.total_ops;
    let mut set = BTreeSet::new();
    if t == 0 {
        return Vec::new();
    }
    set.insert(0);
    set.insert(t - 1);
    set.extend(dry.erases.iter().copied().filter(|&i| i < t));
    set.extend(dry.root_ops.iter().copied().filter(|&i| i < t));
    let mut r = seed | 1;
    for _ in 0..sample {
        r = r
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        set.insert((r >> 33) % t);
    }
    set.into_iter().collect()
}

/// Where failing chips are kept.
pub fn keep_dir() -> PathBuf {
    repo_root().join("target/tree-store-cuts")
}
