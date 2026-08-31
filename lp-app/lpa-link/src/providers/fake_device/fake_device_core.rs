//! The scripted fake ESP32 device: state machine, buffers, and the bridge
//! to a REAL host `LpServer`.

use std::collections::VecDeque;
use std::future::Future;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

use fw_host::{HostRuntime, create_memory_server_with};
use lpc_model::AsLpPath;
use lpc_wire::messages::ClientMessage;
use lpfs::{LpFs, LpFsMemory};

use crate::providers::fake_device::failure_injection::FakeFailurePlan;
use crate::providers::fake_device::fake_device_script::{
    FakeBootState, FakeDeviceScript, FakeLightPlayerState, fake_provenance,
};
use crate::stream::ByteStreamError;

/// How often the blank-flash boot ROM repeats its `invalid header` line.
const BLANK_FLASH_EMIT_INTERVAL: Duration = Duration::from_millis(100);

/// Cloneable handle to one scripted fake device.
///
/// The device outlives individual byte streams (a reconnect opens a new
/// stream on the same device) and is shared with the provider's `manage()`
/// implementation (scripted flash/erase) and with tests (failure injection,
/// premature-input assertions).
#[derive(Clone)]
pub struct FakeEsp32Device {
    inner: Arc<Mutex<FakeDeviceCore>>,
}

impl FakeEsp32Device {
    pub fn new(script: FakeDeviceScript) -> Self {
        let phase = FakePhase::fresh(&script.boot);
        let efuse_mac = match &script.boot {
            FakeBootState::LightPlayer(lp) => lp.base_mac.clone(),
            _ => None,
        };
        Self {
            inner: Arc::new(Mutex::new(FakeDeviceCore {
                script,
                efuse_mac,
                phase,
                out: VecDeque::new(),
                out_since: None,
                served_bytes: 0,
                frames_emitted: 0,
                stalled_by_cut: false,
                last_heartbeat: None,
                input_buf: Vec::new(),
                premature_input_bytes: 0,
                premature_input: Vec::new(),
                failure: FakeFailurePlan::none(),
                dtr_high_seen: false,
                last_rts: None,
                reboot_requests: Arc::new(AtomicUsize::new(0)),
                reboots_performed: 0,
            })),
        }
    }

    /// How many `ClientRequest::Reboot`s this device's server has ACCEPTED,
    /// counted by the embedder reset hook the fake installs.
    ///
    /// The hook fires only after the ack frame is written, so a nonzero
    /// count means the client's answer is already on (or heading for) the
    /// wire — the same ordering real firmware gets.
    pub fn reboot_requests(&self) -> usize {
        self.lock().reboot_requests.load(Ordering::SeqCst)
    }

    /// Install (or replace) the stream failure plan. Byte thresholds count
    /// from the device's cumulative served-byte counter, so install plans
    /// BEFORE the traffic they should affect.
    pub fn set_failure_plan(&self, plan: FakeFailurePlan) {
        self.lock().failure = plan;
    }

    /// Total bytes the device has served to readers so far. Useful for
    /// aiming byte-offset failure knobs mid-session.
    pub fn served_bytes(&self) -> usize {
        self.lock().served_bytes
    }

    /// Bytes written by the host while the device was NOT serving (booting,
    /// blank flash, ROM downloader…). Real hardware drops these on the
    /// floor; a nonzero count means the client talked before readiness —
    /// exactly the M5 pull-before-readiness hardware bug.
    pub fn premature_input_bytes(&self) -> usize {
        self.lock().premature_input_bytes
    }

    /// The dropped premature bytes themselves (lossy UTF-8), so a test can
    /// tell DELIBERATELY loss-tolerant traffic (the readiness hello request,
    /// re-sent until answered) from a request that must not be lost (the
    /// M5 pull-before-readiness bug).
    pub fn premature_input(&self) -> String {
        String::from_utf8_lossy(&self.lock().premature_input).into_owned()
    }

    /// Scripted management transition: "flash firmware" — the device becomes
    /// a fresh LightPlayer (empty storage, no identity) whose provenance
    /// records `image_identity`, then reboots.
    ///
    /// The base MAC is NOT fresh: it is burned into efuse, so it survives
    /// every flash and erase this fake can script. The new firmware
    /// reports the same one the board always had.
    pub fn fake_flash(&self, image_identity: &str) {
        let mut core = self.lock();
        let base_mac = core.efuse_mac.clone();
        core.script.boot = FakeBootState::LightPlayer(FakeLightPlayerState {
            provenance: fake_provenance(image_identity),
            base_mac,
            ..FakeLightPlayerState::new()
        });
        core.reset_current();
    }

