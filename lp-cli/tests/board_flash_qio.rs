//! The core's flash read mode on a desk C6 (PR #1172, plan
//! `lp2025/2026-10-16-0848-tree-store-adoption`). **Silicon, not emulated.**
//!
//! One leased board, one image (already flashed, or `LP_QIO_MERGED`), four
//! parts, each on the store the one before left:
//!
//! 1. **First light** (`LP_QIO_FIRST_LIGHT_BOOTS`, default 10):
//!    `projects/test/basic` pushed, then power-on boots, each reading the
//!    `[perf] first light …` line, the core's one `[flash] …` line and the
//!    hello's `core_sha256` — the running core hashed through the flash
//!    cache, in whatever mode the boot chose — against `split.json`'s.
//! 2. **Read-back soak** (`LP_QIO_SOAK_BOOTS`, default 0): 48 incompressible
//!    4 KiB files written once, then power-on boots, each reading every file
//!    back and comparing bytes, with the same line and hash checks.
//! 3. **Warm resets** (`LP_QIO_WARM_BOOTS`, default 0): the board asked to
//!    restart over its link (`ClientRequest::Reboot`: the firmware's own
//!    `LP_SW_HPSYS` reset, the one an update ends with), the files read back
//!    after each.
//! 4. The files deleted.
//!
//! **It erases the board's filesystem when `LP_QIO_MERGED` is set.** Lease
//! the board first. Env: `BOARD_HOLDER`, `LP_QIO_SLUG` and `LP_QIO_MAC` (the
//! board; CX1 by default), `LP_QIO_LABEL` (e.g. `cx1-qio`), `LP_QIO_DIR`
//! (records), `LP_QIO_SPLIT_JSON` (the image's `split.json`, for the core's
//! hash), `LP_QIO_EXPECT` (`QIO` or `DIO`: what every `[flash]` line must
//! say).

#[path = "support/desk_board.rs"]
mod desk_board;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use desk_board::{CX1_MAC, CX1_SLUG, Console, Desk, PortWatch, close, connect};
use lp_cli::client::cli_connect::CliConnection;
use lpa_client::LpClient;
use lpc_wire::ClientRequest;
use lpfs::LpPath;
use tree_store_board::project_files;

const FILES: u32 = 48;
const FIRST_LIGHT: &str = "[perf] first light ";
const FLASH: &str = "[flash] ";

