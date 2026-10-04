//! Wi-Fi settings on the shipped C6 image, over its USB link (plan
//! `lp2025/2026-10-04-0808-wifi-settings`, P3).
//!
//! The emulated ESP32-C6 runs in this process, stepped slice by slice in
//! EMULATED time, with the product's own host end: a
//! `lpc_wire::WireLinkPort` under an `lpa-client` `LpClient` — the same
//! client calls `lp-cli wifi` makes (`network_status`, `network_set`,
//! `network_forget`). One claim, end to end:
//!
//! - a fresh board has no network and says its firmware cannot join;
//! - credentials set over USB read back as the name and "password set",
//!   never the password;
//! - they survive a reset (the board reboots, its flash kept), so they are
//!   in `lpfs`, not RAM;
//! - and the trusted USB link still cannot read `/.lp/network.json`.
//!
//! Test values only (`lp-walk-net` / `correct-horse-42`), never a real
//! network's. `#[ignore]`d and run by `just test-emu-c6` (it needs a built
//! `fw-esp32c6` ELF, `LP_EMU_BUILD_FW=1`). Times it prints are
//! `lp-emu:esp32c6:t1`.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_emu_esp_common::QueueHandle;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade, UsbHost,
};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpc_model::AsLpPath;
use lpc_wire::server::{StationState, WifiInfo};
use lpc_wire::{
    ClientMessage, ClientRequest, PortRead, TransportError, WifiPassword, WireLinkPort,
    WireServerMessage, WireServerMsgBody,
};

/// Emulated microseconds per slice (the link host's reaction time).
const SLICE_US: u64 = 250;

/// The longest a request may wait for its answer, in emulated seconds.
const ANSWER_BUDGET_S: f64 = 60.0;

/// The longest the board may take to come back after a reboot, in emulated
/// seconds.
const REBOOT_BUDGET_S: f64 = 30.0;

const SSID: &str = "lp-walk-net";
const PASSWORD: &str = "correct-horse-42";

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn wifi_credentials_set_over_usb_survive_a_reset_and_stay_write_only() {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            eprintln!("emu_wifi_settings: skipped — {reason}");
            return;
        }
    };
    let mut io = LinkIo::new(&elf);

    // A fresh board: no network, and an honest station.
    {
        let mut client = lpa_client::LpClient::new(&mut io);
        let hello = block_on(client.hello()).expect("hello").value;
        assert_eq!(hello.proto, lpc_wire::WIRE_PROTO_VERSION);
        let status = block_on(client.network_status()).expect("status").value;
        assert_eq!(status.wifi, None);
        assert!(!status.lan_only);
        assert_eq!(status.station, StationState::Unsupported);

        // Set the network over the trusted USB link.
        let status = block_on(client.network_set(
            Some(String::from(SSID)),
            Some(WifiPassword::new(PASSWORD)),
            None,
            None,
        ))
        .expect("set")
        .value;
        assert_eq!(status.wifi, Some(saved()));
        assert_eq!(status.station, StationState::Unsupported);

        // Reset the board; its flash is kept.
        let ack = block_on(client.send_request(ClientRequest::Reboot)).expect("the reboot ack");
        assert!(
            matches!(ack.value.msg, WireServerMsgBody::Reboot),
            "{:?}",
            ack.value.msg
        );
    }
    io.wait_for_reboot();
    eprintln!(
        "emu_wifi_settings: rebooted at {:.1} s emulated (lp-emu:esp32c6:t1)",
        io.seconds()
    );

    {
        let mut client = lpa_client::LpClient::new(&mut io).with_request_ids_from(1_000);
        let hello = block_on(client.hello()).expect("hello after the reboot").value;
        assert_eq!(hello.proto, lpc_wire::WIRE_PROTO_VERSION);
        // The settings came back from lpfs.
        let status = block_on(client.network_status())
            .expect("status after the reboot")
            .value;
        assert_eq!(status.wifi, Some(saved()), "the network survived the reset");

        // The trusted link still cannot read the file, and the refusal
        // carries no byte of it.
        let error = block_on(client.fs_read("/.lp/network.json".as_path()))
            .expect_err("the network file is write-only");
        let shown = format!("{error} {error:?}");
        assert!(shown.contains("write-only"), "{shown}");
        assert!(!shown.contains(PASSWORD), "{shown}");

        // Forget it.
        let status = block_on(client.network_forget()).expect("forget").value;
        assert_eq!(status.wifi, None);
    }
    assert_eq!(io.machine.reboots(), 1, "one software reboot");
    assert_eq!(io.unparsed, 0, "every message parsed");
    eprintln!(
        "emu_wifi_settings: done at {:.1} s emulated (lp-emu:esp32c6:t1); notes {:?}",
        io.seconds(),
        io.notes
    );
}