    /// Scripted management transition: "erase flash" — back to blank flash,
    /// then reboot.
    pub fn fake_erase(&self) {
        let mut core = self.lock();
        core.script.boot = FakeBootState::BlankFlash;
        core.reset_current();
    }

    /// Scripted management transition: "reset runtime" — replay the current
    /// state's boot.
    pub fn reset_runtime(&self) {
        self.lock().reset_current();
    }

    /// Toggle correlated-response dropping at runtime (no reboot): lets a
    /// test starve requests, then heal the device and prove the same
    /// session recovers. No-op outside the LightPlayer boot state.
    pub fn set_drop_responses(&self, drop: bool) {
        if let FakeBootState::LightPlayer(lp) = &mut self.lock().script.boot {
            lp.drop_responses = drop;
        }
    }

    /// The scripted LightPlayer state, when the device is in that boot
    /// state. Backs the fake raw-filesystem read: the image it returns holds
    /// the same files the fake server serves, so a backup taken through the
    /// fake contains what the device actually "has".
    pub(crate) fn light_player_state(&self) -> Option<FakeLightPlayerState> {
        match &self.lock().script.boot {
            FakeBootState::LightPlayer(state) => Some(state.clone()),
            _ => None,
        }
    }

    /// Consume the scripted one-shot manage failure, if any.
    pub(crate) fn take_manage_failure(&self) -> Option<String> {
        self.lock().script.manage_failure.take()
    }