fn data(i: u32) -> Vec<u8> {
    (0..4_096u32)
        .map(|j| ((j + i * 7_919 + 4_242).wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect()
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
#[ignore = "drives a leased desk C6 (module docs)"]
fn flash_read_mode_on_a_desk_c6() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(sitting()));
}

struct Sitting {
    desk: Desk,
    port: String,
    console: Console,
    core_sha256: String,
    expect: String,
    failures: Vec<String>,
}

async fn sitting() {
    let dir = PathBuf::from(std::env::var("LP_QIO_DIR").expect("LP_QIO_DIR"));
    std::fs::create_dir_all(&dir).expect("the records directory");
    let label = std::env::var("LP_QIO_LABEL").expect("LP_QIO_LABEL");
    let split: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("LP_QIO_SPLIT_JSON").expect("LP_QIO_SPLIT_JSON")).unwrap(),
    )
    .unwrap();
    let holder = std::env::var("BOARD_HOLDER").expect("BOARD_HOLDER");
    let desk = Desk {
        slug: std::env::var("LP_QIO_SLUG").unwrap_or_else(|_| CX1_SLUG.into()),
        mac: std::env::var("LP_QIO_MAC").unwrap_or_else(|_| CX1_MAC.into()),
        holder,
    };
    desk.check().unwrap_or_else(|e| panic!("{e}"));
    desk.renew(120).expect("the lease");
    let port = desk
        .port_wait(Duration::from_secs(30))
        .expect("the board's port");
    let mut s = Sitting {
        console: Console::new(Instant::now()),
        core_sha256: split["core"]["sha256"].as_str().unwrap().to_string(),
        expect: std::env::var("LP_QIO_EXPECT").expect("LP_QIO_EXPECT"),
        failures: Vec::new(),
        desk,
        port,
    };

    if let Ok(merged) = std::env::var("LP_QIO_MERGED") {
        let out = Command::new("espflash")
            .args([
                "write-bin",
                "--chip",
                "esp32c6",
                "--port",
                &s.port,
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
        assert!(s.desk.port_wait(Duration::from_secs(30)).is_some());
    }

    let mut rows = serde_json::Map::new();
    let (mut conn, first) = s.boot(None, "first boot", false).await;
    rows.insert("first boot".into(), serde_json::json!([first]));

    // 1. First light on `basic`.
    {
        let mut client = LpClient::new(conn.client_io());
        client
            .replace_and_load_project("first-light", &project_files("projects/test/basic"))
            .await
            .expect("push basic");
        // Let the board settle after the load (its `/lightplayer.json`
        // rewrite).
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            let _ = client.project_list_loaded().await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    let mut got = Vec::new();
    for i in 0..env_usize("LP_QIO_FIRST_LIGHT_BOOTS", 10) {
        let (c, b) = s
            .boot(Some(conn), &format!("first light #{i}"), false)
            .await;
        conn = c;
        got.push(b);
    }
    rows.insert("first light".into(), serde_json::json!(got));
    s.desk.renew(120).ok();

    // 2. The read-back soak.
    let soak = env_usize("LP_QIO_SOAK_BOOTS", 0);
    let warm = env_usize("LP_QIO_WARM_BOOTS", 0);
    if soak + warm > 0 {
        let mut client = LpClient::new(conn.client_io());
        for i in 0..FILES {
            client
                .fs_write(LpPath::new(&format!("/qio/f{i:03}.bin")), data(i))
                .await
                .unwrap_or_else(|e| panic!("write f{i:03}: {e}"));
        }
        drop(client);
    }
    let mut got = Vec::new();
    for i in 0..soak {
        let (c, mut b) = s.boot(Some(conn), &format!("soak #{i}"), false).await;
        conn = c;
        b["read_back"] = s.read_back(&conn, &format!("soak #{i}")).await;
        got.push(b);
        if i % 20 == 19 {
            s.desk.renew(120).ok();
            eprintln!("soak: {} boots, {} failures", i + 1, s.failures.len());
        }
    }
    rows.insert("soak".into(), serde_json::json!(got));

    // 3. Warm resets.
    let mut got = Vec::new();
    for i in 0..warm {
        let (c, mut b) = s.boot(Some(conn), &format!("warm #{i}"), true).await;
        conn = c;
        b["read_back"] = s.read_back(&conn, &format!("warm #{i}")).await;
        got.push(b);
    }
    rows.insert("warm".into(), serde_json::json!(got));

    // 4. Leave the store as found, less these files.
    if soak + warm > 0 {
        let mut client = LpClient::new(conn.client_io());
        for i in 0..FILES {
            let _ = client
                .fs_delete_file(LpPath::new(&format!("/qio/f{i:03}.bin")))
                .await;
        }
        drop(client);
    }
    close(conn).await;

    let lines = s.console.take();
    let mut f = std::fs::File::create(dir.join(format!("qio-{label}-console.txt"))).unwrap();
    for (t, l) in &lines {
        writeln!(f, "{t:9.3} {l}").ok();
    }
    let out = serde_json::json!({
        "label": label,
        "board": format!("{} {}", s.desk.slug, s.desk.mac),
        "configuration": "silicon:esp32c6",
        "expect": s.expect,
        "core_sha256": s.core_sha256,
        "failures": s.failures,
        "rows": rows,
    });
    std::fs::write(
        dir.join(format!("qio-{label}.json")),
        serde_json::to_string_pretty(&out).unwrap(),
    )
    .unwrap();
    eprintln!("== {label}: {} failures", s.failures.len());
    assert!(s.failures.is_empty(), "{label}: {:#?}", s.failures);
}

impl Sitting {
    /// One boot: a power cycle (or, `warm`, the board's own restart), the
    /// link opened when the port is back, and the boot's lines read.
    async fn boot(
        &mut self,
        conn: Option<CliConnection>,
        what: &str,
        warm: bool,
    ) -> (CliConnection, serde_json::Value) {
        let epoch = Instant::now();
        let boot = Console::new(epoch);
        let watch = PortWatch::start(&self.port, epoch);
        let mut power = None;
        if warm {
            let conn = conn.expect("a link to ask the restart on");
            let mut client = LpClient::new(conn.client_io());
            // The answer may or may not beat the reset; the port going away
            // and coming back is the evidence.
            let _ = tokio::time::timeout(
                Duration::from_secs(3),
                client.send_request(ClientRequest::Reboot),
            )
            .await;
            drop(client);
            close(conn).await;
        } else {
            if let Some(c) = conn {
                close(c).await;
            }
            power = Some(self.desk.power_cycle_async(epoch));
        }
        let until = Instant::now() + Duration::from_secs(30);
        while watch.times().back.is_none() && Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            watch.times().back.is_some(),
            "{what}: the port never came back"
        );
        let conn = connect(&self.port, &boot)
            .await
            .unwrap_or_else(|e| panic!("{what}: {e}"));
        if let Some(power) = power {
            while power.lock().unwrap().is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let pc = power.lock().unwrap().clone().unwrap();
            assert_eq!(pc.code, Some(0), "STOP: `board power-cycle`: {}", pc.output);
        }
        // Keep the link read until the boot's lines are in (first light is
        // said at the first lit frame; only on a power-on boot).
        let mut client = LpClient::new(conn.client_io());
        let until = Instant::now() + Duration::from_secs(15);
        while (boot.find(FLASH).is_none() || (!warm && boot.find(FIRST_LIGHT).is_none()))
            && Instant::now() < until
        {
            let _ = client.project_list_loaded().await;
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        drop(client);
        let flash = boot.find(FLASH);
        let first_light = boot.find(FIRST_LIGHT);
        let reset = boot.find("[RECOVERY] boot: cause=");
        let hello = conn.hello().map(|h| serde_json::to_value(h).unwrap());
        let core_sha256 = hello
            .as_ref()
            .and_then(|h| h["firmware"]["coreSha256"].as_str().map(str::to_string));
        let mode_ok = flash
            .as_deref()
            .is_some_and(|l| l.contains(&format!("{FLASH}{}", self.expect)));
        if !mode_ok {
            self.failures.push(format!("{what}: flash line {flash:?}"));
        }
        if core_sha256.as_deref() != Some(self.core_sha256.as_str()) {
            self.failures.push(format!(
                "{what}: core sha256 {core_sha256:?} (want {})",
                self.core_sha256
            ));
        }
        if !warm && first_light.is_none() {
            self.failures.push(format!("{what}: no first light line"));
        }
        eprintln!(
            "{what}: {} | {}",
            flash.as_deref().unwrap_or("-"),
            first_light.as_deref().unwrap_or("-")
        );
        for (t, l) in boot.take() {
            self.console.push(format!("[{what} +{t:.3}] {l}"));
        }
        (
            conn,
            serde_json::json!({
                "what": what,
                "flash": flash,
                "first_light": first_light,
                "reset": reset,
                "core_sha256_ok": core_sha256.as_deref() == Some(self.core_sha256.as_str()),
                "port_back_s": watch.times().back,
            }),
        )
    }

    /// Every soak file read back over the link and compared.
    async fn read_back(&mut self, conn: &CliConnection, what: &str) -> serde_json::Value {
        let mut client = LpClient::new(conn.client_io());
        let t = Instant::now();
        let mut ok = 0;
        for i in 0..FILES {
            match client
                .fs_read(LpPath::new(&format!("/qio/f{i:03}.bin")))
                .await
            {
                Ok(o) if o.value == data(i) => ok += 1,
                Ok(o) => self
                    .failures
                    .push(format!("{what}: f{i:03} differs ({} B)", o.value.len())),
                Err(e) => self.failures.push(format!("{what}: f{i:03} read: {e}")),
            }
        }
        serde_json::json!({"identical": ok, "files": FILES, "seconds": t.elapsed().as_secs_f64()})
    }
}
