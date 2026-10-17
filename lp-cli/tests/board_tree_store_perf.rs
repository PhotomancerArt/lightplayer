//! The tree store's silicon perf on CX1 (plan
//! `lp2025/2026-10-08-1017-tree-store-device-round`, M7 scope 2): what
//! `docs/reports/2026-10-10-tree-store-emulator-walk.md` "On CX1" did not
//! measure — commit times (a save, a panel write, a 4 KiB append), the
//! largest c40 file's read, a whole-project load, and the mount at empty and
//! nearly full. **Silicon, not emulated.** The same operations as the
//! emulated `emu_tree_store_perf`, so the two tables compare row for row.
//!
//! A request's figure is its **round trip** on the host's wall clock (send to
//! answer, one request in flight, over the board's USB lp-link), as the
//! emulated walk's is in emulated time. Each operation is also given with a
//! baseline's round trip taken away (a read of a file of the same size, or a
//! `ListLoadedProjects`), which leaves the store's share and the server's
//! tick. A mount's figure is the firmware's own `[perf] fs mount` line, read
//! at power-up after `board power-cycle` (idle cuts, each checked to be a
//! power-on boot). The panel write's flash work is its auto-save on a later
//! frame: its cost is read off the firmware's `[perf] frames … max=` lines.
//!
//! **It erases the board's `lpfs`** (espflash, over the bootloader) to start
//! from a first-boot format. Run it only on the sacrificial board, leased.
//! Env: `LP_SOAK_DIR` (records), `LP_PERF_LABEL` (the image: `fs-tree` or
//! `littlefs`), `LP_PERF_ERASE=1` (erase first; default on),
//! `LP_PERF_BOOT_ONLY=1` (the first boot and the empty mounts, then stop),
//! `LP_PERF_FILL_PAUSE_MS=<ms>` (quiet time between the near-full fill's
//! writes, the link polled meanwhile: what lets a board clean up on quiet
//! ticks; default 0, back to back) from the fill's file
//! `LP_PERF_FILL_PAUSE_FROM=<n>` on (default 0), `BOARD_HOLDER`.

#[path = "support/desk_board.rs"]
mod desk_board;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use desk_board::{Console, Desk, close, connect, power_up};
use lpa_client::LpClient;
use lpc_model::LpValue;
use lpfs::LpPath;
use tree_store_board::c40_files;

/// Repeats per timed operation.
const REPEATS: usize = 10;

/// 4 KiB files on top of c40 for the nearly full mount (stops at the first
/// "no space").
const FILL_FILES: usize = 200;

/// Time `n` runs of `body` (ms each), inside the calling async fn (the body
/// may `.await` and borrow the caller's client).
macro_rules! timed {
    ($what:expr, $n:expr, $baseline:expr, $i:ident => $body:block) => {{
        let what: String = $what.to_string();
        let mut samples = Vec::new();
        for $i in 0..$n {
            let t = Instant::now();
            let r: Result<(), String> = $body;
            r.unwrap_or_else(|e| panic!("{what} #{}: {e}", $i));
            samples.push(t.elapsed().as_secs_f64() * 1e3);
        }
        eprintln!("{what}: median {:.2} ms", median(&samples));
        Row {
            what,
            samples,
            baseline: $baseline,
            note: String::new(),
        }
    }};
}

#[test]
#[ignore = "drives the sacrificial CX1 on the desk (module docs)"]
fn tree_store_silicon_perf_on_cx1() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(perf()));
}

/// One row: an operation's samples (ms) and its baseline's.
struct Row {
    what: String,
    samples: Vec<f64>,
    baseline: Option<(String, f64)>,
    note: String,
}

impl Row {
    fn json(&self) -> serde_json::Value {
        let m = median(&self.samples);
        serde_json::json!({
            "what": self.what,
            "n": self.samples.len(),
            "median_ms": m,
            "min_ms": self.samples.iter().cloned().fold(f64::INFINITY, f64::min),
            "max_ms": self.samples.iter().cloned().fold(0.0, f64::max),
            "baseline": self.baseline.as_ref().map(|(w, b)| serde_json::json!({"what": w, "median_ms": b})),
            "less_baseline_ms": self.baseline.as_ref().map(|(_, b)| m - b),
            "samples_ms": self.samples,
            "note": self.note,
        })
    }
}

