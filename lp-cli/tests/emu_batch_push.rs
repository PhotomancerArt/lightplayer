//! The push boundary on the real C6 image: one project upload with a batch
//! and deflated chunks, and a link cut in the middle of another (plan
//! `lp2025/2026-10-08-2339-wire-push-boundary-and-deflate`, P7).
//!
//! The emulated ESP32-C6 runs in this process (the shape of
//! `emu_usb_link.rs`: stepped slice by slice in EMULATED time, the host's end
//! the product's own `WireLinkPort` under an `lpa-client` `LpClient`), and
//! the host's side of the conversation is the real push primitive,
//! `LpClient::deploy_project_files` — the call `lp-cli upload` and `dev`
//! make. Two claims, both about the shipped firmware, which mounts the tree
//! store (since the tree-store cutover) and so answers `BeginBatch` with
//! `atomic: true`:
//!
//! 1. **A batched, deflated upload lands byte for byte.** The upload sends
//!    `StopAllProjects`, `BeginBatch`, the writes (the files that shrink go
//!    as `WriteChunkDeflated`), `CommitBatch` — the one ending that lands
//!    them — and `LoadProject`. The board's `HashPackage` of the project
//!    equals the hash the host computes over the same files with
//!    `lpc_history::hash_package`, which can only be true if the board
//!    inflated every deflated chunk to the bytes that were sent.
//! 2. **A cut link leaves the OLD project, byte for byte, and nothing
//!    held.** A second push is started by hand (stop, begin, some writes
//!    including a multi-chunk deflated file), the USB cable is detached
//!    through the emulator's control channel while a chunk is in flight,
//!    and a new client attaches. The board answers its hello; the server
//!    dropped the batch the old session owned (`take_reset_links`), so the
//!    store never committed a byte of version 2: the project hashes as
//!    version 1 and its shader is version 1's. A commit says "no batch is
//!    open", a begin and an abort answer `atomic: true`, a plain write is
//!    taken, and a full upload then completes with the matching hash.
//!
//! Before the cutover this ran on littlefs (`atomic: false`), and claim 2's
//! last assertion was the inverse: what landed before the cut stayed, a
//! project half-way between the two versions. That firmware is the pinned
//! fielded image now; the two-slot push it runs is `lpa-client`'s own tests.
//!
//! It lives in lp-cli because nothing under `lp-emu/` may depend on a
//! product crate (the MIT fence). `#[ignore]`d and run by `just test-emu-c6`'s
//! link half: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`).
//! Numbers it prints are `lp-emu:esp32c6:t1`.

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
use lpa_client::push_files::file_requests;
use lpa_client::{LpClient, ProjectDeployFile};
use lpc_model::AsLpPathBuf;
use lpc_wire::server::{BatchOp, FsResponse};
use lpc_wire::{
    ClientMessage, ClientRequest, FsRequest, PortRead, TransportError, WireLinkPort,
    WireServerMessage, WireServerMsgBody,
};
use lpfs::{LpFs, LpFsMemory, LpPath};

/// Emulated microseconds per slice: the host services its link between
/// slices, so this bounds its reaction time.
const SLICE_US: u64 = 250;

/// The longest a request may wait for its answer, in emulated seconds. A
/// project load compiles every shader on the board; this only bounds a
/// broken run.
const ANSWER_BUDGET_S: f64 = 120.0;

/// How long the cable stays out, in emulated seconds: long enough that the
/// board's link has seen the silence and the host's old session is dead.
const CABLE_OUT_S: f64 = 3.0;