    pub(crate) fn manage_latency(&self) -> Duration {
        self.lock().script.manage_latency
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, FakeDeviceCore> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Where the device currently is in its boot lifecycle.
enum FakePhase {
    /// Non-LightPlayer states (blank flash / ROM downloader / foreign
    /// firmware): announcement-only output.
    Passive {
        announced: bool,
        last_emit: Option<Instant>,
    },
    /// LightPlayer before `boot_delay` elapsed: silent, input discarded.
    BootingLp { since: Instant },
    /// LightPlayer serving: a real host `LpServer` on its own thread.
    RunningLp { runtime: HostRuntime },
}

impl FakePhase {
    fn fresh(boot: &FakeBootState) -> Self {
        match boot {
            FakeBootState::LightPlayer(_) => Self::BootingLp {
                since: Instant::now(),
            },
            _ => Self::Passive {
                announced: false,
                last_emit: None,
            },
        }
    }
}

pub(crate) struct FakeDeviceCore {
    script: FakeDeviceScript,
    /// The board's factory base MAC, held at DEVICE level because that is
    /// where the real one lives: efuse outlives every boot state, so a
    /// flash or an erase must not be able to change it.
    efuse_mac: Option<String>,
    phase: FakePhase,
    /// Device→host bytes not yet served to the reader.
    out: VecDeque<u8>,
    /// When `out` last became non-empty (read-latency reference point).
    out_since: Option<Instant>,
    /// Cumulative bytes served to readers (failure-knob offsets).
    served_bytes: usize,
    /// Protocol frames fully emitted (mid-frame-cut counting).
    frames_emitted: usize,
    /// A mid-frame cut happened: stop responding, no EOF.
    stalled_by_cut: bool,
    /// When the last synthetic heartbeat was emitted (scripts with
    /// `heartbeat_interval`; the host server never heartbeats on its own).
    last_heartbeat: Option<Instant>,
    /// Host→device bytes not yet forming a complete line.
    input_buf: Vec<u8>,
    premature_input_bytes: usize,
    premature_input: Vec<u8>,
    failure: FakeFailurePlan,
    dtr_high_seen: bool,
    last_rts: Option<bool>,
    /// Reboots the server's reset hook has asked for, cumulative. Shared
    /// with the server thread (the hook runs there), and held at DEVICE
    /// level because the counter must survive the reboot it causes.
    reboot_requests: Arc<AtomicUsize>,
    /// Reboots this device has actually performed — the difference from
    /// [`Self::reboot_requests`] is a reset still owed.
    reboots_performed: usize,
}

impl FakeDeviceCore {
    /// Reset to the current state's boot: clear the wire, drop any running
    /// server, start the boot over.
    fn reset_current(&mut self) {
        self.out.clear();
        self.out_since = None;
        self.input_buf.clear();
        self.stalled_by_cut = false;
        self.last_heartbeat = None;
        // Dropping a RunningLp phase drops the HostRuntime, which joins the
        // server thread (bounded).
        self.phase = FakePhase::fresh(&self.script.boot);
    }

    /// Drive the state machine and pump server frames into `out`.
    fn advance(&mut self) {
        match &self.script.boot {
            FakeBootState::BlankFlash => {
                let FakePhase::Passive {
                    announced,
                    last_emit,
                } = &mut self.phase
                else {
                    return;
                };
                let first = !*announced;
                let due = last_emit.is_none_or(|at| at.elapsed() >= BLANK_FLASH_EMIT_INTERVAL);
                if !due {
                    return;
                }
                *announced = true;
                *last_emit = Some(Instant::now());
                let mut lines: Vec<String> = Vec::new();
                if first {
                    lines.push("ESP-ROM:esp32c6-20220919".to_string());
                }
                lines.push("invalid header: 0xffffffff".to_string());
                for line in lines {
                    self.push_line(&line);
                }
            }
            FakeBootState::RomDownloadMode => {
                if let FakePhase::Passive { announced, .. } = &mut self.phase
                    && !*announced
                {
                    *announced = true;
                    for line in [
                        "ESP-ROM:esp32c6-20220919",
                        "boot:0x16 (DOWNLOAD(USB/UART0/SDIO_REI_FEO))",
                        "waiting for download",
                    ] {
                        self.push_line(line);
                    }
                }
            }
            FakeBootState::ForeignFirmware => {
                if let FakePhase::Passive { announced, .. } = &mut self.phase
                    && !*announced
                {
                    *announced = true;
                    for line in [
                        "ESP-ROM:esp32c6-20220919",
                        "Hello from Seeed Studio XIAO ESP32-C6",
                    ] {
                        self.push_line(line);
                    }
                }
            }
            FakeBootState::LightPlayer(lp) => match &self.phase {
                FakePhase::BootingLp { since } => {
                    if since.elapsed() < lp.boot_delay {
                        return;
                    }
                    let lp = lp.clone();
                    self.finish_light_player_boot(&lp);
                }
                FakePhase::RunningLp { .. } => {
                    let heartbeat_interval = lp.heartbeat_interval;
                    self.emit_heartbeat_if_due(heartbeat_interval);
                    self.pump_server_frames();
                    self.reboot_if_owed();
                }
                FakePhase::Passive { .. } => {}
            },
        }
    }

    /// Emit the boot banner (including the real M2-shaped server-start
    /// line) and start the real host server over a seeded memory fs.
    fn finish_light_player_boot(&mut self, lp: &FakeLightPlayerState) {
        self.push_line("ESP-ROM:esp32c6-20220919");
        self.push_line("[INIT] LightPlayer fake device booting");
        self.push_line(&format!(
            "[INIT] fw-esp32 initialized, starting server loop... proto={} commit={} dirty={}",
            lpc_wire::WIRE_PROTO_VERSION,
            lp.provenance.commit,
            lp.provenance.dirty,
        ));

        let files = lp.project_files.clone();
        let load_at_boot = lp.load_project_at_boot;
        let project_dir = lp.project_dir.clone();
        let identity = lp.identity.clone();
        let base_mac = lp.base_mac.clone();
        let reboot_requests = Arc::clone(&self.reboot_requests);
        let hello_identity = lp
            .provenance
            .clone()
            .with_proto(lp.proto_override.unwrap_or(lpc_wire::WIRE_PROTO_VERSION))
            .with_device_uid(identity.as_ref().map(|identity| identity.uid.clone()));
        let start = HostRuntime::start_with_server(move || {
            let fs = LpFsMemory::new();
            for (relative, bytes) in &files {
                let path = format!("{project_dir}/{relative}");
                if let Err(error) = fs.write_file(path.as_path(), bytes) {
                    eprintln!("[fake-device] failed to seed {path}: {error}");
                }
            }
            if let Some(identity) = &identity {
                // identity is device-scoped: stamped at the fs ROOT, not
                // inside the project storage dir
                let json = lpc_wire::json::to_string(identity)
                    .expect("device identity serializes to JSON");
                if let Err(error) =
                    fs.write_file(fw_host::DEVICE_IDENTITY_PATH.as_path(), json.as_bytes())
                {
                    eprintln!("[fake-device] failed to stamp identity: {error}");
                }
            }
            let mut server = create_memory_server_with(fs, hello_identity);
            // The embedder's reset action (real firmware calls
            // `software_reset()` here). Recording rather than resetting is
            // the point: the server fires this only AFTER the ack frame is
            // written, so the device core can hold the reset until those
            // bytes reach the reader — the ordering silicon gives for free.
            server.set_reboot_hook(Some(Rc::new(move || {
                reboot_requests.fetch_add(1, Ordering::SeqCst);
            })));
            // The efuse half of the hello: only the embedder can read it,
            // so the fake plays embedder here (A1 identity evidence).
            if base_mac.is_some() {
                server.set_hardware_identity(lpc_wire::HardwareIdentity {
                    base_mac: base_mac.clone(),
                    ..lpc_wire::HardwareIdentity::default()
                });
            }
            if load_at_boot {
                // the real-hardware shape: firmware auto-resumes its
                // startup project before serving (fw-esp32c6 boot.rs)
                if let Err(error) = server.load_project(project_dir.as_path()) {
                    eprintln!("[fake-device] boot auto-load failed: {error}");
                }
            }
            server
        });
        match start {
            Ok(runtime) => {
                self.phase = FakePhase::RunningLp { runtime };
                // The server loop sends the unsolicited id-0 hello as its
                // first frame; the next `advance()` pumps it onto the wire.
            }
            Err(error) => {
                self.push_line(&format!("[fake-device] server start failed: {error}"));
                self.phase = FakePhase::Passive {
                    announced: true,
                    last_emit: None,
                };
            }
        }
    }

    /// Move any frames the real server produced onto the byte wire as
    /// `M!<json>\n` lines, applying frame-level injection knobs.
    fn pump_server_frames(&mut self) {
        loop {
            if self.stalled_by_cut {
                return;
            }
            let FakePhase::RunningLp { runtime } = &self.phase else {
                return;
            };
            let transport = runtime.client_transport();
            let received = poll_once(async {
                let mut transport = transport.lock().await;
                transport.receive().await
            });
            let frame = match received {
                Some(Ok(frame)) => frame,
                // Server side gone: nothing more will arrive; leave the
                // wire quiet (a real dead firmware also just goes silent).
                Some(Err(_)) => return,
                None => return,
            };
            // Scripted pre-hello firmware: swallow every hello at the wire
            // (unsolicited AND requested) while the rest of the protocol
            // keeps flowing.
            let suppress_hello = matches!(
                &self.script.boot,
                FakeBootState::LightPlayer(lp) if lp.suppress_hello
            );
            if suppress_hello && matches!(frame.msg, lpc_wire::ServerMsgBody::Hello(_)) {
                continue;
            }
            // Scripted response starvation: correlated responses (id != 0)
            // die at the wire while unsolicited frames keep flowing —
            // firmware dropping responses under engine load.
            let drop_responses = matches!(
                &self.script.boot,
                FakeBootState::LightPlayer(lp) if lp.drop_responses
            );
            if drop_responses && frame.id != 0 {
                continue;
            }
            if !self.emit_wire_frame(&frame) {
                return;
            }
        }
    }

    /// Emit one synthetic unsolicited id-0 heartbeat when the script's
    /// cadence says it is due. Real firmware's server loop heartbeats every
    /// 5 s; the fake's host `LpServer` never does, so scripts that need a
    /// live-but-not-answering wire opt in via `heartbeat_interval`.
    ///
    /// The heartbeat carries the same identity the firmware loop stamps
    /// (R4a: uid from the scripted stamp, MAC from efuse), so host tests
    /// cover the passive mid-stream resolution path — attaching to a device
    /// that booted long ago and learning who it is without a hello.
    fn emit_heartbeat_if_due(&mut self, interval: Option<Duration>) {
        let Some(interval) = interval else {
            return;
        };
        if self.stalled_by_cut {
            return;
        }
        let due = self
            .last_heartbeat
            .is_none_or(|at| at.elapsed() >= interval);
        if !due {
            return;
        }
        self.last_heartbeat = Some(Instant::now());
        let identity = self.heartbeat_identity();
        let frame = lpc_wire::WireServerMessage::new(
            0,
            lpc_wire::ServerMsgBody::Heartbeat {
                fps: lpc_wire::server::SampleStats {
                    avg: 0.0,
                    sdev: 0.0,
                    min: 0.0,
                    max: 0.0,
                },
                frame_count: 0,
                loaded_projects: Vec::new(),
                uptime_ms: 0,
                memory: None,
                recovery: None,
                outputs: None,
                link: None,
                identity,
            },
        );
        self.emit_wire_frame(&frame);
    }

    /// The identity this device announces on its heartbeats, mirroring
    /// `LpServer::heartbeat_identity`: the stamped uid and the efuse MAC,
    /// or `None` when the script gave it neither.
    fn heartbeat_identity(&self) -> Option<lpc_wire::HeartbeatIdentity> {
        let FakeBootState::LightPlayer(lp) = &self.script.boot else {
            return None;
        };
        let identity = lpc_wire::HeartbeatIdentity {
            device_uid: lp.identity.as_ref().map(|identity| identity.uid.clone()),
            base_mac: self.efuse_mac.clone(),
        };
        match identity.is_empty() {
            true => None,
            false => Some(identity),
        }
    }

    /// Perform a reset the server's reboot hook asked for — but only once
    /// its ack has left the wire.
    ///
    /// This is where the fake reproduces the ANSWER-THEN-RESET contract.
    /// The hook fires on the server thread once the in-proc transport
    /// accepted the ack, so the frame is already in the transport channel
    /// when the flag appears: pump it into the byte wire, and hold the reset
    /// until the reader has taken those bytes. Resetting earlier would clear
    /// `out` and eat the answer, which real hardware does not do — it
    /// finishes the UART write before the ROM restarts.
    fn reboot_if_owed(&mut self) {
        if self.reboot_requests.load(Ordering::SeqCst) == self.reboots_performed {
            return;
        }
        self.pump_server_frames();
        if !self.out.is_empty() {
            return;
        }
        self.reboots_performed = self.reboot_requests.load(Ordering::SeqCst);
        self.reset_current();
    }

    /// Serialize one protocol frame onto the byte wire as an `M!<json>\n`
    /// line, applying the frame-level injection knobs (log flood,
    /// mid-frame cut). Returns `false` when the wire stalled on a cut.
    fn emit_wire_frame(&mut self, frame: &lpc_wire::WireServerMessage) -> bool {
        let json = match lpc_wire::json::to_string(frame) {
            Ok(json) => json,
            Err(error) => {
                eprintln!("[fake-device] failed to serialize frame: {error}");
                return true;
            }
        };
        if let Some(flood) = self.failure.log_flood_line.clone() {
            // Logs and frames share the wire on real hardware.
            self.push_line(&flood);
        }
        let frame_line = format!("M!{json}\n");
        if self.failure.cut_mid_frame_after_frames == Some(self.frames_emitted) {
            let cut = frame_line.len() / 2;
            self.push_bytes(&frame_line.as_bytes()[..cut]);
            self.stalled_by_cut = true;
            return false;
        }
        self.push_bytes(frame_line.as_bytes());
        self.frames_emitted += 1;
        true
    }

    /// Serve up to `buf.len()` bytes from the device, applying the failure
    /// plan (latency, stall, disconnect, garble, drop).
    pub(crate) fn serve_read(&mut self, buf: &mut [u8]) -> Result<usize, ByteStreamError> {
        self.advance();

        if let Some(threshold) = self.failure.disconnect_read_after_bytes
            && self.served_bytes >= threshold
        {
            return Err(ByteStreamError::Closed);
        }
        if self.stalled_by_cut && self.out.is_empty() {
            return Ok(0);
        }
        if let Some(threshold) = self.failure.stall_read_after_bytes
            && self.served_bytes >= threshold
        {
            return Ok(0);
        }
        if self.out.is_empty() {
            return Ok(0);
        }
        if let Some(since) = self.out_since
            && since.elapsed() < self.failure.read_latency
        {
            return Ok(0);
        }

        // Cap the chunk so byte-offset thresholds land exactly on a call
        // boundary (the NEXT read observes the stall/disconnect).
        let mut limit = buf.len().min(self.out.len());
        for threshold in [
            self.failure.stall_read_after_bytes,
            self.failure.disconnect_read_after_bytes,
        ]
        .into_iter()
        .flatten()
        {
            limit = limit.min(threshold.saturating_sub(self.served_bytes));
        }

        let mut written = 0;
        for _ in 0..limit {
            let Some(mut byte) = self.out.pop_front() else {
                break;
            };
            let offset = self.served_bytes;
            self.served_bytes += 1;
            if self.failure.drop_byte_at == Some(offset) {
                continue;
            }
            if self.failure.garble_byte_at == Some(offset) {
                byte ^= 0xFF;
            }
            buf[written] = byte;
            written += 1;
        }
        if self.out.is_empty() {
            self.out_since = None;
        }
        Ok(written)
    }

    /// Accept host→device bytes: feed the running server, or discard (and
    /// count) them exactly like real hardware whose server is not up.
    pub(crate) fn accept_write(&mut self, bytes: &[u8]) -> Result<(), ByteStreamError> {
        if self.failure.write_latency > Duration::ZERO {
            std::thread::sleep(self.failure.write_latency);
        }
        // Make boot-completion race-free for writers: a write that arrives
        // after the boot delay elapsed (but before any read poll) should
        // reach the server, not count as premature.
        self.advance();
        if !matches!(self.phase, FakePhase::RunningLp { .. }) {
            self.premature_input_bytes += bytes.len();
            self.premature_input.extend_from_slice(bytes);
            return Ok(());
        }
        self.input_buf.extend_from_slice(bytes);
        while let Some(newline) = self.input_buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.input_buf.drain(..=newline).collect();
            let Ok(line) = std::str::from_utf8(&line_bytes[..line_bytes.len() - 1]) else {
                continue;
            };
            let line = line.trim_end_matches('\r');
            let Some(json) = line.strip_prefix("M!") else {
                continue;
            };
            match lpc_wire::json::from_str::<ClientMessage>(json) {
                Ok(message) => self.forward_to_server(message),
                Err(error) => {
                    eprintln!("[fake-device] malformed client frame: {error}");
                }
            }
        }
        Ok(())
    }

    fn forward_to_server(&mut self, message: ClientMessage) {
        let FakePhase::RunningLp { runtime } = &self.phase else {
            return;
        };
        let transport = runtime.client_transport();
        let sent = poll_once(async {
            let mut transport = transport.lock().await;
            transport.send(message).await
        });
        match sent {
            Some(Ok(())) => {}
            Some(Err(error)) => eprintln!("[fake-device] server rejected frame: {error}"),
            None => eprintln!("[fake-device] server send did not complete"),
        }
    }

    /// Track DTR/RTS writes and recognize the two reset dances:
    ///
    /// - Any DTR-high write marks the sequence as the usb-jtag-download
    ///   dance (`R0 D0 W100 D1 R0 W100 R1 D0 R1 W100 R0 D0`) — neither
    ///   hard-reset variant ever raises DTR.
    /// - An RTS falling edge (true→false) completes a dance: download mode
    ///   if DTR went high, otherwise a hard reset replaying the current
    ///   state's boot.
    pub(crate) fn set_signals(&mut self, dtr: Option<bool>, rts: Option<bool>) {
        if dtr == Some(true) {
            self.dtr_high_seen = true;
        }
        if let Some(rts) = rts {
            let falling = self.last_rts == Some(true) && !rts;
            self.last_rts = Some(rts);
            if falling {
                if self.dtr_high_seen {
                    self.dtr_high_seen = false;
                    self.script.boot = FakeBootState::RomDownloadMode;
                    self.reset_current();
                } else {
                    self.reset_current();
                }
            }
        }
    }

    /// A reopen (baud change) flushes the wire but does not reboot the
    /// device — matching a real port close/reopen.
    pub(crate) fn reopen(&mut self) {
        self.out.clear();
        self.out_since = None;
        self.input_buf.clear();
    }

    fn push_line(&mut self, line: &str) {
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\n');
        self.push_bytes(&bytes);
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        if self.out.is_empty() && !bytes.is_empty() {
            self.out_since = Some(Instant::now());
        }
        self.out.extend(bytes.iter().copied());
    }
}

/// Poll a future exactly once with a no-op waker; `None` when pending.
///
/// The fake device bridges the sync byte stream to the server's tokio
/// channels: channel sends and non-empty receives complete on the first
/// poll, and a pending receive simply means "no frame yet" — the serial
/// thread polls again on its next loop.
fn poll_once<F: Future>(future: F) -> Option<F::Output> {
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => Some(output),
        Poll::Pending => None,
    }
}
