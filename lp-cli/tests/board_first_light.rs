//! Time to first light on CX1 (plan `lp2025/2026-10-16-0848-tree-store-adoption`,
//! A1): power-on to the first lit LED frame, and its parts, read off the
//! firmware's own `[perf] first light …` line (`fw-esp32c6`'s `first_light`,
//! on the LP timer, which counts from power-on). **Silicon, not emulated.**
//!
//! It flashes one split image's `merged.bin` (the whole chip, `lpfs` blank)
//! and then, for each case, cuts the power with `board power-cycle` at idle
//! and reads the boot's line over the board's USB lp-link, opened the moment
//! the port is back (the line rides the log channel, which the board holds
//! until a host drains it). The line is said only on a power-on boot, so
//! reading it is the check that the cut was one.
//!
//! The cases, in order, each on the store the one before left:
//! - `none`: the formatted, empty filesystem: no project (`first light none
//!   by`);
//! - `basic`: `projects/test/basic` pushed (Studio's replace-and-load);
//! - `c40`: the same project replaced by a c40-sized one (132 files, ~216 KB);
//! - `used` (`LP_FIRST_LIGHT_CHURN` pushes of c40 first, 20 by default): a
//!   written-out store, as a board that has been edited for a while has;
//! - `full` (4 KiB files until the store says no space): nearly full.
//!
//! **It erases the board's filesystem** (the merged image's blank `lpfs`).
//! Run it only on the sacrificial board, leased. Env: `LP_FIRST_LIGHT_DIR`
//! (records), `LP_FIRST_LIGHT_LABEL` (the image: `littlefs`, `fs-tree`),
//! `LP_FIRST_LIGHT_MERGED` (the image to flash; unset, the board is not
//! flashed), `LP_FIRST_LIGHT_CASES` (default `none,basic,c40`),
//! `LP_FIRST_LIGHT_BOOTS` (per case, default 10),
//! `LP_FIRST_LIGHT_HOST_DELAY_MS` (open the link this long after the port
//! is back, so the boot runs with no host draining it), `BOARD_HOLDER`.

#[path = "support/desk_board.rs"]
mod desk_board;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use desk_board::{Console, Desk, PortWatch, close, connect};
use lp_cli::client::cli_connect::CliConnection;
use lpa_client::LpClient;
use lpfs::LpPath;
use tree_store_board::{c40_files, project_files};

/// The line's prefix.
const LINE: &str = "[perf] first light ";

#[test]
#[ignore = "drives the sacrificial CX1 on the desk (module docs)"]
fn time_to_first_light_on_cx1() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(measure()));
}

