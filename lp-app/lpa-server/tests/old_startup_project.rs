//! A newer engine booted with an older startup project (M3 P10, the design
//! the update's project stage depends on, lean Q99): between engine Y's
//! first start and a host rewriting the board's projects, the engine holds
//! a startup project it cannot read. It must refuse it by name, not reset,
//! keep the startup choice, and keep answering channel 1 — the listing, file
//! reads, the package hash, batches and loads — so any host with edit can
//! upgrade the project in place and load it.
//!
//! The old project is a real format-11 project from `lpa-upgrade`'s corpus
//! (`eaves-wired`: a patch file, so the upgrade deletes a file), and the
//! upgraded one is that corpus's own expected format-12 output. Proved over
//! a memory filesystem and over the tree store on simulated NOR flash (the
//! C6's and the S3's store, the boards that update over the air). The
//! recovery record's half is in `old_startup_project_recovery.rs`.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::path::{Path, PathBuf};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_nor_sim::{NorFlashSim, NorGeometry};
use lp_tree_store::{LpFsTree, SoftSha256, StoreConfig, TreeStore};
use lpa_server::{LpGraphics, LpServer};
use lpc_model::server::server_config::ServerConfig;
use lpc_model::{AsLpPath, AsLpPathBuf};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{Incoming, Link, LinkId};
use lpc_wire::server::{BatchOp, FsRequest, FsResponse};
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireServerMessage, WireServerMsgBody,
};
use lpfs::{LpFs, LpFsMemory};

const STARTUP: &str = "eaves";

#[test]
fn a_newer_engine_refuses_an_old_startup_project_and_keeps_serving_over_memory() {
    let fs = LpFsMemory::new();
    old_board(&fs);
    let board = Board::over(Box::new(fs));
    the_old_startup_project_is_refused_and_the_board_keeps_serving(&board, false);
}

#[test]
fn a_newer_engine_refuses_an_old_startup_project_and_keeps_serving_over_the_tree_store() {
    let mut flash = NorFlashSim::new(NorGeometry::c6(48));
    TreeStore::format(&mut flash, SoftSha256, StoreConfig::default())
        .map_err(|(e, ..)| e)
        .expect("format");
    let tree = LpFsTree::new(mount(flash));
    old_board(&tree);
    let handle = tree.handle();
    let board = Board::over(Box::new(tree));
    the_old_startup_project_is_refused_and_the_board_keeps_serving(&board, true);

    // And on the flash itself, mounted afresh: the new project, whole.
    let remounted = LpFsTree::new(mount(handle.with_store(|store| store.flash().clone())));
    assert_eq!(
        remounted
            .read_file("/projects/eaves/project.json".as_path())
            .unwrap(),
        expected_files()
            .into_iter()
            .find(|(path, _)| path == "project.json")
            .unwrap()
            .1
    );
    assert!(
        !remounted
            .file_exists("/projects/eaves/fixture.patch.json".as_path())
            .unwrap()
    );
}

