//! The desk's sacrificial C6, driven from a host test (plan
//! `lp2025/2026-10-08-1017-tree-store-device-round`, M7): who it is, where
//! its port is, its power, its link and its raw `lpfs`.
//!
//! Every board fact comes from the desk registry (`board`,
//! github.com/PhotomancerArt/lp-board-bench): the port is resolved by MAC
//! (`board show --json`), never by a port-name pattern; power goes through
//! `board power-cycle` only, lease-checked as the holder; and a board that is
//! not tagged `sacrificial` with the allocated MAC is refused (M4's DD6).
//! Nothing here probes the bus or runs `uhubctl`.
//!
//! macOS keeps a powered-off port's device node until power comes back (the
//! node goes and returns at power-up, `lp-board-bench`'s `power.ts` measured
//! it too), so a watcher polling the node every 2 ms stamps the power-up,
//! not the cut; the cut is bounded by the link's last answer
//! ([`StampedIo`]). The link is opened the moment the node is back, so the
//! board's own `[RECOVERY] boot: cause=…` and `[FS] tree store mounted (…)`
//! lines are on the console.

#![allow(dead_code, reason = "each test file uses part of this")]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lp_cli::client::cli_connect::{CliConnection, cli_connect};
use lpa_client::{ClientEvent, HostSpecifier, LpClient};
use lpa_link::{DeviceEvent, DeviceLineOrigin};

/// CX1, the board allocated to be worn out (Yona, 2026-10-08).
pub const CX1_SLUG: &str = "c6-expendable";
/// CX1's MAC, as the registry and its USB serial number give it.
pub const CX1_MAC: &str = "14:C1:9F:E6:54:90";

/// The desk's board, as the registry names it, and who holds it.
pub struct Desk {
    pub slug: String,
    pub mac: String,
    pub holder: String,
}

/// The console: every line the board printed (and the link's own notes),
/// with the host's seconds since `epoch`.
#[derive(Clone)]
pub struct Console {
    pub epoch: Instant,
    pub lines: Rc<RefCell<Vec<(f64, String)>>>,
}