async fn measure() {
    let dir = PathBuf::from(std::env::var("LP_FIRST_LIGHT_DIR").expect("LP_FIRST_LIGHT_DIR"));
    std::fs::create_dir_all(&dir).expect("the records directory");
    let label = std::env::var("LP_FIRST_LIGHT_LABEL").expect("LP_FIRST_LIGHT_LABEL");
    let cases = std::env::var("LP_FIRST_LIGHT_CASES").unwrap_or_else(|_| "none,basic,c40".into());
    let boots: usize = std::env::var("LP_FIRST_LIGHT_BOOTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let churn: usize = std::env::var("LP_FIRST_LIGHT_CHURN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let holder = std::env::var("BOARD_HOLDER").expect("BOARD_HOLDER");
    let desk = Desk::cx1(&holder);
    desk.check().unwrap_or_else(|e| panic!("{e}"));
    desk.renew(120).expect("the lease");
    let epoch = Instant::now();
    let console = Console::new(epoch);
    let port = desk.port_wait(Duration::from_secs(30)).expect("CX1's port");

    if let Ok(merged) = std::env::var("LP_FIRST_LIGHT_MERGED") {
        let out = Command::new("espflash")
            .args([
                "write-bin",
                "--chip",
                "esp32c6",
                "--port",
                &port,
                "0x0",
                &merged,
            ])
            .output()
            .expect("espflash");
        assert!(
            out.status.success(),
            "flash {merged}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        eprintln!("flashed {merged}");
        // The first boot after the flash formats the blank `lpfs`; a power
        // cycle makes it a power-on boot like the rest.
        assert_eq!(
            desk.port_wait(Duration::from_secs(30)).as_deref(),
            Some(port.as_str())
        );
    }

    let mut conn: Option<CliConnection> = None;
    let mut rows = Vec::new();
    // The first boot of a freshly flashed chip formats: measured, not a case.
    let (c, first) = power_up(&desk, &port, conn.take(), &console, "first boot").await;
    rows.push(serde_json::json!({"case": "first boot", "boots": [first]}));
    conn = Some(c);

    for case in cases.split(',') {
        let c = conn.take().expect("a link");
        let c = prepare(case, c, churn).await;
        let mut got = Vec::new();
        let mut next = Some(c);
        for i in 0..boots {
            let (c, b) =
                power_up(&desk, &port, next.take(), &console, &format!("{case} #{i}")).await;
            got.push(b);
            next = Some(c);
        }
        conn = next;
        rows.push(serde_json::json!({"case": case, "boots": got}));
        desk.renew(120).ok();
    }
    if let Some(c) = conn {
        close(c).await;
    }

    let lines = console.take();
    let mut f =
        std::fs::File::create(dir.join(format!("first-light-{label}-console.txt"))).unwrap();
    for (t, l) in &lines {
        writeln!(f, "{t:9.3} {l}").ok();
    }
    let out = serde_json::json!({
        "label": label,
        "board": "CX1 c6-expendable, MAC 14:C1:9F:E6:54:90",
        "configuration": "silicon:esp32c6",
        "rows": rows,
    });
    std::fs::write(
        dir.join(format!("first-light-{label}.json")),
        serde_json::to_string_pretty(&out).unwrap(),
    )
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

/// Put the store in `case`'s state over the open link.
async fn prepare(case: &str, conn: CliConnection, churn: usize) -> CliConnection {
    let mut client = LpClient::new(conn.client_io());
    match case {
        "none" => {}
        "basic" => {
            client
                .replace_and_load_project("first-light", &project_files("projects/test/basic"))
                .await
                .expect("push basic");
        }
        "c40" => {
            client
                .replace_and_load_project("first-light", &c40_files())
                .await
                .expect("push c40");
        }
        "used" => {
            let base = c40_files();
            for i in 0..churn {
                let mut files = base.clone();
                // Each push different, so every one rewrites its documents.
                for (_, bytes) in files.iter_mut().filter(|(p, _)| p.starts_with("docs/")) {
                    bytes.extend_from_slice(format!(" {i}").as_bytes());
                }
                let t = Instant::now();
                client
                    .replace_and_load_project("first-light", &files)
                    .await
                    .expect("a churn push");
                eprintln!("churn push {i}: {:.1} s", t.elapsed().as_secs_f64());
            }
        }
        "full" => {
            let mut written = 0usize;
            for i in 0..400u32 {
                let fill: Vec<u8> = (0..4_096u32)
                    .map(|j| ((j + i * 7_919).wrapping_mul(2_654_435_761) >> 24) as u8)
                    .collect();
                match client
                    .fs_write(LpPath::new(&format!("/fill/f{i:03}.bin")), fill)
                    .await
                {
                    Ok(_) => written += 1,
                    Err(e) => {
                        eprintln!("full: {written} × 4 KiB, stopped by: {e}");
                        break;
                    }
                }
            }
        }
        other => panic!("unknown case {other:?}"),
    }
    // Let the board settle (its `/lightplayer.json` rewrite after a load).
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        let _ = client.project_list_loaded().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    drop(client);
    conn
}

/// Cut the power at idle (closing `conn` first), open the link at power-up,
/// and read the boot's first-light line (and the mount and compile lines).
async fn power_up(
    desk: &Desk,
    port: &str,
    conn: Option<CliConnection>,
    console: &Console,
    what: &str,
) -> (CliConnection, serde_json::Value) {
    if let Some(c) = conn {
        close(c).await;
    }
    let epoch = Instant::now();
    let boot = Console::new(epoch);
    let watch = PortWatch::start(port, epoch);
    let power = desk.power_cycle_async(epoch);
    let until = Instant::now() + Duration::from_secs(30);
    while watch.times().back.is_none() && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        watch.times().back.is_some(),
        "{what}: the port never came back"
    );
    // A host that opens the port late: the boot runs with nobody draining
    // the link (a board on a USB charger), and the line waits in the log
    // ring for the host.
    if let Some(ms) = std::env::var("LP_FIRST_LIGHT_HOST_DELAY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
    let conn = connect(port, &boot)
        .await
        .unwrap_or_else(|e| panic!("{what}: {e}"));
    while power.lock().unwrap().is_none() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let pc = power.lock().unwrap().clone().unwrap();
    assert_eq!(
        pc.code,
        Some(0),
        "STOP: `board power-cycle` failed: {}",
        pc.output
    );
    // The line: keep the link read until it comes (it is said at the first
    // lit frame, or after 64 unlit frames).
    let mut client = LpClient::new(conn.client_io());
    let until = Instant::now() + Duration::from_secs(15);
    while boot.find(LINE).is_none() && Instant::now() < until {
        let _ = client.project_list_loaded().await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    drop(client);
    let line = boot.find(LINE);
    assert!(
        line.is_some(),
        "STOP: {what}: no `{LINE}…` line in 15 s — not a power-on boot, or it never lit"
    );
    let compile = boot.find("[shader-node] compilation succeeded");
    let fs_line = boot.find("[FS] ");
    let fs = conn
        .hello()
        .map(|h| serde_json::to_value(h).unwrap()["hardware"]["fs"].clone());
    let times = watch.times();
    eprintln!("{what}: {}", line.as_deref().unwrap_or(""));
    for (t, l) in boot.take() {
        console.push(format!("[{what} +{t:.3}] {l}"));
    }
    (
        conn,
        serde_json::json!({
            "what": what,
            "line": line,
            "compile": compile,
            "fs_line": fs_line,
            "fs": fs,
            "port_back_s": times.back,
            "power_cycle_exited_s": pc.exited,
        }),
    )
}
