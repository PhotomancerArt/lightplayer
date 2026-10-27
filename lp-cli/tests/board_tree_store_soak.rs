//! The tree store's power-cut soak on the sacrificial C6, CX1 (plan
//! `lp2025/2026-10-08-1017-tree-store-device-round`, M7 scope 1). **Zero
//! failures is the only pass.** Silicon, not emulated.
//!
//! The board runs the `fs-tree` firmware (the tree store as `lpfs`). Each
//! cycle, over USB, the host does one thing a user does — a push (Studio's
//! one-slot batch, `lpa_client::push_project`), a save (one file written
//! whole), a panel write (persisted by the board's own auto-save), a switch
//! (`LoadProject` of the other project, which rewrites `/lightplayer.json`),
//! or now and then a c40-sized push — and `board power-cycle` cuts the power
//! at a random moment inside it. Then:
//!
//! 1. **The cut was a cut:** the port's device node went and came back, the
//!    command exited 0, and the board says it booted from power-on (its
//!    `[RECOVERY] boot: cause=power-on` line, or a heartbeat's
//!    `resetReason`). Anything else **stops the run** (a hub that cannot
//!    switch, a failed `board`): a reset that was not a cut is not counted.
//! 2. **The boot check:** the hello's `fs` (the mount verdict) is `mounted`,
//!    and the board's `[FS] tree store mounted (…)` line (sectors, free,
//!    root) and `[perf] fs mount` are kept.
//! 3. **The files:** every file the board serves is read back over the wire
//!    and judged by `support/board_files_model.rs`: old or new, per file; a
//!    batch old or new as a whole; nothing acknowledged lost.
//! 4. **The app:** the project `/lightplayer.json` names is the one running,
//!    and the board's `HashPackage` of it equals the host's hash of the files
//!    it served.
//! 5. **The flash**, every `LP_SOAK_FLASH_EVERY` cycles and after any
//!    failure: the raw `lpfs` read over the bootloader (`lp-cli hardware
//!    lpfs save`), mounted on the host with `lp-tree-store`, must hold
//!    exactly the files the board served, and pass `hardware tree check`.
//!    A failing cycle's raw image is kept for replay.
//!
//! Env: `LP_SOAK_DIR` (the run's records; required), `LP_SOAK_CYCLES`
//! (default 3,000), `LP_SOAK_START` (resume at this cycle; the base is read
//! back from the board), `LP_SOAK_SETUP=1` (upload the two projects first),
//! `LP_SOAK_SEED`, `LP_SOAK_DRY=1` (no cuts: time each operation),
//! `LP_SOAK_LP_CLI` (an `lp-cli` binary for the flash reads — copy it out of
//! `target/`, which the desk's hourly clean deletes), `LP_SOAK_FLASH_EVERY`
//! (default 25), `BOARD_HOLDER` (the lease's holder).
//!
//! `#[ignore]`d: it needs the desk, the board leased, and `fs-tree`
//! flashed. Records: `cycles.jsonl` (one line per cycle), `console.txt`
//! (every cycle's console), `failures/` (raw images and consoles).

#[path = "support/board_files_model.rs"]
mod board_files_model;
#[path = "support/desk_board.rs"]
mod desk_board;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use board_files_model::{Change, Event, Files, Landed, check, slot_files, startup_slot};
use desk_board::{Console, Desk, PortWatch, StampedIo, close, connect, lpfs_save, tree_check};
use lpa_client::{ClientEvent, ClientIo, LpClient};
use lpc_model::LpValue;
use lpfs::{LpFs, LpFsMemory, LpPath};
use tree_store_board::{c40_files, host_mount, project_files};

/// The cut lands about this long after `board power-cycle` starts. Measured
/// on CX1's hub (VIA twin 1-1.2 / 1-2.2), 2026-10-15, with
/// `power_cut_timing_on_cx1`: the board's last answer comes 0.25–0.35 s
/// after the command starts (the USB 2 half switching off), and VBUS drops
/// when the twin switches, a uhubctl run later. The schedule aims the cut at
/// a random point of the operation from it; the record keeps the last
/// answer the host had.
const CUT_LATENCY_S: f64 = 0.45;

/// Seconds after boot the first panel auto-save waits out
/// (`PANEL_STATE_WRITE_INTERVAL_MS`).
const PANEL_SAVE_AFTER_BOOT_S: f64 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Push,
    PushC40,
    Save,
    Panel,
    Switch,
}