fn the_old_startup_project_is_refused_and_the_board_keeps_serving(board: &Board, atomic: bool) {
    let startup_config = board.read_root(ServerConfig::PATH);
    let project = format!("/projects/{STARTUP}");

    // The boot's startup load (what `fw-esp32-common`'s `auto_load_project`
    // calls): refused by name, nothing loaded, no reset.
    let error = board
        .server
        .borrow_mut()
        .load_startup_project(project.as_str().as_path())
        .expect_err("a format-11 project does not load on a format-12 engine");
    let error = error.to_string();
    assert!(
        error.contains("unsupported project format 11"),
        "refused by name: {error}"
    );
    board.frames(8);
    assert!(!board.rebooted.get(), "the board did not reset");
    assert_eq!(board.running(), Vec::<String>::new());
    assert_eq!(
        board.read_root(ServerConfig::PATH),
        startup_config,
        "the startup choice is kept"
    );

    // Channel 1 still answers: the listing, reads, the hash.
    let WireServerMsgBody::ListAvailableProjects { projects } =
        board.request(ClientRequest::ListAvailableProjects)
    else {
        panic!("listing");
    };
    assert!(
        projects
            .iter()
            .any(|p| p.path.as_str().trim_end_matches('/').ends_with(STARTUP)),
        "{projects:?}"
    );
    let manifest = format!("{project}/project.json");
    let FsResponse::Read { data, error, .. } = board.fs(FsRequest::Read {
        path: manifest.as_str().as_path_buf(),
    }) else {
        panic!("read");
    };
    assert_eq!(error, None);
    assert!(
        String::from_utf8(data.expect("bytes"))
            .unwrap()
            .contains("\"format\": 11")
    );
    let FsResponse::ListDir { entries, error, .. } = board.fs(FsRequest::ListDir {
        path: project.as_str().as_path_buf(),
        recursive: true,
    }) else {
        panic!("list");
    };
    assert_eq!(error, None);
    assert!(
        entries
            .iter()
            .any(|e| e.as_str().ends_with("fixture.patch.json")),
        "{entries:?}"
    );
    let FsResponse::PackageHash { hash, error, .. } = board.fs(FsRequest::HashPackage {
        prefix: project.as_str().as_path_buf(),
    }) else {
        panic!("hash");
    };
    assert_eq!((hash.len(), error), (64, None));

    // A batch: the upgraded files written, the patch file deleted, one
    // commit. The tree store answers `atomic`; memory says it is not.
    let FsResponse::Batch {
        op: BatchOp::Begin,
        atomic: begun_atomic,
        error: None,
    } = board.fs(FsRequest::BeginBatch)
    else {
        panic!("begin");
    };
    assert_eq!(begun_atomic, atomic);
    for (path, bytes) in expected_files() {
        let FsResponse::Write { error, .. } = board.fs(FsRequest::Write {
            path: format!("{project}/{path}").as_str().as_path_buf(),
            data: bytes,
        }) else {
            panic!("write {path}");
        };
        assert_eq!(error, None, "{path}");
    }
    let FsResponse::DeleteFile { error, .. } = board.fs(FsRequest::DeleteFile {
        path: format!("{project}/fixture.patch.json")
            .as_str()
            .as_path_buf(),
    }) else {
        panic!("delete");
    };
    assert_eq!(error, None);
    if atomic {
        let FsResponse::Batch {
            op: BatchOp::Commit,
            error: None,
            ..
        } = board.fs(FsRequest::CommitBatch)
        else {
            panic!("commit");
        };
    }

    // The load: the upgraded project runs, and stays the startup choice.
    let answer = board.request(ClientRequest::LoadProject {
        path: project.clone(),
    });
    assert!(
        matches!(answer, WireServerMsgBody::LoadProject { .. }),
        "{answer:?}"
    );
    board.frames(8);
    assert_eq!(board.running(), vec![project.clone()]);
    assert!(!board.rebooted.get());
    let config: ServerConfig =
        lpc_wire::json::from_slice(&board.read_root(ServerConfig::PATH).expect("config"))
            .expect("config parses");
    assert_eq!(config.startup_project.as_deref(), Some(STARTUP));
}

// --- the board -------------------------------------------------------------------

struct Board {
    server: RefCell<LpServer>,
    transport: RefCell<VecTransport>,
    rebooted: Rc<Cell<bool>>,
    next_id: Cell<u64>,
}

impl Board {
    fn over(fs: Box<dyn LpFs>) -> Self {
        let mut server = LpServer::new(
            Rc::new(RefCell::new(MemoryOutputProvider::new())),
            fs,
            "projects".as_path(),
            None,
            None,
            Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND))
                as Arc<dyn LpGraphics>,
        );
        let rebooted = Rc::new(Cell::new(false));
        let flag = Rc::clone(&rebooted);
        server.set_reboot_hook(Some(Rc::new(move || flag.set(true))));
        Self {
            server: RefCell::new(server),
            transport: RefCell::new(VecTransport::default()),
            rebooted,
            next_id: Cell::new(100),
        }
    }

    fn request(&self, msg: ClientRequest) -> WireServerMsgBody {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let mut transport = self.transport.borrow_mut();
        transport.sent.clear();
        block_on(self.server.borrow_mut().tick_and_send(
            16,
            vec![Incoming::primary(ClientMessage { id, msg })],
            &mut *transport,
        ))
        .expect("tick");
        transport
            .sent
            .drain(..)
            .find(|frame| frame.id == id)
            .unwrap_or_else(|| panic!("request {id} unanswered"))
            .msg
    }

    fn fs(&self, request: FsRequest) -> FsResponse {
        match self.request(ClientRequest::Filesystem(request)) {
            WireServerMsgBody::Filesystem(response) => response,
            other => panic!("expected an fs answer, got {other:?}"),
        }
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

    fn read_root(&self, path: &str) -> Option<Vec<u8>> {
        self.server
            .borrow()
            .base_fs()
            .read_file(path.as_path())
            .ok()
    }
}

#[derive(Default)]
struct VecTransport {
    sent: Vec<WireServerMessage>,
}

impl lpc_shared::transport::ServerTransport for VecTransport {
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

/// A board as engine Y finds it after an update: a format-11 project, the
/// startup choice naming it.
fn old_board(fs: &dyn LpFs) {
    for (path, bytes) in corpus_files("v11/eaves-wired") {
        fs.write_file(
            format!("/projects/{STARTUP}/{path}").as_str().as_path(),
            &bytes,
        )
        .expect("write the old project");
    }
    fs.write_file(
        ServerConfig::PATH.as_path(),
        format!("{{\"startup_project\":\"{STARTUP}\"}}").as_bytes(),
    )
    .expect("write the startup choice");
}

/// The same project at format 12: the corpus's expected upgrade.
fn expected_files() -> Vec<(String, Vec<u8>)> {
    corpus_files("v11/_expected/eaves-wired")
}

fn corpus_files(name: &str) -> Vec<(String, Vec<u8>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../lpa-upgrade/tests/corpus")
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
