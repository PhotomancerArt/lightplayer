//! The scripted fake ESP32 device: state machine, buffers, and the bridge
//! to a REAL host `LpServer`.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

use fw_host::{HostRuntime, create_memory_server_with};
use lpc_model::AsLpPath;
use lpc_wire::messages::ClientMessage;
use lpfs::{LpFs, LpFsMemory};

use crate::providers::fake_device::failure_injection::FakeFailurePlan;
use crate::providers::fake_device::fake_device_script::{
    FAKE_DEVICE_PROJECT_DIR, FakeBootState, FakeDeviceScript, FakeLightPlayerState, fake_provenance,
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
        Self {
            inner: Arc::new(Mutex::new(FakeDeviceCore {
                script,
                phase,
                out: VecDeque::new(),
                out_since: None,
                served_bytes: 0,
                frames_emitted: 0,
                stalled_by_cut: false,
                input_buf: Vec::new(),
                premature_input_bytes: 0,
                failure: FakeFailurePlan::none(),
                dtr_high_seen: false,
                last_rts: None,
            })),
        }
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

    /// Scripted management transition: "flash firmware" — the device becomes
    /// a fresh LightPlayer (empty storage, no identity) whose provenance
    /// records `image_identity`, then reboots.
    pub fn fake_flash(&self, image_identity: &str) {
        let mut core = self.lock();
        core.script.boot = FakeBootState::LightPlayer(FakeLightPlayerState {
            provenance: fake_provenance(image_identity),
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
    /// Host→device bytes not yet forming a complete line.
    input_buf: Vec<u8>,
    premature_input_bytes: usize,
    failure: FakeFailurePlan,
    dtr_high_seen: bool,
    last_rts: Option<bool>,
}

impl FakeDeviceCore {
    /// Reset to the current state's boot: clear the wire, drop any running
    /// server, start the boot over.
    fn reset_current(&mut self) {
        self.out.clear();
        self.out_since = None;
        self.input_buf.clear();
        self.stalled_by_cut = false;
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
                FakePhase::RunningLp { .. } => self.pump_server_frames(),
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
        let identity = lp.identity.clone();
        let hello = lpc_wire::ServerHello {
            proto: lp.proto_override.unwrap_or(lpc_wire::WIRE_PROTO_VERSION),
            fw: lp.provenance.clone(),
            device_uid: identity.as_ref().map(|identity| identity.uid.clone()),
        };
        let start = HostRuntime::start_with_server(move || {
            let fs = LpFsMemory::new();
            for (relative, bytes) in &files {
                let path = format!("{FAKE_DEVICE_PROJECT_DIR}/{relative}");
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
            create_memory_server_with(fs, hello)
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
            let json = match lpc_wire::json::to_string(&frame) {
                Ok(json) => json,
                Err(error) => {
                    eprintln!("[fake-device] failed to serialize frame: {error}");
                    continue;
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
                return;
            }
            self.push_bytes(frame_line.as_bytes());
            self.frames_emitted += 1;
        }
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
