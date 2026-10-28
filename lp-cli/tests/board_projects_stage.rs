//! `lp-cli`'s project stage (M3 P11): the update's last leg, as `firmware
//! install` and `emu run --ota-offer` run it
//! (`lp_cli::commands::ota_host::finish_board_projects`), against a real
//! `lpa-server` engine on the tree store over simulated NOR flash (the C6's
//! and the S3's store: a batch is one transaction).
//!
//! The board is the one an update leaves: the new engine (format 12) holding
//! two format-11 projects from `lpa-upgrade`'s corpus (`eaves-wired`, the
//! startup choice, whose upgrade removes a patch file; `fiber-headband`,
//! whose upgrade adds a map file), its startup load refused for its format.
//! The expected bytes are the corpus's own `_expected` outputs.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use lp_cli::commands::ota_host::{BoardHello, finish_board_projects};
use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_nor_sim::{NorFlashSim, NorGeometry};
use lp_tree_store::{LpFsTree, SoftSha256, StoreConfig, TreeStore};
use lpa_client::{ClientIo, LpClient};
use lpa_server::{LpGraphics, LpServer};
use lpc_model::AsLpPath;
use lpc_model::server::server_config::ServerConfig;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{Incoming, Link, LinkId, ServerTransport};
use lpc_wire::{ClientMessage, ClientRequest, ServerHello, TransportError, WireServerMessage};
use lpfs::LpFs;

const STARTUP: &str = "eaves";
const OTHER: &str = "headband";

#[test]
fn an_install_ends_with_the_boards_projects_upgraded_and_its_startup_project_running() {
    let board = Board::updated();
    let mut client = LpClient::new(BoardIo::healthy(&board));
    let hello = block_on(client.hello()).expect("hello").into_value();
    assert_eq!(
        hello.build.project_format,
        lpc_model::PROJECT_FORMAT_VERSION
    );

    let finished = block_on(finish_board_projects(
        &mut client,
        &BoardHello::from_hello(hello),
        false,
    ));

    assert_eq!(
        finished.lines,
        [
            "projects: upgraded “eaves” to format 12",
            "projects: upgraded “headband” to format 12",
        ]
    );
    assert!(finished.ok);
    board.assert_project_is_the_expected_upgrade(STARTUP, "eaves-wired");
    board.assert_project_is_the_expected_upgrade(OTHER, "fiber-headband");
    board.frames(8);
    assert_eq!(board.running(), [format!("/projects/{STARTUP}")]);
}

#[test]
fn a_rerun_finishes_a_board_left_mid_stage() {
    let board = Board::updated();

    // The link goes right after the startup project is loaded: the other
    // project is still at 11.
    let mut client = LpClient::new(BoardIo::cut_after_the_load(&board));
    let hello = block_on(client.hello()).expect("hello").into_value();
    let hello = BoardHello::from_hello(hello);
    let first = block_on(finish_board_projects(&mut client, &hello, false));
    assert!(!first.ok);
    assert_eq!(first.lines[0], "projects: upgraded “eaves” to format 12");
    assert!(
        first.lines[1].starts_with("projects: the link went before the projects were finished"),
        "{:?}",
        first.lines
    );
    assert_eq!(first.lines[2], "projects: “headband” still needs upgrading");
    board.assert_project_is_the_expected_upgrade(STARTUP, "eaves-wired");
    assert!(board.manifest(OTHER).contains("\"format\": 11"));

    // Run again (an install or an update run again on a board already at
    // the build): only what is left is done.
    let mut client = LpClient::new(BoardIo::healthy(&board));
    let again = block_on(finish_board_projects(&mut client, &hello, false));
    assert_eq!(again.lines, ["projects: upgraded “headband” to format 12"]);
    assert!(again.ok);
    board.assert_project_is_the_expected_upgrade(OTHER, "fiber-headband");

    // And once more: nothing is older, nothing is written.
    let mut client = LpClient::new(BoardIo::healthy(&board));
    let third = block_on(finish_board_projects(&mut client, &hello, false));
    assert_eq!(third.lines, ["projects: every project is at format 12"]);
    assert!(third.ok);
}