impl Op {
    fn name(self) -> &'static str {
        match self {
            Op::Push => "push",
            Op::PushC40 => "push-c40",
            Op::Save => "save",
            Op::Panel => "panel",
            Op::Switch => "switch",
        }
    }

    /// How long its flash work lasts after it starts (measured with
    /// `LP_SOAK_DRY=1` on CX1), the window the cut is aimed into.
    fn window_s(self) -> f64 {
        match self {
            Op::Push => 1.0,
            Op::PushC40 => 7.0,
            Op::Save => 0.25,
            Op::Panel => 1.0,
            Op::Switch => 1.5,
        }
    }
}

#[test]
#[ignore = "drives the sacrificial CX1 on the desk (module docs)"]
fn tree_store_power_cut_soak_on_cx1() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(soak()));
}

/// Where a `board power-cycle`'s cut lands on this hub: the board answers
/// requests back to back (one in flight at a time) while the command runs,
/// and the last answer bounds the cut from below, the first failure and
/// the port's going from above. `LP_SOAK_DIR`, `LP_SOAK_CYCLES` (default
/// 10). What `CUT_LATENCY_S` is set from.
#[test]
#[ignore = "drives the sacrificial CX1 on the desk (module docs)"]
fn power_cut_timing_on_cx1() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(cut_timing()));
}