async fn perf() {
    let dir = PathBuf::from(std::env::var("LP_SOAK_DIR").expect("LP_SOAK_DIR"));
    std::fs::create_dir_all(&dir).expect("the records directory");
    let label = std::env::var("LP_PERF_LABEL").unwrap_or_else(|_| "fs-tree".to_string());
    let erase = std::env::var("LP_PERF_ERASE").as_deref() != Ok("0");
    let holder =
        std::env::var("BOARD_HOLDER").unwrap_or_else(|_| "direct: tree-store M7".to_string());
    let desk = Desk::cx1(&holder);
    desk.check().unwrap_or_else(|e| panic!("{e}"));
    desk.renew(120).expect("the lease");
    let epoch = Instant::now();
    let console = Console::new(epoch);
    let mut port = desk.port_wait(Duration::from_secs(30)).expect("CX1's port");
    let mut rows: Vec<Row> = Vec::new();
    let mut mounts: Vec<serde_json::Value> = Vec::new();

    // Empty: a first-boot format, then mounts of the empty store.
    if erase {
        let out = Command::new("espflash")
            .args([
                "erase-region",
                "--chip",
                "esp32c6",
                "--port",
                &port,
                "0x350000",
                "0xB0000",
            ])
            .output()
            .expect("espflash");
        assert!(
            out.status.success(),
            "erase lpfs: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // The board is reset back into its firmware by espflash; power-cycle
        // it so the format is a clean first boot from power-on.
        port = desk.port_wait(Duration::from_secs(30)).expect("CX1's port");
        let (c, m) = power_up(&desk, &port, None, &console, "first boot (format)").await;
        mounts.push(m);
        let mut conn = c;
        for i in 0..3 {
            let (c, m) = power_up(
                &desk,
                &port,
                Some(conn),
                &console,
                &format!("mount, empty #{i}"),
            )
            .await;
            mounts.push(m);
            conn = c;
        }
        close(conn).await;
    }
    if std::env::var("LP_PERF_BOOT_ONLY").as_deref() == Ok("1") {
        write_record(
            &dir,
            &label,
            &console,
            serde_json::json!({ "label": label, "mounts": mounts }),
        );
        return;
    }

    let mut conn = connect(&port, &console).await.expect("CX1's link");
    let hello = conn.hello().map(|h| serde_json::to_value(h).unwrap());
    let mut client = LpClient::new(conn.client_io());

    // The baseline: a request that touches no file.
    rows.push(
        timed!("ListLoadedProjects (baseline: no file)", REPEATS * 2, None, _i => {
            client.project_list_loaded().await.map(drop).map_err(|e| e.to_string())
        }),
    );
    let floor = median(&rows[0].samples);

    // c40: the push, then a load of it.
    let c40 = c40_files();
    let t0 = Instant::now();
    client
        .replace_and_load_project("demo", &c40)
        .await
        .expect("push c40");
    rows.push(Row {
        what: "push c40 (132 files, ~216 KB) and load".into(),
        samples: vec![t0.elapsed().as_secs_f64() * 1e3],
        baseline: None,
        note: "replace_and_load_project: StopAll, one batch, deflated writes, LoadProject".into(),
    });
    tokio::time::sleep(Duration::from_secs(5)).await;

    // A save: the 1.7 KB document written whole, each time different.
    let doc_path = "/projects/demo/docs/doc-000.json";
    let doc = c40
        .iter()
        .find(|(p, _)| p == "docs/doc-000.json")
        .unwrap()
        .1
        .clone();
    let read_doc = timed!("read doc-000 (1.7 KB; the save's baseline)", REPEATS, None, _i => {
        client.fs_read(LpPath::new(doc_path)).await.map(drop).map_err(|e| e.to_string())
    });
    let read_doc_m = median(&read_doc.samples);
    rows.push(read_doc);
    rows.push(timed!(
        "save (1.7 KB document, written whole), each",
        REPEATS,
        Some(("read of the same document".to_string(), read_doc_m)),
        i => {
            let mut d = doc.clone();
            d.extend_from_slice(format!(" {i}").as_bytes());
            client.fs_write(LpPath::new(doc_path), d).await.map(drop).map_err(|e| e.to_string())
        }
    ));

    // 4 KiB appends: three 20 KiB files, each one write and four appends.
    let mut appends = Vec::new();
    for f in 0..3 {
        let big: Vec<u8> = (0..20_480u32)
            .map(|i| b"lightplayer "[((i + f) % 12) as usize])
            .collect();
        let requests = lpa_client::project_deploy::project_write_requests(
            "demo",
            [lpa_client::ProjectDeployFile::new(
                format!("big{f}.txt"),
                big,
            )],
        );
        client
            .send_request(requests[0].clone())
            .await
            .expect("the first chunk");
        for r in &requests[1..] {
            let t = Instant::now();
            client.send_request(r.clone()).await.expect("an append");
            appends.push(t.elapsed().as_secs_f64() * 1e3);
        }
    }
    // Its baseline: a 4 KiB read.
    let four_k: Vec<u8> = (0..4_096u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    client
        .fs_write(LpPath::new("/projects/demo/four-k.bin"), four_k.clone())
        .await
        .expect("a 4 KiB file");
    let read_4k = timed!("read a 4 KiB file (the append's baseline)", REPEATS, None, _i => {
        client
            .fs_read(LpPath::new("/projects/demo/four-k.bin"))
            .await
            .map(drop)
            .map_err(|e| e.to_string())
    });
    let read_4k_m = median(&read_4k.samples);
    rows.push(read_4k);
    rows.push(Row {
        what: "4 KiB append (WriteChunk at an offset, with its file_size check), each".into(),
        samples: appends,
        baseline: Some(("read of a 4 KiB file".into(), read_4k_m)),
        note: "three 20 KiB files, four appends each".into(),
    });

    // The largest c40 file.
    let (largest, bytes) = c40.iter().max_by_key(|(_, b)| b.len()).unwrap().clone();
    let path = format!("/projects/demo/{largest}");
    rows.push(timed!(
        format!("read the largest c40 file ({largest}, {} B), each", bytes.len()),
        REPEATS,
        Some(("ListLoadedProjects".to_string(), floor)),
        _i => { client.fs_read(LpPath::new(&path)).await.map(drop).map_err(|e| e.to_string()) }
    ));

    // A whole-project load (stop + LoadProject; it compiles the shader).
    rows.push(
        timed!("whole-project load (stop + LoadProject c40)", 3, None, _i => {
            match client.stop_all_projects().await {
                Err(e) => Err(e.to_string()),
                Ok(_) => client
                    .project_load(&lpa_client::project_deploy::project_load_path("demo"))
                    .await
                    .map(drop)
                    .map_err(|e| e.to_string()),
            }
        }),
    );
    tokio::time::sleep(Duration::from_secs(3)).await;

    // A panel write: its round trip, then its auto-save on a later frame,
    // read from the `[perf] frames … max=` lines. Spaced past the 10 s
    // auto-save interval; idle windows between them are the control.
    let mut panel = Vec::new();
    let mut panel_at = Vec::new();
    for k in 0..6 {
        let value = 100.5 + k as f32;
        let t = Instant::now();
        panel_time(&mut client, value).await.expect("a panel write");
        panel.push(t.elapsed().as_secs_f64() * 1e3);
        panel_at.push(epoch.elapsed().as_secs_f64());
        // Keep the link read (heartbeats, log records) while waiting.
        let until = Instant::now() + Duration::from_secs(12);
        while Instant::now() < until {
            let _ = client.project_list_loaded().await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    rows.push(Row {
        what: "panel write (WirePanelWrite, the request), each".into(),
        samples: panel,
        baseline: Some(("ListLoadedProjects".into(), floor)),
        note: "its flash work is the auto-save on a later frame: see the frame-time windows".into(),
    });
    // An idle control: 60 s with no writes.
    let idle_from = epoch.elapsed().as_secs_f64();
    let until = Instant::now() + Duration::from_secs(60);
    while Instant::now() < until {
        let _ = client.project_list_loaded().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let idle_to = epoch.elapsed().as_secs_f64();

    // Mount with c40.
    drop(client);
    for i in 0..3 {
        let (c, m) = power_up(
            &desk,
            &port,
            Some(conn),
            &console,
            &format!("mount, c40 #{i}"),
        )
        .await;
        mounts.push(m);
        conn = c;
    }

    // Nearly full: 4 KiB incompressible files until the store says no space.
    let mut client = LpClient::new(conn.client_io());
    let mut written = 0usize;
    let mut fill_rtt = Vec::new();
    let mut stopped = String::new();
    let fill_pause = std::env::var("LP_PERF_FILL_PAUSE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let pause_from = std::env::var("LP_PERF_FILL_PAUSE_FROM")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    for i in 0..FILL_FILES {
        if fill_pause > 0 && i >= pause_from {
            let until = Instant::now() + Duration::from_millis(fill_pause);
            while Instant::now() < until {
                let _ = client.project_list_loaded().await;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        let fill: Vec<u8> = (0..4_096u32)
            .map(|j| ((j + i as u32 * 7_919).wrapping_mul(2_654_435_761) >> 24) as u8)
            .collect();
        let t = Instant::now();
        match client
            .fs_write(LpPath::new(&format!("/fill/f{i:03}.bin")), fill)
            .await
        {
            Ok(_) => {
                fill_rtt.push(t.elapsed().as_secs_f64() * 1e3);
                written += 1;
            }
            Err(e) => {
                stopped = e.to_string();
                break;
            }
        }
    }
    rows.push(Row {
        what: format!("new 4 KiB file (incompressible), each, filling to full ({written} files)"),
        samples: fill_rtt,
        baseline: Some(("read of a 4 KiB file".into(), read_4k_m)),
        note: format!(
            "stopped by: {stopped}; {fill_pause} ms quiet before each write from file {pause_from}"
        ),
    });
    drop(client);
    for i in 0..3 {
        let (c, m) = power_up(
            &desk,
            &port,
            Some(conn),
            &console,
            &format!("mount, full (c40 + {written} × 4 KiB) #{i}"),
        )
        .await;
        mounts.push(m);
        conn = c;
    }
    close(conn).await;

    let out = serde_json::json!({
        "label": label,
        "board": "CX1 c6-expendable, MAC 14:C1:9F:E6:54:90, flash JEDEC 0x464016 (4 MiB)",
        "hello": hello,
        "rows": rows.iter().map(Row::json).collect::<Vec<_>>(),
        "mounts": mounts,
        "panel_writes_at_s": panel_at,
        "idle_window_s": [idle_from, idle_to],
    });
    write_record(&dir, &label, &console, out);
}

/// The record: the console, and `out` as `perf-<label>.json`.
fn write_record(dir: &std::path::Path, label: &str, console: &Console, out: serde_json::Value) {
    let lines = console.take();
    let mut f = std::fs::File::create(dir.join(format!("perf-{label}-console.txt"))).unwrap();
    for (t, l) in &lines {
        writeln!(f, "{t:9.3} {l}").ok();
    }
    std::fs::write(
        dir.join(format!("perf-{label}.json")),
        serde_json::to_string_pretty(&out).unwrap(),
    )
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if s.is_empty() {
        return f64::NAN;
    }
    s[s.len() / 2]
}

/// A panel write to the loaded project's `time` channel.
async fn panel_time<Io: lpa_client::ClientIo>(
    client: &mut LpClient<Io>,
    value: f32,
) -> Result<(), String> {
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
        value: LpValue::F32(value),
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