#[test]
fn a_link_that_cannot_edit_names_the_projects_and_writes_nothing() {
    let board = Board::updated();
    let mut client = LpClient::new(BoardIo::healthy(&board));
    let mut hello: ServerHello = block_on(client.hello()).expect("hello").into_value();
    hello.auth.granted = Some(lpc_access::Tier::Play);

    let finished = block_on(finish_board_projects(
        &mut client,
        &BoardHello::from_hello(hello),
        false,
    ));

    assert_eq!(
        finished.lines,
        [
            "projects: “eaves” needs upgrading; this link can't edit the board",
            "projects: “headband” needs upgrading; this link can't edit the board",
        ]
    );
    assert!(!finished.ok);
    assert!(board.manifest(STARTUP).contains("\"format\": 11"));
    assert!(board.manifest(OTHER).contains("\"format\": 11"));
}

#[test]
fn a_board_on_another_wire_is_not_asked_anything() {
    let board = Board::updated();
    let mut io = BoardIo::healthy(&board);
    io.refuse_all = true;
    let mut client = LpClient::new(io);
    let other = lpc_wire::WIRE_PROTO_VERSION - 1;

    let finished = block_on(finish_board_projects(
        &mut client,
        &BoardHello::OtherWire { proto: other },
        false,
    ));

    assert!(!finished.ok);
    assert_eq!(
        finished.lines,
        [format!(
            "projects: this board's firmware speaks wire {other} and this lp-cli speaks wire \
             {}, so it can't check the board's projects. If they need upgrading, finish them \
             from a Studio or lp-cli that speaks wire {other}.",
            lpc_wire::WIRE_PROTO_VERSION
        )]
    );
}

// --- the board ---------------------------------------------------------------------

/// An engine on the tree store, as an update leaves it.
struct Board {
    server: RefCell<LpServer>,
    transport: RefCell<VecTransport>,
}

impl Board {
    /// The new engine booted over two format-11 projects; its startup load
    /// refused for its format, as the boot's own load is.
    fn updated() -> Self {
        let mut flash = NorFlashSim::new(NorGeometry::c6(64));
        TreeStore::format(&mut flash, SoftSha256, StoreConfig::default())
            .map_err(|(e, ..)| e)
            .expect("format");
        let tree = LpFsTree::new(mount(flash));
        for (name, corpus) in [(STARTUP, "eaves-wired"), (OTHER, "fiber-headband")] {
            for (path, bytes) in corpus_files(&format!("v11/{corpus}")) {
                tree.write_file(
                    format!("/projects/{name}/{path}").as_str().as_path(),
                    &bytes,
                )
                .expect("write the old project");
            }
        }
        tree.write_file(
            ServerConfig::PATH.as_path(),
            format!("{{\"startup_project\":\"{STARTUP}\"}}").as_bytes(),
        )
        .expect("write the startup choice");
        let server = LpServer::new(
            Rc::new(RefCell::new(MemoryOutputProvider::new())),
            Box::new(tree),
            "projects".as_path(),
            None,
            None,
            Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND))
                as Arc<dyn LpGraphics>,
        );
        let board = Self {
            server: RefCell::new(server),
            transport: RefCell::new(VecTransport::default()),
        };
        let startup = format!("/projects/{STARTUP}");
        assert!(
            board
                .server
                .borrow_mut()
                .load_startup_project(startup.as_str().as_path())
                .is_err(),
            "a format-11 startup project is refused by a format-12 engine"
        );
        board
    }

    /// One tick with `message` in; every frame the server sent out.
    fn tick(&self, message: ClientMessage) -> Vec<WireServerMessage> {
        let mut transport = self.transport.borrow_mut();
        transport.sent.clear();
        block_on(self.server.borrow_mut().tick_and_send(
            16,
            vec![Incoming::primary(message)],
            &mut *transport,
        ))
        .expect("tick");
        transport.sent.drain(..).collect()
    }

    fn frames(&self, n: u32) {
        for _ in 0..n {
            let mut transport = self.transport.borrow_mut();
            block_on(
                self.server
                    .borrow_mut()
                    .tick_and_send(16, Vec::new(), &mut *transport),
            )
            .expect("tick");
            transport.sent.clear();
        }
    }

    fn running(&self) -> Vec<String> {
        self.server
            .borrow()
            .project_manager()
            .list_loaded_projects()
            .iter()
            .map(|p| String::from(p.path.as_str()))
            .collect()
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.server
            .borrow()
            .base_fs()
            .read_file(path.as_path())
            .ok()
    }

    fn manifest(&self, name: &str) -> String {
        String::from_utf8(
            self.read(&format!("/projects/{name}/project.json"))
                .expect("a manifest"),
        )
        .expect("utf-8")
    }

    /// Every file of the corpus's expected upgrade is on the board, byte for
    /// byte, and every file of the old project the upgrade dropped is gone.
    fn assert_project_is_the_expected_upgrade(&self, name: &str, corpus: &str) {
        let expected = corpus_files(&format!("v11/_expected/{corpus}"));
        for (path, bytes) in &expected {
            assert_eq!(
                self.read(&format!("/projects/{name}/{path}")).as_ref(),
                Some(bytes),
                "{name}/{path}"
            );
        }
        for (path, _) in corpus_files(&format!("v11/{corpus}")) {
            if !expected.iter().any(|(kept, _)| *kept == path) {
                assert_eq!(
                    self.read(&format!("/projects/{name}/{path}")),
                    None,
                    "{name}/{path} was removed by the upgrade"
                );
            }
        }
    }
}

