//! Shader edits, the way Studio sends them, against an emulated C6 over its
//! USB link and over its Wi-Fi (LAN) link: the free heap and the largest
//! block after each edit, and the edit at which a request is refused
//! (`docs/defects/2026-10-08-shader-edits-over-wi-fi-are-refused-board-memory-busy.md`).
//!
//! Studio applies an edited shader as one `SetArtifactBody`/`ReplaceBody`
//! overlay mutation (`ProjectController::apply_asset_body`): since wire 41
//! the body is the shader's text as one JSON string (`lpc_model::body_bytes`;
//! before, a JSON array of numbers, ~3.5 characters a byte). The "old rule"
//! column is the pre-#1047 gate on the 7,142 B byte-array request the board
//! refused, kept for comparison. Each edit here adds a line of work, so every
//! recompile's code is a little bigger than the last (a comment-only edit
//! compiles to the same code and does not fragment).
//!
//! The board is `lp-cli emu run` in a process of its own: on USB its link
//! socket (`serial:tcp://…`), on the LAN its forward (`lan:127.0.0.1:…`),
//! after a first run that saved the fixture's network over the USB link.
//! The project (`catalog/projects/playful-choker`) is deployed over the
//! link under test. After each edit the driver waits for the recompile and
//! reads the board's runtime status (free bytes, largest block).
//!
//! A request the board's link drops is never answered: after
//! [`REQUEST_NET`] the driver says `NO ANSWER`, opens a new link (as
//! Studio's reconnect does) and goes on, loading the project again if the
//! board came back without it. It asserts nothing past the deploy: it is
//! the measurement, and its table is the report.
//!
//! Every wait is a wall-clock safety net, never a measurement; the heap
//! figures are what transfer. Figures it prints are
//! `lp-emu:esp32c6:t1` (USB) and `lp-emu:esp32c6:t1+net=lan` (LAN).
//! `#[ignore]`d, not in CI: it needs a built `fw-esp32c6` ELF
//! (`LP_EMU_BUILD_FW=1`, or `LP_EMU_C6_ELF_ESP32C6_SERVER_RADIO=<elf>` — a
//! `just fetch-ci-images` tree image), and the LAN run takes minutes.
//!
//! Knobs: `LP_EDIT_FRAG_EDITS` (edits, default 24); `LP_EDIT_FRAG_VIA=fs`
//! (write the shader file instead: the same text as one JSON string);
//! `LP_EDIT_FRAG_HOST_LINK=1` (LAN run: a USB host connected too — the
//! configuration that refuses at edit 5 — and the board's decoded console);
//! `LP_EDIT_FRAG_USB_JOINED=1` (USB run with Wi-Fi joined);
//! `LP_EDIT_FRAG_STOP_ON_RESET=1`; `LP_EDIT_FRAG_NET_FLASH=<file>` (keep the
//! flash that holds the network between runs); `LP_EDIT_FRAG_CONSOLE=<file>`
//! (`emu run`'s output, and the board's console at `<file>.board`).

use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lp_cli::client::cli_connect::{CliConnection, cli_connect_with_password};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpa_client::{HostSpecifier, LpClient};
use lpc_model::{
    ArtifactLocation, AssetBodyOverlay, MutationCmd, MutationCmdBatch, MutationCmdId, MutationOp,
};
use lpc_wire::{ProjectReadRequest, WireOverlayMutationRequest};
use serde_json::{Value, json};

const FIXTURE: &str = r#"# emu_edit_frag.rs: made-up test values only.
[[access_point]]
name = "lp-edit-frag-net"
password = "frag-test-pass-3"
signal_dbm = -50
"#;
const NETWORK_ADD: &str =
    r#"{"networkAdd":{"ssid":"lp-edit-frag-net","password":"frag-test-pass-3"}}"#;

/// The request Yona's board refused (2026-10-08): 7,142 B.
const REPORTED_REQUEST_BYTES: usize = 7_142;

/// Wall-clock pause after an edit for its recompile to land.
const SETTLE: Duration = Duration::from_millis(300);

/// Wall-clock net on one request's answer: a request the board's link drops
/// is never answered, and a reconnect follows.
const REQUEST_NET: Duration = Duration::from_secs(20);