const PROJECT: &str = "batch-push";

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn a_batched_deflated_upload_lands_byte_for_byte_in_one_committed_batch() {
    let Some(elf) = image() else { return };
    let mut board = Board::new(&elf);
    let files = project_files("projects/test/basic");
    let host_hash = host_package_hash(&files);

    {
        let mut client = LpClient::new(&mut board);
        let hello = block_on(client.hello()).expect("the board says hello");
        assert_eq!(hello.value.proto, lpc_wire::WIRE_PROTO_VERSION);
        block_on(client.deploy_project_files(PROJECT, deploy_files(&files)))
            .expect("the batched, deflated upload");
        let loaded = block_on(client.project_list_loaded()).expect("the loaded list");
        assert!(
            loaded
                .value
                .iter()
                .any(|project| project.path.as_str().contains(PROJECT)),
            "the uploaded project is loaded: {:?}",
            loaded.value
        );
        let board_hash = block_on(client.hash_package(PROJECT))
            .expect("the board hashes its copy")
            .value;
        assert_eq!(
            board_hash, host_hash,
            "the board's copy of the project is not the bytes the host sent \
             (an inflated chunk differs)"
        );
    }

    // What went over the wire is the primitive's plan, answered by a board
    // whose filesystem has transactions.
    let kinds: Vec<&str> = board.sent.iter().map(kind).collect();
    let begin = kinds
        .iter()
        .position(|k| *k == "begin")
        .expect("the upload began a batch");
    assert_eq!(
        kinds[..=begin].iter().rev().nth(1),
        Some(&"stop"),
        "the stop comes right before the begin: {kinds:?}"
    );
    let load = kinds
        .iter()
        .position(|k| *k == "load")
        .expect("the upload loaded the project");
    let commit = kinds
        .iter()
        .position(|k| *k == "commit")
        .expect("an atomic board's batch is committed");
    assert!(
        begin < commit && commit < load,
        "the commit comes after the begin and before the load: {kinds:?}"
    );
    assert!(
        kinds[..load]
            .iter()
            .filter(|k| k.starts_with("write"))
            .count()
            > 0
            && kinds[load..].iter().all(|k| !k.starts_with("write")),
        "every write comes before the load: {kinds:?}"
    );
    let deflated = kinds.iter().filter(|k| **k == "write-deflated").count();
    let plain = kinds
        .iter()
        .filter(|k| **k == "write" || **k == "write-chunk")
        .count();
    assert!(
        deflated >= 2,
        "the project has a file that deflates into several chunks: {kinds:?}"
    );
    assert!(
        plain >= 1,
        "the project has files too small to deflate, sent as they are: {kinds:?}"
    );
    assert_eq!(
        board.batch_answers,
        [(BatchOp::Begin, true, None), (BatchOp::Commit, true, None)],
        "the board's batch answers: a transaction begun and committed, no error"
    );
    let sent_bytes: usize = board.sent.iter().map(wire_data_len).sum();
    let logical_bytes: usize = files.iter().map(|(_, bytes)| bytes.len()).sum();
    eprintln!(
        "emu_batch_push: lp-emu:esp32c6:t1, {:.1} s emulated: {deflated} deflated chunks + {plain} \
         plain writes carried {logical_bytes} B of project in {sent_bytes} B of payload; board \
         HashPackage == host hash {host_hash}",
        board.seconds()
    );
    assert!(
        sent_bytes < logical_bytes,
        "deflate saved nothing on the wire: {sent_bytes} B for {logical_bytes} B"
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn a_link_cut_mid_push_leaves_the_old_project_and_no_batch_and_the_next_push_completes() {
    let Some(elf) = image() else { return };
    let mut board = Board::new(&elf);
    let v1 = project_files("projects/test/basic");
    // Version 2 differs from version 1 in two files that a push writes one
    // after the other (path order): at the very start of the shader, so even
    // its first chunk alone changes what the board holds, and in the shader's
    // own settings, which come after it and which the cut never reaches.
    let mut v2 = v1.clone();
    for (name, bytes) in v2.iter_mut() {
        match name.as_str() {
            "shader.glsl" => {
                let mut changed = b"// version 2\n".to_vec();
                changed.extend_from_slice(bytes);
                *bytes = changed;
            }
            "shader.json" => bytes.push(b'\n'),
            _ => {}
        }
    }
    let (hash_v1, hash_v2) = (host_package_hash(&v1), host_package_hash(&v2));
    assert_ne!(hash_v1, hash_v2);

    // The board holds version 1, loaded.
    {
        let mut client = LpClient::new(&mut board);
        block_on(client.hello()).expect("hello");
        block_on(client.deploy_project_files(PROJECT, deploy_files(&v1))).expect("version 1");
        let board_hash = block_on(client.hash_package(PROJECT)).expect("hash").value;
        assert_eq!(board_hash, hash_v1);
    }

    // A push of version 2, by hand, cut off in the middle of a chunk.
    let shader_requests = file_requests(
        PROJECT,
        "shader.glsl",
        &v2.iter().find(|(name, _)| name == "shader.glsl").unwrap().1,
        true,
    );
    assert!(
        shader_requests.len() >= 2
            && matches!(
                &shader_requests[0],
                ClientRequest::Filesystem(FsRequest::WriteChunkDeflated { .. })
            ),
        "version 2's shader is several deflated chunks: {} requests",
        shader_requests.len()
    );
    {
        let mut client = LpClient::new(&mut board);
        let stopped = block_on(client.send_request(ClientRequest::StopAllProjects))
            .expect("stop")
            .value
            .msg;
        assert!(matches!(stopped, WireServerMsgBody::StopAllProjects));
        let begun = block_on(client.send_request(ClientRequest::Filesystem(FsRequest::BeginBatch)))
            .expect("begin")
            .value
            .msg;
        assert_eq!(batch_answer(&begun), (BatchOp::Begin, true, None));
        for request in file_requests(PROJECT, "clock.json", &file(&v1, "clock.json"), true)
            .into_iter()
            .chain(shader_requests[..1].iter().cloned())
        {
            let answered = block_on(client.send_request(request))
                .expect("a write")
                .value
                .msg;
            match answered {
                WireServerMsgBody::Filesystem(FsResponse::Write { error: None, .. })
                | WireServerMsgBody::Filesystem(FsResponse::WriteChunk { error: None, .. }) => {}
                other => panic!("a write before the cut was refused: {other:?}"),
            }
        }
    }
    // The next chunk is cut in the middle of its bytes: half of them reach
    // the board, a slice of the board's time takes them, and the cable is
    // out. (shader.json, the next file of the push, is never sent.)
    let (kept, whole) = board.send_cut_short(shader_requests[1].clone(), 50);
    eprintln!(
        "emu_batch_push: the cable came out {kept} B into the {whole} B the second shader chunk \
         takes on the link"
    );
    board.pump_us(SLICE_US);
    board.unplug();
    board.pump_seconds(CABLE_OUT_S);

    // A new client on a re-plugged cable.
    board.replug();
    {
        let mut client = LpClient::new(&mut board);
        let hello = block_on(client.hello()).expect("the board answers a new client after the cut");
        assert_eq!(hello.value.proto, lpc_wire::WIRE_PROTO_VERSION);

        // The batch the old session owned was dropped with it: nothing of
        // version 2 was committed, so the board holds version 1, whole.
        let after_cut = block_on(client.hash_package(PROJECT)).expect("hash").value;
        assert_eq!(
            after_cut, hash_v1,
            "the store committed part of a batch whose link was cut"
        );
        let shader_now = block_on(
            client.send_request(ClientRequest::Filesystem(FsRequest::Read {
                path: format!("/projects/{PROJECT}/shader.glsl")
                    .as_str()
                    .as_path_buf(),
            })),
        )
        .expect("read the shader back")
        .value
        .msg;
        let shader_len = match shader_now {
            WireServerMsgBody::Filesystem(FsResponse::Read { data, .. }) => data.map(|d| d.len()),
            other => panic!("not a read answer: {other:?}"),
        };
        assert_eq!(
            shader_len,
            Some(file(&v1, "shader.glsl").len()),
            "the shader is version 1's, whole"
        );
        eprintln!(
            "emu_batch_push: lp-emu:esp32c6:t1, after the cut the board's copy hashes \
             {after_cut} (version 1 {hash_v1}, version 2 {hash_v2}); its shader.glsl is \
             {shader_len:?} B (version 1: {} B, version 2: {} B)",
            file(&v1, "shader.glsl").len(),
            file(&v2, "shader.glsl").len()
        );

        // No batch is held: nothing for the dead session to leave behind.
        let committed =
            block_on(client.send_request(ClientRequest::Filesystem(FsRequest::CommitBatch)))
                .expect("commit")
                .value
                .msg;
        assert_eq!(
            batch_answer(&committed),
            (BatchOp::Commit, true, Some("no batch is open".to_string()))
        );
        let begun = block_on(client.send_request(ClientRequest::Filesystem(FsRequest::BeginBatch)))
            .expect("begin")
            .value
            .msg;
        assert_eq!(batch_answer(&begun), (BatchOp::Begin, true, None));
        let aborted =
            block_on(client.send_request(ClientRequest::Filesystem(FsRequest::AbortBatch)))
                .expect("abort")
                .value
                .msg;
        assert_eq!(batch_answer(&aborted), (BatchOp::Abort, true, None));
        let probe = ClientRequest::Filesystem(FsRequest::Write {
            path: "/projects/batch-push-probe/note.txt".as_path_buf(),
            data: b"after the cut".to_vec(),
        });
        let wrote = block_on(client.send_request(probe))
            .expect("a write")
            .value
            .msg;
        assert!(
            matches!(
                wrote,
                WireServerMsgBody::Filesystem(FsResponse::Write { error: None, .. })
            ),
            "a plain write after the cut is taken: {wrote:?}"
        );

        // And the full push completes, to the bytes of version 2.
        block_on(client.deploy_project_files(PROJECT, deploy_files(&v2)))
            .expect("version 2, whole, after the cut");
        let finished = block_on(client.hash_package(PROJECT)).expect("hash").value;
        assert_eq!(
            finished, hash_v2,
            "the finished push is version 2, byte for byte"
        );
        let loaded = block_on(client.project_list_loaded()).expect("the loaded list");
        assert!(
            loaded
                .value
                .iter()
                .any(|project| project.path.as_str().contains(PROJECT)),
            "version 2 is loaded: {:?}",
            loaded.value
        );
    }
    assert_eq!(
        board.link_errors, 0,
        "no message failed to parse, and no link reset reached the host's new session"
    );
}

fn image() -> Option<std::path::PathBuf> {
    match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("emu_batch_push: skipped — {reason}");
            None
        }
    }
}

