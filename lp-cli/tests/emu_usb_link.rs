//! The shipped C6 image on its USB link, as lp-cli speaks to it: lp-link
//! both ends (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`).
//!
//! The emulated ESP32-C6 runs in this process (the comms lab's `EmuPipe`
//! shape), stepped slice by slice in EMULATED time, and the host's end is
//! the product's own: one `lpc_wire::WireLinkPort` under an `lpa-client`
//! `LpClient`, exactly what `lp-cli upload serial:…` runs over a port. Two
//! claims:
//!
//! - the board says hello first on the link session, answers a hello
//!   request, and takes a whole project upload (stop, clear, chunked writes,
//!   load) with the loaded project then listed;
//! - with the emulator's USB fault injector damaging ~1 % of packets each
//!   way (`--usb-faults`), the same conversation sees **zero app errors**:
//!   every loss is resent under the messages and shows only in the link's
//!   counters.
//!
//! It lives in lp-cli because nothing under `lp-emu/` may depend on lp-link
//! or a product crate (the MIT fence, D11). `#[ignore]`d and run by
//! `just test-emu-c6`: it needs a built `fw-esp32c6` ELF
//! (`LP_EMU_BUILD_FW=1`). Numbers it prints are `lp-emu:esp32c6:t1`.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_emu_esp_common::QueueHandle;
use lp_emu_esp_common::link_faults::{FaultCounters, LinkFaults};
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade, UsbHost,
};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpc_wire::{
    ClientMessage, LinkCounters, PortRead, TransportError, WireLinkPort, WireServerMessage,
};

/// Emulated microseconds per slice: the host services its link between
/// slices, so this bounds its reaction time (the lab's own figure).
const SLICE_US: u64 = 250;

/// Upload-and-list rounds under faults: enough traffic (~1,000 packets each
/// way) that a 1 % rate injects a handful of faults per run, not one or none.
const ROUNDS: usize = 5;

/// The longest a request may wait for its answer, in emulated seconds. A
/// project load compiles every shader on the board; the budget is generous
/// and only bounds a broken run.
const ANSWER_BUDGET_S: f64 = 60.0;

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn the_shipped_image_says_hello_and_takes_an_upload_over_lp_link() {
    let Some(elf) = image() else { return };
    let run = converse(&elf, None, 1);
    eprintln!("\n=== clean ===\n{}", run.summary());
    assert_eq!(run.app_errors, 0, "{}", run.summary());
    assert_eq!(run.host.resets.total, 0, "one session, start to end");
    assert_eq!(run.host.payload_errors, 0);
    assert!(
        run.notes.iter().any(|note| note.contains("replies packed")),
        "the shipped image packs its replies once asked: {}",
        run.summary()
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn an_upload_under_one_percent_usb_faults_sees_no_app_errors() {
    let Some(elf) = image() else { return };
    let faults = LinkFaults::parse(
        "in-drop=0.25%,in-tail=0.25%,in-corrupt=0.5%,out-drop=0.25%,out-tail=0.25%,\
         out-corrupt=0.5%,seed=31",
    )
    .expect("a fault spec");
    let run = converse(&elf, Some(faults), ROUNDS);
    eprintln!("\n=== 1% mixed each way ===\n{}", run.summary());
    assert_eq!(run.app_errors, 0, "{}", run.summary());
    assert_eq!(run.host.payload_errors, 0);
    assert!(
        run.host.resends > 0 || run.host.damaged > 0,
        "the injector damaged nothing the link had to recover from: {}",
        run.summary()
    );
    run.assert_injected_both_ways();
}

/// The macOS shape (M1 of the reliable-link plan): loss that starts inside a
/// packet and runs through the next several — a kilobyte gone at once, the
/// way a tty overflowing under Chromium's `PARMRK` lost it — on top of the
/// 1 % mix, device to host (a run starts in 1 % of packets and eats the next
/// sixteen). The upload and the reads still see no app error:
/// every run is resent under the messages.
#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn an_upload_through_kilobyte_runs_of_loss_sees_no_app_errors() {
    let Some(elf) = image() else { return };
    let faults = LinkFaults::parse(
        "in-drop=0.25%,in-tail=0.25%,in-corrupt=0.25%,in-run=1%,run-packets=16,\
         out-drop=0.25%,out-tail=0.25%,out-corrupt=0.5%,seed=47",
    )
    .expect("a fault spec");
    let run = converse(&elf, Some(faults), ROUNDS);
    eprintln!("\n=== 1% mixed + 16-packet runs ===\n{}", run.summary());
    assert_eq!(run.app_errors, 0, "{}", run.summary());
    assert_eq!(run.host.payload_errors, 0);
    run.assert_injected_both_ways();
    let (to_host, _) = run.injected.expect("faults were configured");
    assert!(
        to_host.runs_started > 0,
        "no run of loss was injected: {}",
        run.summary()
    );
}

/// What one conversation found.
struct Run {
    /// Anything the conversation saw go wrong: a request that failed, a
    /// message that did not parse, a link reset.
    app_errors: u32,
    host: LinkCounters,
    seconds: f64,
    notes: Vec<String>,
    console_lines: usize,
    /// What the emulator's injector did: (device → host, host → device).
    injected: Option<(FaultCounters, FaultCounters)>,
}

impl Run {
    /// The injector damaged traffic in both directions, so the run's zero
    /// app errors is a claim about recovery, not about luck.
    fn assert_injected_both_ways(&self) {
        let (to_host, to_board) = self.injected.expect("faults were configured");
        let damaging =
            |c: &FaultCounters| c.packets_dropped + c.tails_cut + c.bits_flipped + c.runs_started;
        assert!(
            damaging(&to_host) > 0 && damaging(&to_board) > 0,
            "the injector must damage both directions: {}",
            self.summary()
        );
    }

    fn summary(&self) -> String {
        let h = &self.host;
        format!(
            "lp-emu:esp32c6:t1, {:.1} s emulated: {} app errors; host link {} frames out / {} in, \
             {} resent, {} damaged, {} stale partials, {} duplicates, {} resets; \
             {} console lines; notes {:?}",
            self.seconds,
            self.app_errors,
            h.frames_tx,
            h.frames_rx,
            h.resends,
            h.damaged,
            h.stale_partials,
            h.duplicates,
            h.resets.total,
            self.console_lines,
            self.notes
        ) + &match &self.injected {
            Some((to_host, to_board)) => {
                format!("; injected → host: {to_host}; injected → board: {to_board}")
            }
            None => String::new(),
        }
    }
}

fn image() -> Option<std::path::PathBuf> {
    match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("emu_usb_link: skipped — {reason}");
            None
        }
    }
}

