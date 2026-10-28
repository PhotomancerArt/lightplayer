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

/// DD30: the hello names a startup project the boot refused for its format,
/// with the format it found, until anything is loaded, unloaded or stopped
/// (whatever came of it); a startup load that fails for any other reason is
/// not reported, and a board that refused nothing says nothing.
#[test]
fn the_hello_names_a_startup_project_refused_for_its_format_until_anything_runs() {
    let project = format!("/projects/{STARTUP}");
    let refused = || {
        let fs = LpFsMemory::new();
        old_board(&fs);
        let board = Board::over(Box::new(fs));
        assert_eq!(hello(&board).startup_refused, None, "nothing refused yet");
        board
            .server
            .borrow_mut()
            .load_startup_project(project.as_str().as_path())
            .expect_err("refused");
        assert_eq!(
            hello(&board).startup_refused,
            Some(lpc_wire::StartupRefusal {
                project: project.clone(),
                found: Some(11),
            })
        );
        board
    };

    // Frames go by and channel 1 is read: the report stays.
    let board = refused();
    board.frames(8);
    board.request(ClientRequest::ListAvailableProjects);
    assert!(hello(&board).startup_refused.is_some());

    // A stop ends it.
    board.request(ClientRequest::StopAllProjects);
    assert_eq!(hello(&board).startup_refused, None, "a stop ends it");

    // So does a load, even one that fails.
    let board = refused();
    board.request(ClientRequest::LoadProject {
        path: project.clone(),
    });
    assert_eq!(hello(&board).startup_refused, None, "a load ends it");

    // A startup project at this engine's format that fails for another
    // reason (its files are missing) is not a refusal for its format.
    let fs = LpFsMemory::new();
    let manifest = expected_files()
        .into_iter()
        .find(|(path, _)| path == "project.json")
        .expect("a manifest")
        .1;
    fs.write_file(
        format!("{project}/project.json").as_str().as_path(),
        &manifest,
    )
    .expect("write");
    let board = Board::over(Box::new(fs));
    board
        .server
        .borrow_mut()
        .load_startup_project(project.as_str().as_path())
        .expect_err("a project with no files does not load");
    assert_eq!(hello(&board).startup_refused, None);
}