/// The wire's name for what a request is, for asserting the push's shape.
fn kind(request: &ClientRequest) -> &'static str {
    match request {
        ClientRequest::Hello => "hello",
        ClientRequest::StopAllProjects => "stop",
        ClientRequest::LoadProject { .. } => "load",
        ClientRequest::Filesystem(FsRequest::BeginBatch) => "begin",
        ClientRequest::Filesystem(FsRequest::CommitBatch) => "commit",
        ClientRequest::Filesystem(FsRequest::AbortBatch) => "abort",
        ClientRequest::Filesystem(FsRequest::Write { .. }) => "write",
        ClientRequest::Filesystem(FsRequest::WriteChunk { .. }) => "write-chunk",
        ClientRequest::Filesystem(FsRequest::WriteChunkDeflated { .. }) => "write-deflated",
        ClientRequest::Filesystem(FsRequest::HashPackage { .. }) => "hash",
        _ => "other",
    }
}

/// The payload bytes a write request puts on the wire (the deflated form
/// for `WriteChunkDeflated`).
fn wire_data_len(request: &ClientRequest) -> usize {
    match request {
        ClientRequest::Filesystem(
            FsRequest::Write { data, .. }
            | FsRequest::WriteChunk { data, .. }
            | FsRequest::WriteChunkDeflated { data, .. },
        ) => data.len(),
        _ => 0,
    }
}