/// The board's channel 1 as a client's IO: each request is one tick of the
/// engine, its answer queued for `receive`. `cut_after_load`: the link goes
/// once a `LoadProject` has been answered.
struct BoardIo<'a> {
    board: &'a Board,
    answers: VecDeque<WireServerMessage>,
    cut_after_load: bool,
    cut: bool,
    refuse_all: bool,
}

impl<'a> BoardIo<'a> {
    fn healthy(board: &'a Board) -> Self {
        Self {
            board,
            answers: VecDeque::new(),
            cut_after_load: false,
            cut: false,
            refuse_all: false,
        }
    }

    fn cut_after_the_load(board: &'a Board) -> Self {
        Self {
            cut_after_load: true,
            ..Self::healthy(board)
        }
    }
}

#[async_trait::async_trait(?Send)]
impl ClientIo for BoardIo<'_> {
    async fn send(&mut self, message: ClientMessage) -> Result<(), TransportError> {
        assert!(!self.refuse_all, "asked {:?}", message.msg);
        if self.cut {
            return Err(TransportError::ConnectionLost);
        }
        let is_load = matches!(message.msg, ClientRequest::LoadProject { .. });
        let id = message.id;
        for frame in self.board.tick(message) {
            if frame.id == id {
                self.answers.push_back(frame);
            }
        }
        if is_load && self.cut_after_load {
            self.cut = true;
        }
        Ok(())
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        self.answers
            .pop_front()
            .ok_or(TransportError::ConnectionLost)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

#[derive(Default)]
struct VecTransport {
    sent: Vec<WireServerMessage>,
}

impl ServerTransport for VecTransport {
    async fn send(&mut self, _link: LinkId, msg: WireServerMessage) -> Result<(), TransportError> {
        self.sent.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        Ok(Vec::new())
    }

    fn links(&self) -> Vec<Link> {
        vec![Link::PRIMARY]
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

// --- the projects ----------------------------------------------------------------

fn corpus_files(name: &str) -> Vec<(String, Vec<u8>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../lp-app/lpa-upgrade/tests/corpus")
        .join(name);
    let mut out = Vec::new();
    collect(&root, &root, &mut out);
    assert!(!out.is_empty(), "{} is missing", root.display());
    out
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, out);
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, std::fs::read(&path).unwrap()));
        }
    }
}

fn mount(flash: NorFlashSim) -> TreeStore<NorFlashSim, SoftSha256> {
    match TreeStore::mount(flash, SoftSha256, StoreConfig::default()) {
        Ok(store) => store,
        Err((error, _, _)) => panic!("mount: {error:?}"),
    }
}

/// Every future here is ready at once: the engine answers in-line.
fn block_on<F: Future>(future: F) -> F::Output {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        if let Poll::Ready(output) = Future::poll(Pin::as_mut(&mut future), &mut cx) {
            return output;
        }
    }
}

fn noop_waker() -> Waker {
    unsafe fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(core::ptr::null(), &VTABLE)
    }
    unsafe fn wake(_: *const ()) {}
    unsafe fn wake_by_ref(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
    // SAFETY: every vtable entry ignores its data pointer.
    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
}