fn saved() -> WifiInfo {
    WifiInfo {
        ssid: String::from(SSID),
        has_password: true,
        enabled: true,
    }
}

/// The product's host end of the link over the in-process board, which
/// steps the machine while it waits. A link reset is expected exactly once,
/// at the reboot.
struct LinkIo {
    machine: Esp32C6Machine,
    queue: QueueHandle,
    /// Emulated microseconds stepped since the machine was built — across
    /// the reboot, which restarts the chip's own clock.
    elapsed_us: u64,
    port: WireLinkPort,
    pending: VecDeque<WireServerMessage>,
    unparsed: u32,
    resets: u32,
    ups: u32,
    notes: Vec<String>,
}

impl LinkIo {
    fn new(elf: &std::path::Path) -> Self {
        let machine = Esp32C6Builder::new()
            .app(AppSource::Path(elf.to_path_buf()))
            .time_grade(TimeGrade::T1)
            .reboot_on_reset(true)
            .usb_host(UsbHost::Attached { draining: true })
            .usb_sj_queue_source()
            .build()
            .expect("building the emulated C6");
        let queue = machine
            .usb_sj_host_handle()
            .expect("an in-process USB host queue");
        Self {
            machine,
            queue,
            elapsed_us: 0,
            port: WireLinkPort::new(lpc_wire::lp_link::LinkConfig::usb(), 0x5_1F1_C601, true),
            pending: VecDeque::new(),
            unparsed: 0,
            resets: 0,
            ups: 0,
            notes: Vec::new(),
        }
    }

    fn now(&self) -> u64 {
        self.elapsed_us
    }

    fn seconds(&self) -> f64 {
        self.now() as f64 / 1e6
    }

    /// Step until the board has rebooted and the link is up again; drop
    /// whatever was pending from before.
    fn wait_for_reboot(&mut self) {
        let deadline = self.seconds() + REBOOT_BUDGET_S;
        let ups_before = self.ups;
        while self.machine.reboots() == 0 || self.ups == ups_before {
            assert!(
                self.seconds() < deadline,
                "the board did not come back within {REBOOT_BUDGET_S} emulated seconds \
                 (reboots {}, resets {}, ups {})",
                self.machine.reboots(),
                self.resets,
                self.ups
            );
            self.step();
        }
        self.pending.clear();
    }

    /// One slice of the board, then the host's end of the link.
    fn step(&mut self) {
        let stop = StopCondition {
            stop_cycle: Some(
                self.machine.cycles() + SLICE_US * lp_emu_esp32c6::memmap::CYCLES_PER_US,
            ),
            ..Default::default()
        };
        match self.machine.run_until(&stop) {
            Outcome::Deadline { .. } => {}
            other => panic!("the emulated board stopped: {other:?}"),
        }
        self.elapsed_us += SLICE_US;
        let now = self.now();
        let bytes = self.machine.take_usb_sj_output();
        if !bytes.is_empty() {
            self.port.on_bytes(now, &bytes);
        }
        while let Some(frame) = self.port.poll_transmit(now) {
            self.queue.push(frame);
        }
        while let Some(read) = self.port.poll_read() {
            match read {
                PortRead::Message(payload) => match payload.message {
                    Ok(message) => self.pending.push_back(message),
                    Err(error) => {
                        eprintln!("emu_wifi_settings: a message did not parse: {error}");
                        self.unparsed += 1;
                    }
                },
                PortRead::Log(_) => {}
                PortRead::Reset { reason } => {
                    eprintln!("emu_wifi_settings: the link reset: {reason:?}");
                    self.resets += 1;
                }
                PortRead::Up { .. } => self.ups += 1,
                PortRead::Note(note) => self.notes.push(note),
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl lpa_client::ClientIo for &mut LinkIo {
    async fn send(&mut self, message: ClientMessage) -> Result<(), TransportError> {
        self.port
            .send_client(&message)
            .map_err(|error| TransportError::Other(format!("the link refused it: {error:?}")))
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        let deadline = self.seconds() + ANSWER_BUDGET_S;
        loop {
            if let Some(message) = self.pending.pop_front() {
                return Ok(message);
            }
            if self.seconds() > deadline {
                return Err(TransportError::Other(format!(
                    "no answer within {ANSWER_BUDGET_S} emulated seconds"
                )));
            }
            self.step();
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

/// Drive a future whose every await completes synchronously (the io steps
/// the board inside `receive`): tests are edges, and a null waker is enough.
fn block_on<F: Future>(future: F) -> F::Output {
    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Noop));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
}