/// Hello, then the whole `projects/test/basic` upload, then the list of
/// loaded projects — every step a request over the link.
fn converse(elf: &std::path::Path, faults: Option<LinkFaults>, rounds: usize) -> Run {
    let mut io = LinkIo::new(elf, faults);
    let mut app_errors = 0u32;
    let mut failed = |what: &str, error: &dyn std::fmt::Display| {
        eprintln!("emu_usb_link: {what} failed: {error}");
        app_errors += 1;
    };
    {
        let mut client = lpa_client::LpClient::new(&mut io);
        match block_on(client.hello()) {
            Ok(hello) => assert_eq!(hello.value.proto, lpc_wire::WIRE_PROTO_VERSION),
            Err(error) => failed("hello", &error),
        }
        let files = project_files("projects/test/basic");
        for _ in 0..rounds {
            match block_on(client.replace_and_load_project("emu-link-basic", &files)) {
                Ok(_) => {}
                Err(error) => failed("the upload", &error),
            }
            match block_on(client.project_list_loaded()) {
                Ok(loaded) => assert!(
                    loaded
                        .value
                        .iter()
                        .any(|project| project.path.as_str().contains("emu-link-basic")),
                    "the uploaded project is loaded: {:?}",
                    loaded.value
                ),
                Err(error) => failed("listing loaded projects", &error),
            }
        }
    }
    Run {
        app_errors: app_errors + io.link_errors,
        host: io.port.counters(),
        seconds: io.seconds(),
        notes: io.notes,
        console_lines: io.console_lines,
        injected: io.machine.usb_fault_counters(),
    }
}

/// A project directory as the upload's `(relative path, bytes)` list, in
/// path order (as `lp-cli upload` sends it).
fn project_files(relative: &str) -> Vec<(String, Vec<u8>)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative);
    let mut files = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("the project directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let name = path
                    .strip_prefix(&root)
                    .expect("under the root")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((name, std::fs::read(&path).expect("a file")));
            }
        }
    }
    files.sort();
    files
}

/// The product's host end of the link over the in-process board: an
/// `lpa-client` io that steps the machine while it waits.
struct LinkIo {
    machine: Esp32C6Machine,
    queue: QueueHandle,
    start: u64,
    port: WireLinkPort,
    pending: VecDeque<WireServerMessage>,
    /// A link reset or a message that did not parse: never expected.
    link_errors: u32,
    notes: Vec<String>,
    console_lines: usize,
}

impl LinkIo {
    fn new(elf: &std::path::Path, faults: Option<LinkFaults>) -> Self {
        let mut builder = Esp32C6Builder::new()
            .app(AppSource::Path(elf.to_path_buf()))
            .time_grade(TimeGrade::T1)
            .reboot_on_reset(true)
            .usb_host(UsbHost::Attached { draining: true })
            .usb_sj_queue_source();
        if let Some(faults) = faults {
            builder = builder.usb_faults(faults);
        }
        let machine = builder.build().expect("building the emulated C6");
        let queue = machine
            .usb_sj_host_handle()
            .expect("an in-process USB host queue");
        let start = machine.micros();
        Self {
            machine,
            queue,
            start,
            port: WireLinkPort::new(lpc_wire::lp_link::LinkConfig::usb(), 0x4057_C601, true),
            pending: VecDeque::new(),
            link_errors: 0,
            notes: Vec::new(),
            console_lines: 0,
        }
    }

    fn now(&self) -> u64 {
        self.machine.micros() - self.start
    }

    fn seconds(&self) -> f64 {
        self.now() as f64 / 1e6
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
                        eprintln!("emu_usb_link: a message did not parse: {error}");
                        self.link_errors += 1;
                    }
                },
                PortRead::Log(_) => self.console_lines += 1,
                PortRead::Reset { reason } => {
                    eprintln!("emu_usb_link: the link reset: {reason:?}");
                    self.link_errors += 1;
                }
                PortRead::Up { .. } => {}
                PortRead::Note(note) => self.notes.push(note),
            }
        }
    }
}

/// Implemented on the borrow, so the test keeps the io (and its counters)
/// after the client is done with it.
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