impl Console {
    pub fn new(epoch: Instant) -> Self {
        Self {
            epoch,
            lines: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn push(&self, line: String) {
        let t = self.epoch.elapsed().as_secs_f64();
        self.lines.borrow_mut().push((t, line));
    }

    /// Take everything so far (the next cycle starts empty).
    pub fn take(&self) -> Vec<(f64, String)> {
        std::mem::take(&mut *self.lines.borrow_mut())
    }

    pub fn find(&self, needle: &str) -> Option<String> {
        self.lines
            .borrow()
            .iter()
            .find(|(_, l)| l.contains(needle))
            .map(|(_, l)| l.clone())
    }
}

/// What one `board power-cycle` did.
#[derive(Clone, Debug, Default)]
pub struct PowerCycle {
    /// Host seconds (since the cycle's epoch) the command was started.
    pub spawned: f64,
    /// When it exited, and its exit code.
    pub exited: f64,
    pub code: Option<i32>,
    pub output: String,
}

/// When the port's device node went away and came back (host seconds since
/// the cycle's epoch).
#[derive(Clone, Copy, Debug, Default)]
pub struct PortTimes {
    pub gone: Option<f64>,
    pub back: Option<f64>,
}

impl Desk {
    pub fn cx1(holder: &str) -> Self {
        Self {
            slug: CX1_SLUG.to_string(),
            mac: CX1_MAC.to_string(),
            holder: holder.to_string(),
        }
    }

    /// `board show <slug> --json`.
    pub fn show(&self) -> Result<serde_json::Value, String> {
        let out = Command::new("board")
            .args(["show", &self.slug, "--json"])
            .output()
            .map_err(|e| format!("`board show`: {e} (is `board` installed?)"))?;
        if !out.status.success() {
            return Err(format!(
                "`board show {}` failed: {}",
                self.slug,
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        serde_json::from_slice(&out.stdout).map_err(|e| format!("`board show` JSON: {e}"))
    }

    /// Refuse unless the registry names this board sacrificial, with the
    /// allocated MAC, leased by us.
    pub fn check(&self) -> Result<(), String> {
        let info = self.show()?;
        let tags: Vec<String> = info["tags"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if !tags.iter().any(|t| t == "sacrificial") {
            return Err(format!(
                "REFUSED: {} is not tagged `sacrificial` (tags {tags:?})",
                self.slug
            ));
        }
        let mac = info["mac"].as_str().unwrap_or_default();
        if !mac.eq_ignore_ascii_case(&self.mac) {
            return Err(format!(
                "REFUSED: {} has MAC {mac}, not the allocated {}",
                self.slug, self.mac
            ));
        }
        let holder = info["lease"]["holder"].as_str().unwrap_or_default();
        if holder != self.holder {
            return Err(format!(
                "REFUSED: {} is leased by {holder:?}, not {:?}",
                self.slug, self.holder
            ));
        }
        Ok(())
    }

    /// The board's port now, by MAC (`None` while it is not plugged in or
    /// powered).
    pub fn port(&self) -> Option<String> {
        let info = self.show().ok()?;
        if info["present"].as_bool() != Some(true) {
            return None;
        }
        let serial = info["usbDevice"]["serial"].as_str().unwrap_or_default();
        if !serial.eq_ignore_ascii_case(&self.mac) {
            return None;
        }
        info["port"].as_str().map(str::to_string)
    }

    /// Wait (up to `budget`) for the board's port.
    pub fn port_wait(&self, budget: Duration) -> Option<String> {
        let until = Instant::now() + budget;
        while Instant::now() < until {
            if let Some(p) = self.port() {
                return Some(p);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        None
    }

    /// Extend the lease.
    pub fn renew(&self, minutes: u32) -> Result<(), String> {
        let out = Command::new("board")
            .args([
                "renew",
                &self.slug,
                "--as",
                &self.holder,
                "--minutes",
                &minutes.to_string(),
            ])
            .output()
            .map_err(|e| format!("`board renew`: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!(
                "`board renew` failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ))
        }
    }

    /// Start `board power-cycle` on a thread; the result lands in the
    /// returned slot when the command exits.
    pub fn power_cycle_async(&self, epoch: Instant) -> Arc<Mutex<Option<PowerCycle>>> {
        let slot = Arc::new(Mutex::new(None));
        let out = slot.clone();
        let slug = self.slug.clone();
        let holder = self.holder.clone();
        std::thread::spawn(move || {
            let spawned = epoch.elapsed().as_secs_f64();
            let result = Command::new("board")
                .args(["power-cycle", &slug, "--as", &holder])
                .stdin(Stdio::null())
                .output();
            let exited = epoch.elapsed().as_secs_f64();
            let pc = match result {
                Ok(o) => PowerCycle {
                    spawned,
                    exited,
                    code: o.status.code(),
                    output: format!(
                        "{}{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    ),
                },
                Err(e) => PowerCycle {
                    spawned,
                    exited,
                    code: None,
                    output: format!("could not run `board power-cycle`: {e}"),
                },
            };
            *out.lock().unwrap() = Some(pc);
        });
        slot
    }
}

/// Polls a device node every 2 ms and stamps when it goes and comes back.
pub struct PortWatch {
    times: Arc<Mutex<PortTimes>>,
    stop: Arc<Mutex<bool>>,
}

impl PortWatch {
    pub fn start(port: &str, epoch: Instant) -> Self {
        let times = Arc::new(Mutex::new(PortTimes::default()));
        let stop = Arc::new(Mutex::new(false));
        let (t, s, path) = (times.clone(), stop.clone(), PathBuf::from(port));
        std::thread::spawn(move || {
            let mut was = path.exists();
            while !*s.lock().unwrap() {
                let is = path.exists();
                if was && !is {
                    let mut t = t.lock().unwrap();
                    if t.gone.is_none() {
                        t.gone = Some(epoch.elapsed().as_secs_f64());
                    }
                }
                if !was && is {
                    let mut t = t.lock().unwrap();
                    if t.gone.is_some() && t.back.is_none() {
                        t.back = Some(epoch.elapsed().as_secs_f64());
                        break;
                    }
                }
                was = is;
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        Self { times, stop }
    }

    pub fn times(&self) -> PortTimes {
        *self.times.lock().unwrap()
    }
}

impl Drop for PortWatch {
    fn drop(&mut self) {
        *self.stop.lock().unwrap() = true;
    }
}

/// Open the board's link (no reset on open), its console lines into
/// `console`, and wait for its hello.
pub async fn connect(port: &str, console: &Console) -> Result<CliConnection, String> {
    let sink = console.clone();
    let spec = HostSpecifier::Serial {
        port: Some(port.to_string()),
        baud_rate: None,
    };
    let on_event = move |event: DeviceEvent| match event {
        DeviceEvent::LogLine { line, origin } => match origin {
            DeviceLineOrigin::Device => sink.push(line),
            DeviceLineOrigin::Link => sink.push(format!("[link] {line}")),
        },
        DeviceEvent::State { to, .. } => sink.push(format!("[host] state {to:?}")),
        DeviceEvent::ParseAnomaly { detail } => {
            sink.push(format!("[host] parse anomaly: {detail}"))
        }
        _ => {}
    };
    match tokio::time::timeout(Duration::from_secs(25), cli_connect(spec, on_event)).await {
        Ok(Ok(c)) => Ok(c),
        Ok(Err(e)) => Err(format!("connect {port}: {e:#}")),
        Err(_) => Err(format!("connect {port}: no hello in 25 s")),
    }
}

/// A [`ClientIo`](lpa_client::ClientIo) that stamps its last send and last
/// receive (host seconds since `epoch`): the last answer before a cut bounds
/// the cut from below. (macOS keeps a powered-off port's device node until
/// power comes back, so the node's going is the power-up, not the cut.)
pub struct StampedIo<Io> {
    inner: Io,
    epoch: Instant,
    pub last_tx: Rc<std::cell::Cell<Option<f64>>>,
    pub last_rx: Rc<std::cell::Cell<Option<f64>>>,
}

impl<Io> StampedIo<Io> {
    pub fn new(inner: Io, epoch: Instant) -> Self {
        Self {
            inner,
            epoch,
            last_tx: Rc::new(std::cell::Cell::new(None)),
            last_rx: Rc::new(std::cell::Cell::new(None)),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<Io: lpa_client::ClientIo> lpa_client::ClientIo for StampedIo<Io> {
    async fn send(&mut self, msg: lpc_wire::ClientMessage) -> Result<(), lpc_wire::TransportError> {
        let r = self.inner.send(msg).await;
        if r.is_ok() {
            self.last_tx.set(Some(self.epoch.elapsed().as_secs_f64()));
        }
        r
    }

    async fn receive(&mut self) -> Result<lpc_wire::WireServerMessage, lpc_wire::TransportError> {
        let r = self.inner.receive().await;
        if r.is_ok() {
            self.last_rx.set(Some(self.epoch.elapsed().as_secs_f64()));
        }
        r
    }

    async fn close(&mut self) -> Result<(), lpc_wire::TransportError> {
        self.inner.close().await
    }
}

/// Close a link, bounded (a link to a board that lost power may not answer
/// its close).
pub async fn close(connection: CliConnection) {
    let _ = tokio::time::timeout(Duration::from_secs(3), connection.close()).await;
}

/// `lp-cli hardware lpfs save` (the board's raw `lpfs` over the bootloader;
/// the board is reset back into its firmware afterwards). Returns the raw
/// region's path.
pub fn lpfs_save(lp_cli: &Path, port: &str, out: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let o = Command::new(lp_cli)
        .args(["hardware", "lpfs", "save", "--port", port, "--out"])
        .arg(out)
        .output()
        .map_err(|e| format!("lpfs save: {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    let raw = text
        .lines()
        .find_map(|l| l.strip_prefix("raw filesystem: "))
        .and_then(|l| l.split(" (").next())
        .map(PathBuf::from)
        .ok_or_else(|| format!("lpfs save said no raw region:\n{text}"))?;
    Ok(raw)
}

/// `lp-cli hardware tree check --image` (the store's fsck): its verdict
/// line and whether it passed.
pub fn tree_check(lp_cli: &Path, image: &Path) -> (bool, String) {
    match Command::new(lp_cli)
        .args(["hardware", "tree", "check", "--image"])
        .arg(image)
        .output()
    {
        Ok(o) => (
            o.status.success(),
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
        ),
        Err(e) => (false, format!("tree check: {e}")),
    }
}

/// Cut the power at idle (closing `conn` first), open the link at power-up,
/// and read the boot's mount line. Stops unless it was a power-on boot.
pub async fn power_up(
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
    let mut reason = boot
        .find("[RECOVERY] boot: cause=")
        .map(|l| l.contains("cause=power-on"));
    if reason.is_none() {
        let mut client = LpClient::new(conn.client_io());
        let until = Instant::now() + Duration::from_secs(8);
        while reason.is_none() && Instant::now() < until {
            if let Ok(o) = client.project_list_loaded().await {
                for e in &o.events {
                    if let ClientEvent::Heartbeat {
                        recovery: Some(r), ..
                    } = e
                    {
                        reason = Some(r.reset_reason == "power-on");
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }
    assert_eq!(
        reason,
        Some(true),
        "STOP: {what}: the boot was not a power-on"
    );
    let mount_us = boot.find("[perf] fs mount ").and_then(|l| {
        l.split_whitespace()
            .nth(3)
            .and_then(|v| v.parse::<u64>().ok())
    });
    let fs_line = boot.find("[FS] ");
    let fs = conn
        .hello()
        .map(|h| serde_json::to_value(h).unwrap()["hardware"]["fs"].clone());
    eprintln!("{what}: mount {mount_us:?} us, {fs_line:?}, fs {fs:?}");
    for (t, l) in boot.take() {
        console.push(format!("[{what} +{t:.3}] {l}"));
    }
    (
        conn,
        serde_json::json!({"what": what, "mount_us": mount_us, "fs_line": fs_line, "fs": fs}),
    )
}