#[test]
#[ignore = "needs a built fw-esp32c6 ELF"]
fn shader_edits_over_usb() {
    let Some(elf) = image() else { return };
    let dir = tempfile::tempdir().unwrap();
    let flash = dir.path().join("flash.bin");
    let usb = free_addr();
    let fixture = dir.path().join("virtual_lan.toml");
    std::fs::write(&fixture, FIXTURE).unwrap();
    let fixture_arg = fixture.to_string_lossy().to_string();
    let mut extra = vec!["--link", usb.as_str()];
    // `LP_EDIT_FRAG_USB_JOINED=1`: the board has joined the LAN too (its
    // station and LAN endpoint up, no LAN client), edited over USB.
    let joined = std::env::var_os("LP_EDIT_FRAG_USB_JOINED").is_some();
    if joined {
        prepared_flash(&elf, &flash, &fixture);
        extra.extend(["--lan", fixture_arg.as_str()]);
    }
    let board = EmulatedBoard::start(&elf, &flash, &extra);
    wait_listening(&usb);
    let rows = run(drive(&format!("serial:tcp://{usb}"), &board));
    let label = if joined {
        "usb (Wi-Fi joined) lp-emu:esp32c6:t1+net=lan"
    } else {
        "usb lp-emu:esp32c6:t1"
    };
    report(label, &rows);
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF"]
fn shader_edits_over_the_lan() {
    let Some(elf) = image() else { return };
    let dir = tempfile::tempdir().unwrap();
    let fixture = dir.path().join("virtual_lan.toml");
    std::fs::write(&fixture, FIXTURE).unwrap();
    let flash = dir.path().join("flash.bin");
    prepared_flash(&elf, &flash, &fixture);
    let fixture_arg = fixture.to_string_lossy().to_string();
    let mut extra = vec!["--lan", fixture_arg.as_str()];
    // `LP_EDIT_FRAG_STOP_ON_RESET=1`: end the run at a reset instead, so
    // `emu run` writes the console it kept (the reset's own words) to
    // `--console`.
    if std::env::var_os("LP_EDIT_FRAG_STOP_ON_RESET").is_none() {
        extra.push("--reboot-on-reset");
    }
    // `LP_EDIT_FRAG_HOST_LINK=1`: host the USB link in `emu run` too, so its
    // console is decoded (log records, `[OOM]` lines); it costs the USB
    // link's own session on the board's heap.
    if std::env::var_os("LP_EDIT_FRAG_HOST_LINK").is_some() {
        extra.push("--host-link");
    }
    let board = EmulatedBoard::start(&elf, &flash, &extra);
    let lan = board.forward();
    let rows = run(drive(&lan, &board));
    report("lan lp-emu:esp32c6:t1+net=lan", &rows);
}

/// A flash holding the fixture's network. `LP_EDIT_FRAG_NET_FLASH=<file>`:
/// one an earlier run saved, so a rerun skips the join over USB.
fn prepared_flash(elf: &Path, flash: &Path, fixture: &Path) {
    match std::env::var_os("LP_EDIT_FRAG_NET_FLASH").map(PathBuf::from) {
        Some(saved) if saved.exists() => {
            std::fs::copy(&saved, flash).unwrap();
        }
        saved => {
            let started = Instant::now();
            save_network(elf, flash, fixture);
            eprintln!("emu_edit_frag: network saved after {:?}", started.elapsed());
            if let Some(saved) = saved {
                std::fs::copy(flash, &saved).unwrap();
            }
        }
    }
}

/// One edit's outcome.
#[derive(Debug)]
struct Row {
    edit: u32,
    body_bytes: usize,
    request_bytes: usize,
    outcome: String,
    free: Option<u64>,
    largest: Option<u64>,
}

async fn drive(address: &str, board: &EmulatedBoard) -> Vec<Row> {
    let started = Instant::now();
    let mut connection = connect(address).await;
    eprintln!("emu_edit_frag: connected after {:?}", started.elapsed());
    let mut client = LpClient::new(connection.client_io());
    let dir = workspace_dir().join("catalog/projects/playful-choker");
    let (uid, _) = lp_cli::commands::dev::validation::validate_local_project(&dir)
        .unwrap_or_else(|e| panic!("{} validates: {e}", dir.display()));
    let files =
        lp_cli::commands::dev::collect_project_deploy_files(&lpfs::LpFsStd::new(dir.clone()))
            .expect("the project's files");
    client
        .deploy_project_files(&uid, files)
        .await
        .unwrap_or_else(|e| panic!("the deploy failed: {e}\n{}", board.output()));
    let loaded = client
        .project_list_loaded()
        .await
        .expect("the loaded projects")
        .value;
    let project = loaded
        .first()
        .cloned()
        .expect("the deploy loaded the choker");
    eprintln!("emu_edit_frag: deployed after {:?}", started.elapsed());
    let mut handle = project.handle;
    let project_path = project.path.as_str().to_string();
    let original = std::fs::read(dir.join("shader.glsl")).expect("the choker's shader");
    tokio::time::sleep(SETTLE).await;

    let mut rows = Vec::new();
    let (free, largest) = memory(&mut client, handle).await;
    rows.push(Row {
        edit: 0,
        body_bytes: original.len(),
        request_bytes: 0,
        outcome: String::from("deployed"),
        free,
        largest,
    });
    let edits: u32 = std::env::var("LP_EDIT_FRAG_EDITS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(24);
    // `LP_EDIT_FRAG_VIA=fs`: write the shader file instead of Studio's
    // overlay mutation.
    let via_fs = std::env::var("LP_EDIT_FRAG_VIA").is_ok_and(|via| via == "fs");
    for edit in 1..=edits {
        let body = edited_shader(&original, edit);
        let batch = MutationCmdBatch::new(vec![MutationCmd {
            id: MutationCmdId::new(u64::from(edit)),
            mutation: MutationOp::SetArtifactBody {
                artifact: ArtifactLocation::file("/shader.glsl"),
                edit: AssetBodyOverlay::ReplaceBody(body.clone()),
            },
        }]);
        let request = WireOverlayMutationRequest::new(batch);
        let (request_bytes, answer) = if via_fs {
            // The other way a shader reaches the board: the file itself
            // (`FsRequest::Write`, its text as one JSON string), which the
            // board reloads and recompiles.
            let shader = lpfs::LpPathBuf::from(format!("{project_path}/shader.glsl"));
            let bytes = fs_write_len(&shader, &body);
            let answer =
                tokio::time::timeout(REQUEST_NET, client.fs_write(shader.as_path(), body.clone()))
                    .await
                    .map(|result| result.map(|outcome| (outcome.events, None)));
            (bytes, answer)
        } else {
            let bytes = request_len(handle, &request);
            let answer =
                tokio::time::timeout(REQUEST_NET, client.project_overlay_mutate(handle, request))
                    .await
                    .map(|result| {
                        result.map(|outcome| {
                            let results =
                                serde_json::to_value(&outcome.value.result).unwrap_or(Value::Null);
                            (outcome.events, Some(results))
                        })
                    });
            (bytes, answer)
        };
        let outcome = match answer {
            Ok(Ok((events, results))) => {
                note_events(&events);
                match results {
                    Some(results) if results.to_string().contains("rejected") => {
                        format!("rejected {results}")
                    }
                    _ => String::from("accepted"),
                }
            }
            Ok(Err(error)) => format!("error: {error}"),
            Err(_) => format!("NO ANSWER in {REQUEST_NET:?}"),
        };
        let mut lost = outcome.contains("NO ANSWER") || outcome.contains("Connection lost");
        let (mut free, mut largest) = (None, None);
        if !lost {
            tokio::time::sleep(SETTLE).await;
            match tokio::time::timeout(REQUEST_NET, memory(&mut client, handle)).await {
                Ok(figures) => (free, largest) = figures,
                Err(_) => lost = true,
            }
        }
        println!(
            "[{:>5.1}s] edit {edit:>2}: body {:>5} B, request {:>5} B, {outcome}; free {free:?}, largest {largest:?}",
            started.elapsed().as_secs_f64(),
            body.len(),
            request_bytes
        );
        rows.push(Row {
            edit,
            body_bytes: body.len(),
            request_bytes,
            outcome,
            free,
            largest,
        });
        if lost && board.output.lock().unwrap().contains("the run ended") {
            println!("  the emulated board stopped (LP_EDIT_FRAG_STOP_ON_RESET)");
            break;
        }
        if lost {
            // A new link, as Studio's reconnect makes one; and the project
            // again if the board came back without it (a reset).
            drop(client);
            connection.close().await;
            connection = connect(address).await;
            client = LpClient::new(connection.client_io());
            let loaded = client
                .project_list_loaded()
                .await
                .map(|outcome| {
                    note_events(&outcome.events);
                    outcome.value
                })
                .unwrap_or_default();
            handle = match loaded.first() {
                Some(project) => project.handle,
                None => {
                    println!("  reconnected: no project loaded (the board reset); loading it");
                    client
                        .project_load(&project_path)
                        .await
                        .expect("the choker loads again")
                        .value
                }
            };
            println!(
                "  reconnected after {:.1}s",
                started.elapsed().as_secs_f64()
            );
        }
    }
    drop(client);
    connection.close().await;
    rows
}

/// Print the side-channel events a response carried: heartbeats' memory and
/// recovery, and log lines.
fn note_events(events: &[lpa_client::ClientEvent]) {
    for event in events {
        match event {
            lpa_client::ClientEvent::Heartbeat {
                uptime_ms,
                memory,
                recovery,
                ..
            } => {
                let memory = memory
                    .as_ref()
                    .map(|m| (m.free_bytes, m.largest_free_block));
                let crash = recovery.as_ref().and_then(|r| r.last_crash.as_ref());
                println!(
                    "  heartbeat {uptime_ms} ms: (free, largest) {memory:?}; last crash {crash:?}"
                );
            }
            lpa_client::ClientEvent::Log { message, .. } => println!("  log: {message}"),
            _ => {}
        }
    }
}

/// The board's free bytes and largest block, off a runtime-only read.
async fn memory(
    client: &mut LpClient<Box<dyn lpa_client::ClientIo>>,
    handle: lpc_wire::WireProjectHandle,
) -> (Option<u64>, Option<u64>) {
    let read: ProjectReadRequest =
        serde_json::from_value(json!({"since":null,"queries":[{"runtime":null}]})).unwrap();
    let Ok(outcome) = client.project_read(handle, read).await else {
        return (None, None);
    };
    note_events(&outcome.events);
    for event in outcome.value {
        let value = serde_json::to_value(&event).unwrap_or(Value::Null);
        let mut found = None;
        find_key(&value, "freeBytes", &mut found);
        if let Some(free) = found {
            let mut largest = None;
            find_key(&value, "largestFreeBlock", &mut largest);
            return (Some(free), largest);
        }
    }
    (None, None)
}

fn find_key(value: &Value, key: &str, out: &mut Option<u64>) {
    if out.is_some() {
        return;
    }
    match value {
        Value::Object(map) => {
            if let Some(n) = map.get(key).and_then(Value::as_u64) {
                *out = Some(n);
                return;
            }
            for v in map.values() {
                find_key(v, key, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|v| find_key(v, key, out)),
        _ => {}
    }
}

/// The JSON a host sends for this mutation (what the board's gate measures).
fn request_len(handle: lpc_wire::WireProjectHandle, request: &WireOverlayMutationRequest) -> usize {
    let message = lpc_wire::ClientMessage {
        id: 1_000_000,
        msg: lpc_wire::ClientRequest::ProjectCommand {
            handle,
            command: lpc_wire::WireProjectCommand::MutateOverlay {
                request: request.clone(),
            },
        },
    };
    lpc_wire::json::to_string(&message)
        .map(|s| s.len())
        .unwrap_or(0)
}

/// The JSON a host sends for an fs write of `bytes` to `path`.
fn fs_write_len(path: &lpfs::LpPathBuf, bytes: &[u8]) -> usize {
    let message = lpc_wire::ClientMessage {
        id: 1_000_000,
        msg: lpc_wire::ClientRequest::Filesystem(lpc_wire::server::FsRequest::Write {
            path: path.clone(),
            data: bytes.to_vec(),
        }),
    };
    lpc_wire::json::to_string(&message)
        .map(|s| s.len())
        .unwrap_or(0)
}

/// What the gate said, before 2026-10-08, about a request of the reported
/// size at these figures (3/4 of the message plus 1 KiB in one block).
fn reported_request_passes(free: u64, largest: u64) -> bool {
    let block = (REPORTED_REQUEST_BYTES * 3 / 4 + 1024) as u64;
    let total = (REPORTED_REQUEST_BYTES + 16 * 1024) as u64;
    largest >= block && free >= total
}

fn report(label: &str, rows: &[Row]) {
    println!("\n{label}: playful-choker, Studio's ReplaceBody edits");
    println!("edit  body B  request B  free B  largest B  old rule, 7142 B  outcome");
    let mut first_refused = None;
    let mut first_gate = None;
    for row in rows {
        let gate = match (row.free, row.largest) {
            (Some(f), Some(l)) => {
                if reported_request_passes(f, l) {
                    "pass"
                } else {
                    first_gate.get_or_insert(row.edit);
                    "REFUSE"
                }
            }
            _ => "?",
        };
        if row.outcome.contains("refused") {
            first_refused.get_or_insert(row.edit);
        }
        println!(
            "{:>4}  {:>6}  {:>9}  {:>6}  {:>9}  {:>16}  {}",
            row.edit,
            row.body_bytes,
            row.request_bytes,
            row.free.map_or("?".into(), |v| v.to_string()),
            row.largest.map_or("?".into(), |v| v.to_string()),
            gate,
            row.outcome
        );
    }
    println!(
        "{label}: first edit refused: {first_refused:?}; first point a 7,142 B request would be \
         refused: {first_gate:?}"
    );
}

/// The shader after `edits` edits: each adds one more line of work to the
/// brightness (`emu_frag_reads.rs`'s edit).
fn edited_shader(original: &[u8], edits: u32) -> Vec<u8> {
    let source = std::str::from_utf8(original).expect("the shader is UTF-8");
    let at = source
        .find(EDIT_ANCHOR)
        .unwrap_or_else(|| panic!("no {EDIT_ANCHOR:?} line in the choker's shader"));
    let mut edited = String::from(&source[..at]);
    for edit in 1..=edits {
        edited.push_str(&format!(
            "    lum *= 1.0 + 0.01 * sin(p.x * {edit}.0 + time);\n"
        ));
    }
    edited.push_str(&source[at..]);
    edited.into_bytes()
}

const EDIT_ANCHOR: &str = "    vec3 color = texture(palette";

/// Save the fixture's network on `flash` over the USB link, and stop once
/// the board has joined it.
fn save_network(elf: &Path, flash: &Path, fixture: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
        .args(["emu", "run", "--elf"])
        .arg(elf)
        .arg("--flash")
        .arg(flash)
        .args(["--host-link", "--lan"])
        .arg(fixture)
        .args(["--request", NETWORK_ADD, "--exit-on", "[wifi] connected"])
        .args(["--timeout", "120s", "--wall-timeout", "500"])
        .output()
        .expect("lp-cli emu run");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(
        said.contains("stopped on --exit-on"),
        "the network was not saved:\n{}",
        tail(&said)
    );
}

struct EmulatedBoard {
    child: Child,
    output: Arc<Mutex<String>>,
    forward: mpsc::Receiver<String>,
}

impl EmulatedBoard {
    fn start(elf: &Path, flash: &Path, extra: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
            .args(["emu", "run", "--elf"])
            .arg(elf)
            .arg("--flash")
            .arg(flash)
            .args(extra)
            .args(console_arg())
            .args(["--timeout", "3600s", "--wall-timeout", "1800"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawning lp-cli emu run");
        let output = Arc::new(Mutex::new(String::new()));
        let (tx, forward) = mpsc::channel();
        drain(
            child.stdout.take().unwrap(),
            Arc::clone(&output),
            tx.clone(),
        );
        drain(child.stderr.take().unwrap(), Arc::clone(&output), tx);
        Self {
            child,
            output,
            forward,
        }
    }

    fn forward(&self) -> String {
        self.forward
            .recv_timeout(Duration::from_secs(120))
            .unwrap_or_else(|_| panic!("no forward:\n{}", self.output()))
    }

    fn output(&self) -> String {
        tail(&self.output.lock().unwrap())
    }
}

impl Drop for EmulatedBoard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `--console <LP_EDIT_FRAG_CONSOLE>.board`: the board's own console, as
/// `emu run --host-link` decodes it, written as it goes.
fn console_arg() -> Vec<String> {
    match std::env::var("LP_EDIT_FRAG_CONSOLE") {
        Ok(path) => vec![String::from("--console"), format!("{path}.board")],
        Err(_) => Vec::new(),
    }
}

fn drain(pipe: impl Read + Send + 'static, output: Arc<Mutex<String>>, tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            let Ok(line) = line else { break };
            if let Some(at) = line.find("lan:127.0.0.1:") {
                let digits: String = line[at + 14..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                if !digits.is_empty() {
                    let _ = tx.send(format!("lan:127.0.0.1:{digits}"));
                }
            }
            if let Some(path) = std::env::var_os("LP_EDIT_FRAG_CONSOLE") {
                use std::io::Write;
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    let _ = writeln!(file, "{line}");
                }
            }
            let mut text = output.lock().unwrap();
            text.push_str(&line);
            text.push('\n');
        }
    });
}

fn tail(text: &str) -> String {
    let start = text.len().saturating_sub(6_000);
    let start = (start..text.len())
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(text.len());
    text[start..].to_string()
}

async fn connect(address: &str) -> CliConnection {
    let spec = HostSpecifier::parse(address).expect("an address lp-cli parses");
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        match cli_connect_with_password(spec.clone(), None, |_| {}).await {
            Ok(connection) => return connection,
            Err(error) if Instant::now() < deadline => {
                eprintln!("emu_edit_frag: connect {address}: {error:#}; retrying");
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            Err(error) => panic!("connecting {address}: {error:#}"),
        }
    }
}

fn image() -> Option<PathBuf> {
    match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("emu_edit_frag: skipped — {reason}");
            None
        }
    }
}

fn free_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}

fn wait_listening(addr: &str) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        if TcpListener::bind(addr).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("the emulated board never listened on {addr}");
}

fn run<F: std::future::Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, future)
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}