/// End to end, the design's whole loop on this server: engine Y boots with
/// two format-11 projects and refuses its startup one; the update's project
/// stage (`lpa-update`, driven here over the real wire messages) reads the
/// engine's format off the hello, upgrades both projects in one batch each
/// with `lpa-upgrade`, startup first, and loads it — and the engine runs it.
#[test]
fn the_update_stage_upgrades_a_real_servers_projects_and_the_engine_loads_the_startup_one() {
    let mut flash = NorFlashSim::new(NorGeometry::c6(64));
    TreeStore::format(&mut flash, SoftSha256, StoreConfig::default())
        .map_err(|(e, ..)| e)
        .expect("format");
    let tree = LpFsTree::new(mount(flash));
    old_board(&tree);
    for (path, bytes) in corpus_files("v11/fiber-headband") {
        tree.write_file(
            format!("/projects/headband/{path}").as_str().as_path(),
            &bytes,
        )
        .unwrap();
    }
    let board = Board::over(Box::new(tree));
    let startup = format!("/projects/{STARTUP}");
    assert!(
        board
            .server
            .borrow_mut()
            .load_startup_project(startup.as_str().as_path())
            .is_err()
    );

    let WireServerMsgBody::Hello(hello) = board.request(ClientRequest::Hello) else {
        panic!("hello");
    };
    assert_eq!(
        hello.build.project_format,
        lpc_model::PROJECT_FORMAT_VERSION
    );
    let mut stage =
        lpa_update::ProjectStage::new(hello.build.project_format, Some(lpc_access::Tier::Edit));
    let (report, trace) = run_stage(&board, &mut stage);

    assert_eq!(report.end, lpa_update::StageEnd::Finished, "{report:?}");
    assert_eq!(
        trace,
        [
            "write projects/eaves",
            "load projects/eaves",
            "write projects/headband"
        ]
    );
    assert_eq!(report.load, Some(Ok(())));
    assert_eq!(report.upgraded.len(), 2);
    board.frames(8);
    assert_eq!(board.running(), vec![startup.clone()]);
    let WireServerMsgBody::ListLoadedProjects { projects } =
        board.request(ClientRequest::ListLoadedProjects)
    else {
        panic!("loaded");
    };
    assert!(
        projects.len() == 1 && projects[0].fault.is_none(),
        "the upgraded startup project runs with no fault: {projects:?}"
    );
    assert!(!board.rebooted.get());
    for name in [STARTUP, "headband"] {
        let manifest = board
            .read_root(&format!("/projects/{name}/project.json"))
            .unwrap();
        assert!(
            String::from_utf8(manifest)
                .unwrap()
                .contains("\"format\": 12"),
            "{name}"
        );
    }
    assert!(
        board
            .read_root("/projects/headband/fixture.map2d.json")
            .is_some(),
        "the placed lamps' map2d was added"
    );
    assert!(
        board
            .read_root(&format!("{startup}/fixture.patch.json"))
            .is_none(),
        "the patch file was removed"
    );

    // Q110: the board's project, upgraded in place, hashes the same as a
    // library copy of the same project upgraded on open (the step is
    // deterministic), so Studio's runtime-hash check still matches.
    for (name, corpus) in [
        (STARTUP, "v11/eaves-wired"),
        ("headband", "v11/fiber-headband"),
    ] {
        let mut library: lpa_upgrade::ProjectFiles = corpus_files(corpus).into_iter().collect();
        lpa_upgrade::upgrade_to_current(&mut library).expect("the library copy upgrades");
        let mirror = LpFsMemory::new();
        for (path, bytes) in library.iter() {
            mirror
                .write_file(format!("/{path}").as_str().as_path(), bytes)
                .unwrap();
        }
        let library_hash = lpc_history::hash_package(&mirror).unwrap().0.to_string();
        let FsResponse::PackageHash { hash, error, .. } = board.fs(FsRequest::HashPackage {
            prefix: format!("/projects/{name}").as_str().as_path_buf(),
        }) else {
            panic!("hash");
        };
        assert_eq!((hash, error), (library_hash, None), "{name}");
    }

    // A second stage finds nothing to do.
    let mut again = lpa_update::ProjectStage::new(12, Some(lpc_access::Tier::Edit));
    let (report, trace) = run_stage(&board, &mut again);
    assert_eq!(report.end, lpa_update::StageEnd::Finished);
    assert!(
        trace.is_empty() && report.untouched.len() == 2,
        "{report:?}"
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

// --- the stage's edge over the real wire messages -----------------------------------

/// Drive `stage` over `board` with the channel-1 requests each effect
/// names. Returns the report and the writes and loads, in order.
fn run_stage(
    board: &Board,
    stage: &mut lpa_update::ProjectStage,
) -> (lpa_update::ProjectStageReport, Vec<String>) {
    use lpa_update::{BatchOutcome, ProjectStageEffect};

    let absolute = |project: &str| format!("/{}", project.trim_matches('/'));
    let read = |path: &str| match board.fs(FsRequest::Read {
        path: path.as_path_buf(),
    }) {
        FsResponse::Read {
            data, error: None, ..
        } => data,
        _ => None,
    };
    let mut trace = Vec::new();
    let mut effects = stage.start();
    loop {
        let effect = effects.pop().expect("an effect until done");
        effects = match effect {
            ProjectStageEffect::Done(report) => return (report, trace),
            ProjectStageEffect::ListProjects => {
                let WireServerMsgBody::ListAvailableProjects { projects } =
                    board.request(ClientRequest::ListAvailableProjects)
                else {
                    panic!("listing");
                };
                stage.projects_listed(
                    projects
                        .iter()
                        .map(|p| String::from(p.path.as_str()))
                        .collect(),
                )
            }
            ProjectStageEffect::ReadStartupChoice => {
                stage.startup_choice_read(read(ServerConfig::PATH))
            }
            ProjectStageEffect::ReadManifest { project } => {
                let manifest = read(&format!("{}/project.json", absolute(&project)));
                stage.manifest_read(&project, manifest)
            }
            ProjectStageEffect::ReadProject { project } => {
                let root = absolute(&project);
                let FsResponse::ListDir {
                    entries,
                    error: None,
                    ..
                } = board.fs(FsRequest::ListDir {
                    path: root.as_str().as_path_buf(),
                    recursive: true,
                })
                else {
                    panic!("list {root}");
                };
                let entries: Vec<String> =
                    entries.iter().map(|e| String::from(e.as_str())).collect();
                let mut files = lpa_upgrade::ProjectFiles::new();
                for entry in &entries {
                    let dir = format!("{}/", entry.trim_end_matches('/'));
                    if entries.iter().any(|other| other.starts_with(&dir)) {
                        continue;
                    }
                    let key = entry.strip_prefix(&format!("{root}/")).expect("inside");
                    files.insert(key, read(entry).expect("a listed file reads"));
                }
                stage.project_read(&project, files)
            }
            ProjectStageEffect::WriteBatch {
                project,
                writes,
                deletes,
            } => {
                trace.push(format!("write {project}"));
                let root = absolute(&project);
                let FsResponse::Batch {
                    atomic,
                    error: None,
                    ..
                } = board.fs(FsRequest::BeginBatch)
                else {
                    panic!("begin");
                };
                let outcome = if !atomic {
                    BatchOutcome::NotAtomic
                } else {
                    for (path, data) in writes {
                        let FsResponse::Write { error: None, .. } = board.fs(FsRequest::Write {
                            path: format!("{root}/{path}").as_str().as_path_buf(),
                            data,
                        }) else {
                            panic!("write {path}");
                        };
                    }
                    for path in deletes {
                        let FsResponse::DeleteFile { error: None, .. } =
                            board.fs(FsRequest::DeleteFile {
                                path: format!("{root}/{path}").as_str().as_path_buf(),
                            })
                        else {
                            panic!("delete {path}");
                        };
                    }
                    match board.fs(FsRequest::CommitBatch) {
                        FsResponse::Batch { error: None, .. } => BatchOutcome::Committed,
                        other => BatchOutcome::Failed(format!("{other:?}")),
                    }
                };
                stage.batch_done(&project, outcome)
            }
            ProjectStageEffect::LoadProject { project } => {
                trace.push(format!("load {project}"));
                let result = match board.request(ClientRequest::LoadProject {
                    path: absolute(&project),
                }) {
                    WireServerMsgBody::LoadProject { .. } => Ok(()),
                    WireServerMsgBody::Error { error } => Err(error),
                    other => Err(format!("{other:?}")),
                };
                stage.load_done(&project, result)
            }
        };
    }
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

/// The hello the board answers on its primary link.
fn hello(board: &Board) -> lpc_wire::ServerHello {
    match board.request(ClientRequest::Hello) {
        WireServerMsgBody::Hello(hello) => hello,
        other => panic!("expected a hello, got {other:?}"),
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