fn batch_answer(body: &WireServerMsgBody) -> (BatchOp, bool, Option<String>) {
    match body {
        WireServerMsgBody::Filesystem(FsResponse::Batch { op, atomic, error }) => {
            (*op, *atomic, error.clone())
        }
        other => panic!("not a batch answer: {other:?}"),
    }
}

fn file(files: &[(String, Vec<u8>)], name: &str) -> Vec<u8> {
    files
        .iter()
        .find(|(file, _)| file == name)
        .unwrap_or_else(|| panic!("no {name} in the project"))
        .1
        .clone()
}

fn deploy_files(files: &[(String, Vec<u8>)]) -> Vec<ProjectDeployFile> {
    files
        .iter()
        .map(|(name, bytes)| ProjectDeployFile::new(name.clone(), bytes.clone()))
        .collect()
}

/// The hash the host computes over the project's files, which the board must
/// reproduce from its own copy (`HashPackage`).
fn host_package_hash(files: &[(String, Vec<u8>)]) -> String {
    let fs = LpFsMemory::new();
    for (name, bytes) in files {
        fs.write_file(LpPath::new(&format!("/{name}")), bytes)
            .expect("the host copy");
    }
    lpc_history::hash_package(&fs)
        .expect("hashing the host copy")
        .0
        .to_string()
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

/// The product's host end of the USB link over the in-process board: an
/// `lpa-client` io that steps the machine while it waits, and remembers what
/// it sent and what the board said about batches.
struct Board {
    machine: Esp32C6Machine,
    queue: QueueHandle,
    start: u64,
    port: WireLinkPort,
    sessions: u32,
    /// Whether the cable is in (see [`Self::unplug`]).
    cable: bool,
    pending: VecDeque<WireServerMessage>,
    /// Every request the client sent, in order.
    sent: Vec<ClientRequest>,
    /// Every batch answer the board gave: (op, atomic, error).
    batch_answers: Vec<(BatchOp, bool, Option<String>)>,
    /// A message that did not parse or a link reset: never expected.
    link_errors: u32,
}

impl Board {
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
        let start = machine.micros();
        Self {
            machine,
            queue,
            start,
            port: new_port(0),
            sessions: 0,
            cable: true,
            pending: VecDeque::new(),
            sent: Vec::new(),
            batch_answers: Vec::new(),
            link_errors: 0,
        }
    }

    /// Pull the cable: the emulator's `detach`. Nothing crosses it in either
    /// direction until [`Self::replug`] — the old host's link keeps trying
    /// (its resends go nowhere), the way a port with no cable behind it does.
    fn unplug(&mut self) {
        let reply = self.machine.control_line("detach").to_string();
        assert!(reply.starts_with("ok detach"), "{reply}");
        self.cable = false;
        self.queue.clear();
    }

    /// Plug it back in with a new host on it: `attach` and `open`, and a
    /// fresh link session that knows nothing of the old one's frames or
    /// answers.
    fn replug(&mut self) {
        for verb in ["attach", "open"] {
            let reply = self.machine.control_line(verb).to_string();
            assert!(reply.starts_with(&format!("ok {verb}")), "{reply}");
        }
        self.cable = true;
        self.sessions += 1;
        self.port = new_port(self.sessions);
        self.pending.clear();
        self.queue.clear();
    }

    fn now(&self) -> u64 {
        self.machine.micros() - self.start
    }

    fn seconds(&self) -> f64 {
        self.now() as f64 / 1e6
    }

    /// Send the first `kept_percent` of a request's link bytes and no more:
    /// the cable comes out in the middle of the request, so the board is
    /// left holding the front of a frame that never ends.
    fn send_cut_short(&mut self, request: ClientRequest, kept_percent: usize) -> (usize, usize) {
        self.sent.push(request.clone());
        self.port
            .send_client(&ClientMessage {
                id: 900_000 + self.sent.len() as u64,
                msg: request,
            })
            .expect("the link takes a request");
        let now = self.now();
        let mut bytes = Vec::new();
        while let Some(frame) = self.port.poll_transmit(now) {
            bytes.extend_from_slice(&frame);
        }
        let kept = bytes.len() * kept_percent / 100;
        self.queue.push(&bytes[..kept]);
        (kept, bytes.len())
    }

    fn pump_us(&mut self, micros: u64) {
        let until = self.now() + micros;
        while self.now() < until {
            self.step();
        }
    }

    fn pump_seconds(&mut self, seconds: f64) {
        self.pump_us((seconds * 1e6) as u64);
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
        if !bytes.is_empty() && self.cable {
            self.port.on_bytes(now, &bytes);
        }
        while let Some(frame) = self.port.poll_transmit(now) {
            if self.cable {
                self.queue.push(&frame);
            }
        }
        while let Some(read) = self.port.poll_read() {
            match read {
                PortRead::Message(payload) => match payload.message {
                    Ok(message) => {
                        if let WireServerMsgBody::Filesystem(FsResponse::Batch {
                            op,
                            atomic,
                            error,
                        }) = &message.msg
                        {
                            self.batch_answers.push((*op, *atomic, error.clone()));
                        }
                        self.pending.push_back(message);
                    }
                    Err(error) => {
                        eprintln!("emu_batch_push: a message did not parse: {error}");
                        self.link_errors += 1;
                    }
                },
                PortRead::Log(_) | PortRead::Up { .. } | PortRead::Note(_) => {}
                PortRead::Reset { reason } => {
                    eprintln!("emu_batch_push: the link reset: {reason:?}");
                    self.link_errors += 1;
                }
            }
        }
    }
}

fn new_port(session: u32) -> WireLinkPort {
    WireLinkPort::new(
        lpc_wire::lp_link::LinkConfig::usb(),
        0x4057_C700 + session,
        true,
    )
}

/// Implemented on the borrow, so the test keeps the board after the client
/// is done with it.
#[async_trait::async_trait(?Send)]
impl lpa_client::ClientIo for &mut Board {
    async fn send(&mut self, message: ClientMessage) -> Result<(), TransportError> {
        self.sent.push(message.msg.clone());
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