async fn cut_timing() {
    let cfg = Config::from_env();
    let holder =
        std::env::var("BOARD_HOLDER").unwrap_or_else(|_| "direct: tree-store M7".to_string());
    let desk = Desk::cx1(&holder);
    desk.check().unwrap_or_else(|e| panic!("{e}"));
    let mut log = Records::open(&cfg.dir);
    let n = std::env::var("LP_SOAK_CYCLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10u32);
    for i in 0..n {
        let port = desk.port_wait(Duration::from_secs(30)).expect("CX1's port");
        let epoch = Instant::now();
        let console = Console::new(epoch);
        let conn = connect(&port, &console).await.expect("CX1's link");
        let mut client = LpClient::new(conn.client_io());
        let watch = PortWatch::start(&port, epoch);
        let power = desk.power_cycle_async(epoch);
        let mut last_ok = None;
        let mut first_err = None;
        let mut answers = 0u32;
        while first_err.is_none() && epoch.elapsed() < Duration::from_secs(20) {
            match tokio::time::timeout(Duration::from_secs(5), client.project_list_loaded()).await {
                Ok(Ok(_)) => {
                    answers += 1;
                    last_ok = Some(epoch.elapsed().as_secs_f64());
                }
                _ => first_err = Some(epoch.elapsed().as_secs_f64()),
            }
        }
        drop(client);
        close(conn).await;
        while power.lock().unwrap().is_none() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let pc = power.lock().unwrap().clone().unwrap();
        let t = watch.times();
        let record = serde_json::json!({
            "i": i, "answers": answers, "last_ok_s": last_ok, "first_err_s": first_err,
            "port_gone_s": t.gone, "port_back_s": t.back,
            "exited_s": pc.exited, "code": pc.code,
        });
        eprintln!("{record}");
        log.cycle(&record);
        log.console(&format!("timing {i}"), &console.take());
        assert_eq!(
            pc.code,
            Some(0),
            "`board power-cycle` failed: {}",
            pc.output
        );
    }
}

struct Config {
    dir: PathBuf,
    cycles: u64,
    start: u64,
    setup: bool,
    seed: u64,
    dry: bool,
    lp_cli: PathBuf,
    flash_every: u64,
}

impl Config {
    fn from_env() -> Self {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let num = |k: &str, d: u64| env(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        Self {
            dir: PathBuf::from(env("LP_SOAK_DIR").expect("LP_SOAK_DIR: where the records go")),
            cycles: num("LP_SOAK_CYCLES", 3_000),
            start: num("LP_SOAK_START", 0),
            setup: env("LP_SOAK_SETUP").as_deref() == Some("1"),
            seed: num("LP_SOAK_SEED", 0x50A6_0007),
            dry: env("LP_SOAK_DRY").as_deref() == Some("1"),
            lp_cli: PathBuf::from(env("LP_SOAK_LP_CLI").unwrap_or_else(|| "lp-cli".into())),
            flash_every: num("LP_SOAK_FLASH_EVERY", 25),
        }
    }
}

/// The run's counters.
#[derive(Default)]
struct Tally {
    cycles: u64,
    failures: u64,
    flash_checks: u64,
    by_op: std::collections::BTreeMap<&'static str, (u64, u64, u64, u64)>, // cuts, old, new, same/mixed
}

async fn soak() {
    let cfg = Config::from_env();
    std::fs::create_dir_all(cfg.dir.join("failures")).expect("the records directory");
    let holder =
        std::env::var("BOARD_HOLDER").unwrap_or_else(|_| "direct: tree-store M7".to_string());
    let desk = Desk::cx1(&holder);
    desk.check().unwrap_or_else(|e| panic!("{e}"));
    let mut log = Records::open(&cfg.dir);
    let mut rng = Rng(cfg.seed ^ cfg.start.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let basic = project_files("projects/test/basic");

    let epoch = Instant::now();
    let console = Console::new(epoch);
    let port = desk
        .port_wait(Duration::from_secs(30))
        .expect("CX1's port (by MAC)");
    let mut conn = connect(&port, &console).await.expect("CX1's link");
    if cfg.setup {
        let mut client = LpClient::new(conn.client_io());
        let other = variant(&basic, "soak setup other");
        let demo = variant(&basic, "soak setup demo");
        client
            .replace_and_load_project("other", &other)
            .await
            .expect("upload other");
        client
            .replace_and_load_project("demo", &demo)
            .await
            .expect("upload demo");
        tokio::time::sleep(Duration::from_secs(4)).await;
    }
    let mut base = {
        let mut client = LpClient::new(conn.client_io());
        readback(&mut client).await.expect("the first read-back")
    };
    log.console("setup", &console.take());
    eprintln!(
        "soak: CX1 on {port}, {} files at the start, cycles {}..{}",
        base.len(),
        cfg.start,
        cfg.cycles
    );

    let mut tally = Tally::default();
    let mut port = port;
    let mut boot_at: Option<Instant> = Some(Instant::now());
    for n in cfg.start..cfg.cycles {
        if n % 10 == 0 {
            desk.renew(60)
                .unwrap_or_else(|e| panic!("STOP at cycle {n}: {e}"));
        }
        let cycle_epoch = Instant::now();
        let cycle_console = Console::new(cycle_epoch);
        let io = StampedIo::new(conn.client_io(), cycle_epoch);
        let (last_tx, last_rx) = (io.last_tx.clone(), io.last_rx.clone());
        let mut client = LpClient::new(io);
        let running = match running_slot(&mut client).await {
            Ok(s) => s,
            Err(e) => panic!("STOP at cycle {n}: what runs? {e}"),
        };
        let op = pick(&mut rng, &base, &running);
        // Where in the operation the cut is aimed (seconds after it starts).
        let window = match op {
            Op::Panel => {
                let since_boot = boot_at.map_or(30.0, |b| b.elapsed().as_secs_f64());
                (PANEL_SAVE_AFTER_BOOT_S - since_boot).max(0.0) + op.window_s()
            }
            _ => op.window_s(),
        };
        let aim = rng.f64() * (window + 0.6) - 0.3;
        let (spawn_at, op_at) = if aim <= CUT_LATENCY_S {
            (0.0, CUT_LATENCY_S - aim)
        } else {
            (aim - CUT_LATENCY_S, 0.0)
        };

        let watch = PortWatch::start(&port, cycle_epoch);
        let mut power = None;
        if !cfg.dry && spawn_at == 0.0 {
            power = Some(desk.power_cycle_async(cycle_epoch));
        }
        // The operation, with the power cycle started inside it when the aim
        // says so.
        let op_started;
        let op_end = std::cell::Cell::new(None);
        let mut events = Vec::new();
        let op_result = {
            tokio::time::sleep(Duration::from_secs_f64(op_at)).await;
            op_started = cycle_epoch.elapsed().as_secs_f64();
            let spawn_inside = (!cfg.dry && power.is_none()).then_some(spawn_at);
            let fut = async {
                let r = run_op(&mut client, op, n, &running, &base, &mut events).await;
                op_end.set(Some(cycle_epoch.elapsed().as_secs_f64()));
                r
            };
            let deadline = Duration::from_secs_f64(op.window_s() * 4.0 + 30.0);
            match spawn_inside {
                Some(after) => {
                    let pc = async {
                        tokio::time::sleep(Duration::from_secs_f64(after)).await;
                        desk.power_cycle_async(cycle_epoch)
                    };
                    tokio::pin!(fut);
                    tokio::pin!(pc);
                    let mut out = None;
                    let deadline = tokio::time::sleep(deadline);
                    tokio::pin!(deadline);
                    loop {
                        tokio::select! {
                            r = &mut fut, if out.is_none() => { out = Some(r); if power.is_some() { break; } }
                            p = &mut pc, if power.is_none() => { power = Some(p); if out.is_some() { break; } }
                            _ = &mut deadline => { break; }
                        }
                    }
                    if power.is_none() {
                        power = Some(desk.power_cycle_async(cycle_epoch));
                    }
                    out.unwrap_or_else(|| Err("the operation did not finish".to_string()))
                }
                None => match tokio::time::timeout(deadline, fut).await {
                    Ok(r) => r,
                    Err(_) => Err("the operation did not finish".to_string()),
                },
            }
        };
        let op_ended = op_end
            .get()
            .unwrap_or_else(|| cycle_epoch.elapsed().as_secs_f64());
        drop(client);
        // Closed on the side: the link to a board that lost power answers
        // its close late, and the boot's first lines come while it does.
        tokio::task::spawn_local(close(conn));

        // The cut, and the boot after it.
        let mut stop = None;
        let mut pc_record = None;
        if power.is_some() {
            let until = Instant::now() + Duration::from_secs(30);
            while watch.times().back.is_none() && Instant::now() < until {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            if watch.times().gone.is_none() {
                stop = Some("the port never went: no cut".to_string());
            }
        } else {
            // Dry: let what the board does on its own land.
            tokio::time::sleep(Duration::from_secs(4)).await;
        }
        let times = watch.times();
        let mut reconnected = None;
        if stop.is_none() {
            // The same device node, at once (its first lines are the boot's).
            match connect(&port, &cycle_console).await {
                Ok(c) => reconnected = Some(c),
                Err(e) => cycle_console.push(format!("[host] connect at power-up: {e}")),
            }
        }
        if stop.is_none() && reconnected.is_none() {
            port = desk
                .port_wait(Duration::from_secs(30))
                .unwrap_or_else(|| panic!("STOP at cycle {n}: CX1's port did not come back"));
            for attempt in 0..3 {
                match connect(&port, &cycle_console).await {
                    Ok(c) => {
                        reconnected = Some(c);
                        break;
                    }
                    Err(e) => {
                        cycle_console.push(format!("[host] connect attempt {attempt}: {e}"));
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }
        if let Some(power) = &power {
            let until = Instant::now() + Duration::from_secs(60);
            while power.lock().unwrap().is_none() && Instant::now() < until {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let pc = power.lock().unwrap().clone();
            match &pc {
                Some(pc) if pc.code == Some(0) => {}
                Some(pc) => {
                    stop = Some(format!(
                        "`board power-cycle` failed ({:?}): {}",
                        pc.code,
                        pc.output.trim()
                    ))
                }
                None => stop = Some("`board power-cycle` did not exit in 60 s".to_string()),
            }
            pc_record = pc;
        }
        if let Some(why) = &stop {
            log.console(&format!("cycle {n} STOP"), &cycle_console.take());
            panic!("STOP at cycle {n}: {why}");
        }
        let Some(conn_new) = reconnected else {
            // The board does not come up: that is a failure, with its flash.
            let dir = cfg.dir.join("failures").join(format!("cycle-{n:05}"));
            let raw = lpfs_save(&cfg.lp_cli, &port, &dir);
            log.console(&format!("cycle {n} NO LINK"), &cycle_console.take());
            panic!("FAIL at cycle {n}: no link after the cut; raw lpfs {raw:?}");
        };
        conn = conn_new;
        boot_at = Some(Instant::now());
        let hello = conn
            .hello()
            .map(|h| serde_json::to_value(h).expect("hello JSON"));
        let fs = hello
            .as_ref()
            .and_then(|h| h["hardware"]["fs"].as_str().map(str::to_string))
            .unwrap_or_default();

        // The read-back, the app check, the boot's own words.
        let mut failures = Vec::new();
        let mut heartbeat_reason = None;
        // A read-back the LINK interrupts (the host slept, a link reset) is
        // read again on a fresh link: what is judged is the board's files,
        // never a half-finished listing. Each retry is in the record.
        let mut retries = Vec::new();
        let mut observed = None;
        for attempt in 0..3 {
            let mut client = LpClient::new(conn.client_io());
            match readback_with(&mut client, &mut heartbeat_reason).await {
                Ok(f) => {
                    observed = Some(f);
                    break;
                }
                Err(e) => {
                    retries.push(format!("attempt {attempt}: {e}"));
                    drop(client);
                    close(conn).await;
                    port = desk.port_wait(Duration::from_secs(30)).unwrap_or_else(|| {
                        panic!("STOP at cycle {n}: no port for a read-back retry")
                    });
                    conn = connect(&port, &cycle_console).await.unwrap_or_else(|e| {
                        panic!("STOP at cycle {n}: no link for a read-back retry: {e}")
                    });
                }
            }
        }
        let mut client = LpClient::new(conn.client_io());
        let observed = match observed {
            Some(f) => f,
            None => {
                failures.push(format!("read-back: {}", retries.join("; ")));
                Files::new()
            }
        };
        if fs != "mounted" {
            failures.push(format!("mount verdict `{fs}`, not `mounted`"));
        }
        let verdict = check(&base, &events, &observed);
        failures.extend(verdict.failures.iter().cloned());
        if let Err(e) = app_check(&mut client, &observed).await {
            failures.push(format!("app: {e}"));
        }
        if heartbeat_reason.is_none() {
            heartbeat_reason = wait_heartbeat_reason(&mut client).await;
        }
        let boot_line = cycle_console.find("[RECOVERY] boot: cause=");
        let power_on = boot_line
            .as_deref()
            .map(|l| l.contains("cause=power-on"))
            .or(heartbeat_reason.as_deref().map(|r| r == "power-on"));
        if !cfg.dry && power_on != Some(true) {
            log.console(&format!("cycle {n} STOP"), &cycle_console.take());
            panic!(
                "STOP at cycle {n}: the boot after the cut was not a power-on \
                 (boot line {boot_line:?}, heartbeat {heartbeat_reason:?})"
            );
        }
        let fs_line = cycle_console.find("[FS] tree store");
        let mount_us = cycle_console.find("[perf] fs mount ").and_then(|l| {
            l.split_whitespace()
                .nth(3)
                .and_then(|v| v.parse::<u64>().ok())
        });

        // The flash, now and then and after any failure.
        let mut flash = serde_json::Value::Null;
        let flash_due = n % cfg.flash_every == cfg.flash_every - 1 || !failures.is_empty();
        if flash_due && !cfg.dry {
            drop(client);
            close(conn).await;
            let dir = if failures.is_empty() {
                cfg.dir.join("flash").join(format!("cycle-{n:05}"))
            } else {
                cfg.dir.join("failures").join(format!("cycle-{n:05}"))
            };
            flash = match lpfs_save(&cfg.lp_cli, &port, &dir) {
                Ok(raw) => {
                    let (fsck_ok, fsck) = tree_check(&cfg.lp_cli, &raw);
                    let region = std::fs::read(&raw).expect("the raw region");
                    let mounted = host_mount(&region);
                    let (same, summary) = match &mounted {
                        Ok((files, s)) => (
                            *files == observed,
                            format!(
                                "{} sectors, {} free, root {}",
                                s.sectors, s.free_sectors, s.root_seq
                            ),
                        ),
                        Err(e) => (false, format!("host mount: {e:?}")),
                    };
                    if !fsck_ok {
                        failures.push(format!("flash: tree check failed: {fsck}"));
                    }
                    if !same {
                        failures.push(format!(
                            "flash: the host's mount of the raw lpfs ({summary}) is not what the board served"
                        ));
                    }
                    if failures.is_empty() {
                        // A passing image is not kept (704 KB each).
                        let _ = std::fs::remove_file(&raw);
                    }
                    tally.flash_checks += 1;
                    serde_json::json!({"fsck": fsck_ok, "same": same, "summary": summary})
                }
                Err(e) => {
                    failures.push(format!("flash: lpfs save: {e}"));
                    serde_json::json!({"error": e})
                }
            };
            port = desk
                .port_wait(Duration::from_secs(30))
                .unwrap_or_else(|| panic!("STOP at cycle {n}: no port after the flash read"));
            conn = connect(&port, &cycle_console)
                .await
                .unwrap_or_else(|e| panic!("STOP at cycle {n}: no link after the flash read: {e}"));
            boot_at = Some(Instant::now());
        } else {
            drop(client);
        }

        // The record. The cut is estimated from the command's start; the
        // host's last answer bounds it from below.
        let cut_est = pc_record.as_ref().map(|p| p.spawned + CUT_LATENCY_S);
        let cut_at = cut_est.map(|c| c - op_started);
        let acked_before_cut = cut_est.map(|c| op_result.is_ok() && op_ended <= c);
        let last_rx = last_rx.get();
        let last_tx = last_tx.get();
        let entry = tally.by_op.entry(op.name()).or_default();
        entry.0 += 1;
        for l in &verdict.landed {
            match l {
                Some(Landed::Old) => entry.1 += 1,
                Some(Landed::New) => entry.2 += 1,
                _ => entry.3 += 1,
            }
        }
        tally.cycles += 1;
        if !failures.is_empty() {
            tally.failures += 1;
        }
        let record = serde_json::json!({
            "cycle": n,
            "op": op.name(),
            "slot": running,
            "aim_s": aim,
            "op_started_s": op_started,
            "op_ended_s": op_ended,
            "op_result": op_result.as_ref().map(|_| "ok").unwrap_or_else(|e| e.as_str()),
            "events": events.iter().map(|e| serde_json::json!({"label": e.label, "durable": e.durable})).collect::<Vec<_>>(),
            "landed": verdict.landed.iter().map(|l| format!("{l:?}")).collect::<Vec<_>>(),
            "port_gone_s": times.gone,
            "port_back_s": times.back,
            "cut_est_after_op_start_s": cut_at,
            "last_answer_s": last_rx,
            "last_send_s": last_tx,
            "acked_before_cut": acked_before_cut,
            "power_cycle": pc_record.as_ref().map(|p| serde_json::json!({"spawned_s": p.spawned, "exited_s": p.exited, "code": p.code})),
            "boot_line": boot_line,
            "heartbeat_reset_reason": heartbeat_reason,
            "fs": fs,
            "fs_line": fs_line,
            "mount_us": mount_us,
            "files": observed.len(),
            "readback_retries": retries,
            "flash": flash,
            "failures": failures,
            "cycle_s": cycle_epoch.elapsed().as_secs_f64(),
        });
        log.cycle(&record);
        log.console(&format!("cycle {n} {}", op.name()), &cycle_console.take());
        eprintln!(
            "cycle {n:5} {:9} cut {:>7} {:?} fs={fs} {} {}",
            op.name(),
            cut_at.map_or("-".to_string(), |c| format!("{c:+.2}s")),
            verdict.landed,
            fs_line.as_deref().unwrap_or("(no [FS] line)"),
            if failures.is_empty() {
                "ok".to_string()
            } else {
                format!("FAIL: {}", failures.join("; "))
            }
        );
        if !failures.is_empty() {
            std::fs::write(
                cfg.dir.join("failures").join(format!("cycle-{n:05}.txt")),
                serde_json::to_string_pretty(&record).unwrap(),
            )
            .ok();
        }
        // The next cycle starts from what the board holds now.
        if !observed.is_empty() {
            base = observed;
        }
    }
    close(conn).await;
    let summary = serde_json::json!({
        "cycles": tally.cycles,
        "failures": tally.failures,
        "flash_checks": tally.flash_checks,
        "by_op": tally.by_op.iter().map(|(k, v)| (k.to_string(), serde_json::json!({"cuts": v.0, "old": v.1, "new": v.2, "same_or_mixed": v.3}))).collect::<serde_json::Map<_, _>>(),
    });
    std::fs::write(
        cfg.dir
            .join(format!("summary-{}-{}.json", cfg.start, cfg.cycles)),
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .ok();
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    assert_eq!(
        tally.failures,
        0,
        "soak failures (see {})",
        cfg.dir.display()
    );
}

// ---- the operations -----------------------------------------------------------

fn pick(rng: &mut Rng, base: &Files, running: &str) -> Op {
    let has_c40 = base.keys().any(|p| p.contains("/docs/doc-"));
    let r = rng.f64();
    let op = if r < 0.33 {
        Op::Push
    } else if r < 0.36 {
        Op::PushC40
    } else if r < 0.60 {
        Op::Save
    } else if r < 0.80 {
        Op::Panel
    } else {
        Op::Switch
    };
    // A c40 push stays rare and only into a slot that does not already hold
    // one (two c40 copies in a batch is the near-full case M5 walked).
    if op == Op::PushC40 && (has_c40 || running.is_empty()) {
        return Op::Push;
    }
    op
}

/// Run `op`; `events` gets what it changed (durable only when answered).
async fn run_op<Io: ClientIo>(
    client: &mut LpClient<Io>,
    op: Op,
    n: u64,
    running: &str,
    base: &Files,
    events: &mut Vec<Event>,
) -> Result<(), String> {
    match op {
        Op::Push | Op::PushC40 => {
            let files = if op == Op::Push {
                variant(
                    &project_files("projects/test/basic"),
                    &format!("soak push {n}"),
                )
            } else {
                variant(&c40_files(), &format!("soak push-c40 {n}"))
            };
            let expected = host_hash(&files);
            let prefix = format!("/projects/{running}/");
            let mut changes = vec![Change::RemoveTree(prefix.clone())];
            changes.extend(
                files
                    .iter()
                    .map(|(p, b)| Change::Put(format!("{prefix}{p}"), b.clone())),
            );
            changes.push(Change::Startup {
                slot: running.to_string(),
            });
            let i = events.len();
            events.push(Event {
                label: op.name().to_string(),
                changes,
                durable: false,
                atomic_slot: Some(running.to_string()),
            });
            let r = lpa_client::push_project(client, &files, &expected, running, &mut |_, _| {})
                .await
                .map_err(|e| e.to_string())?;
            if r.storage_id != running {
                return Err(format!("pushed into {} not {running}", r.storage_id));
            }
            // Answered: the batch committed. Its startup write stays "old or
            // new" (the board writes it once the load has run).
            let ev = &mut events[i];
            ev.changes.retain(|c| !matches!(c, Change::Startup { .. }));
            ev.durable = true;
            events.push(Event {
                label: "startup after push".into(),
                changes: vec![Change::Startup {
                    slot: running.to_string(),
                }],
                durable: false,
                atomic_slot: None,
            });
            Ok(())
        }
        Op::Save => {
            let path = format!("/projects/{running}/shader.glsl");
            let mut bytes = base.get(&path).cloned().unwrap_or_default();
            // Keep it from growing without bound: drop older save marks.
            if let Some(at) = find(&bytes, b"\n// soak save ") {
                bytes.truncate(at);
            }
            bytes.extend_from_slice(format!("\n// soak save {n}\n").as_bytes());
            let i = events.len();
            events.push(Event {
                label: "save".into(),
                changes: vec![Change::Put(path.clone(), bytes.clone())],
                durable: false,
                atomic_slot: None,
            });
            client
                .fs_write(LpPath::new(&path), bytes)
                .await
                .map_err(|e| e.to_string())?;
            events[i].durable = true;
            Ok(())
        }
        Op::Panel => {
            let value = (n % 1000) as f32 + 0.5;
            events.push(Event {
                label: "panel".into(),
                changes: vec![Change::Panel {
                    path: format!("/projects/{running}/.lp/panel.json"),
                    value,
                }],
                durable: false,
                atomic_slot: None,
            });
            panel_time(client, value).await
        }
        Op::Switch => {
            let other = if running == "demo" { "other" } else { "demo" };
            events.push(Event {
                label: format!("switch to {other}"),
                changes: vec![Change::Startup {
                    slot: other.to_string(),
                }],
                durable: false,
                atomic_slot: None,
            });
            client
                .project_load(&lpa_client::project_deploy::project_load_path(other))
                .await
                .map(drop)
                .map_err(|e| e.to_string())
        }
    }
}

/// `files` with a marker line appended to its shader.
fn variant(files: &[(String, Vec<u8>)], marker: &str) -> Vec<(String, Vec<u8>)> {
    files
        .iter()
        .map(|(p, b)| {
            let mut b = b.clone();
            if p.ends_with(".glsl") {
                b.extend_from_slice(format!("\n// {marker}\n").as_bytes());
            }
            (p.clone(), b)
        })
        .collect()
}

/// The package hash the board computes (`lpc_history::hash_package`), on
/// the host.
fn host_hash(files: &[(String, Vec<u8>)]) -> String {
    let mirror = LpFsMemory::new();
    for (path, bytes) in files {
        mirror
            .write_file(LpPath::new(&format!("/{path}")), bytes)
            .expect("a host write");
    }
    lpc_history::hash_package(&mirror)
        .expect("a host hash")
        .0
        .to_string()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// A panel write to the loaded project's `time` channel.
async fn panel_time<Io: ClientIo>(client: &mut LpClient<Io>, value: f32) -> Result<(), String> {
    let loaded = client
        .project_list_loaded()
        .await
        .map_err(|e| e.to_string())?
        .value;
    let handle = loaded.first().ok_or("no project loaded")?.handle;
    let read: lpc_wire::ProjectReadRequest = serde_json::from_value(serde_json::json!({
        "since": null,
        "probes": [{"binding_graph": {"structure": "always", "include_values": false}}]
    }))
    .expect("a binding-graph read");
    let events = client
        .project_read(handle, read)
        .await
        .map_err(|e| e.to_string())?
        .value;
    let json = serde_json::to_value(&events).expect("events as JSON");
    let scope = find_time_scope(&json).ok_or("no `time` channel")?;
    let request = lpc_wire::WirePanelWriteRequest {
        scope: serde_json::from_value(scope).map_err(|e| e.to_string())?,
        channel: "time".to_string(),
        value: LpValue::F32(value).into(),
        ttl_ms: None,
    };
    client
        .project_panel_write(handle, request)
        .await
        .map(drop)
        .map_err(|e| e.to_string())
}

fn find_time_scope(v: &serde_json::Value) -> Option<serde_json::Value> {
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

// ---- reading back ---------------------------------------------------------------

/// The slot the board runs (`""` when it runs nothing).
async fn running_slot<Io: ClientIo>(client: &mut LpClient<Io>) -> Result<String, String> {
    let loaded = client
        .project_list_loaded()
        .await
        .map_err(|e| e.to_string())?
        .value;
    Ok(loaded
        .first()
        .map(|p| {
            p.path
                .as_str()
                .trim_start_matches('/')
                .trim_start_matches("projects/")
                .trim_end_matches('/')
                .to_string()
        })
        .unwrap_or_default())
}

async fn readback<Io: ClientIo>(client: &mut LpClient<Io>) -> Result<Files, String> {
    let mut reason = None;
    readback_with(client, &mut reason).await
}

/// Every file the board serves, read over the wire (and a heartbeat's reset
/// reason, if one went by).
async fn readback_with<Io: ClientIo>(
    client: &mut LpClient<Io>,
    reason: &mut Option<String>,
) -> Result<Files, String> {
    let listed = client
        .fs_list_dir(LpPath::new("/"), true)
        .await
        .map_err(|e| format!("list /: {e}"))?;
    note_reason(&listed.events, reason);
    let mut out = Files::new();
    for path in listed.value {
        let path = path.as_str().to_string();
        match client.fs_read(LpPath::new(&path)).await {
            Ok(read) => {
                note_reason(&read.events, reason);
                out.insert(path, read.value);
            }
            // A directory listed recursively reads as an error.
            Err(lpa_client::ClientError::Server(_)) => {}
            Err(e) => return Err(format!("read {path}: {e}")),
        }
    }
    Ok(out)
}

fn note_reason(events: &[ClientEvent], reason: &mut Option<String>) {
    for e in events {
        if let ClientEvent::Heartbeat {
            recovery: Some(r), ..
        } = e
        {
            *reason = Some(r.reset_reason.clone());
        }
    }
}

/// Poll (≤ 8 s) for a heartbeat's reset reason.
async fn wait_heartbeat_reason<Io: ClientIo>(client: &mut LpClient<Io>) -> Option<String> {
    let until = Instant::now() + Duration::from_secs(8);
    let mut reason = None;
    while reason.is_none() && Instant::now() < until {
        if let Ok(o) = client.project_list_loaded().await {
            note_reason(&o.events, &mut reason);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    reason
}

/// The project `/lightplayer.json` names is the one running, and its
/// package hash on the board is the host's hash of what it served.
async fn app_check<Io: ClientIo>(client: &mut LpClient<Io>, files: &Files) -> Result<(), String> {
    let named = files
        .get("/lightplayer.json")
        .and_then(|b| startup_slot(b))
        .ok_or("no startup project in /lightplayer.json")?;
    let running = running_slot(client).await?;
    if running != named {
        return Err(format!(
            "`/lightplayer.json` names {named} but the board runs {running:?}"
        ));
    }
    let prefix = format!("/projects/{named}/");
    let slot: Vec<(String, Vec<u8>)> = slot_files(files, &prefix)
        .into_iter()
        .map(|(p, b)| (p[prefix.len()..].to_string(), b))
        .collect();
    let host = host_hash(&slot);
    let board = client
        .hash_package(&named)
        .await
        .map_err(|e| format!("hash {named}: {e}"))?
        .value;
    if board != host {
        return Err(format!(
            "{named}: the board hashes {board}, the files it served hash {host}"
        ));
    }
    Ok(())
}

// ---- records --------------------------------------------------------------------

struct Records {
    cycles: std::fs::File,
    console: std::fs::File,
}

impl Records {
    fn open(dir: &Path) -> Self {
        let open = |name: &str| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(name))
                .expect("a record file")
        };
        Self {
            cycles: open("cycles.jsonl"),
            console: open("console.txt"),
        }
    }

    fn cycle(&mut self, record: &serde_json::Value) {
        writeln!(self.cycles, "{record}").ok();
        self.cycles.flush().ok();
    }

    fn console(&mut self, label: &str, lines: &[(f64, String)]) {
        writeln!(self.console, "===== {label}").ok();
        for (t, l) in lines {
            writeln!(self.console, "{t:9.3} {l}").ok();
        }
        self.console.flush().ok();
    }
}

/// A small seeded generator (splitmix64): two runs with one seed are one run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}
