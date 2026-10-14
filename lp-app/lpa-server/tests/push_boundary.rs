//! The push boundary end to end on the host: a real `LpServer` and the real
//! `lpa_client` push conversation over an in-process loopback.
//!
//! - An **atomic** board (the tree store on simulated NOR flash) gets the
//!   one-slot push; a **non-atomic** one (memory) the two-slot push. Both
//!   end hash-equal to the host's own hash of the files.
//! - The owner's link resetting mid-batch, the idle timeout, a corrupt
//!   deflated chunk and a project the board refuses all leave the old
//!   project.
//! - **Cuts**: power cut at sampled points of one server-driven push, under
//!   every tear model `lp-nor-sim` names (the calibrated ones included);
//!   after each, the remounted flash holds the old project or the new one,
//!   never a mix.
//! - **Measurements** on the repo's corpus (`catalog/` + `projects/`): wire
//!   bytes and request counts with and without deflate, fixed 4 KiB chunks
//!   against the shrink-to-fit plan.
//!
//! Simulator and host numbers: no radio, no real flash timing, the frame
//! clock is the loopback's own 16 ms ticks. `LP_PUSH_SWEEP_CUTS=<n>` widens
//! the sweep (default 24 cuts per tear model).

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_nor_sim::{FaultPlan, NorFlashSim, NorGeometry, TearModel};
use lp_tree_store::{LpFsTree, SoftSha256, StoreConfig, TreeStore};
use lpa_client::push_files::{LpClientSink, abort_batch, begin_batch, file_requests};
use lpa_client::{ClientIo, LpClient, push_project};
use lpa_server::{LpGraphics, LpServer};
use lpc_model::{AsLpPath, AsLpPathBuf};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{Incoming, Link, LinkId, ServerTransport};
use lpc_wire::server::{FsRequest, FsResponse};
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireServerMessage, WireServerMsgBody,
};
use lpfs::{LpFs, LpFsMemory};

const PROJECT: &str = "projects/test/basic";

#[test]
fn an_atomic_board_takes_the_push_in_its_own_folder() {
    let board = Board::tree(formatted());
    let a = project_files(PROJECT);
    let b = edited(&a, "first edit");
    let report = board.push(&a).expect("first push");
    assert_eq!(report.storage_id, "demo", "a fresh board's fallback");
    assert_eq!(report.hash, host_hash(&a));
    board.settle();
    let report = board.push(&b).expect("second push");
    assert_eq!(report.storage_id, "demo", "the same folder, no -b slot");
    assert_eq!(report.hash, host_hash(&b));
    assert_eq!(board.project_dirs(), ["demo"]);
    assert_eq!(board.running(), Some("/projects/demo".into()));
}

#[test]
fn a_board_without_batches_takes_the_two_slot_push() {
    let board = Board::memory();
    let a = project_files(PROJECT);
    let b = edited(&a, "first edit");
    assert_eq!(board.push(&a).expect("first push").storage_id, "demo");
    board.settle();
    let report = board.push(&b).expect("second push");
    assert_eq!(report.storage_id, "demo-b", "today's other slot");
    assert_eq!(report.hash, host_hash(&b));
    assert_eq!(board.project_dirs(), ["demo-b"], "the old slot removed");
}

#[test]
fn a_replug_mid_batch_leaves_the_old_project_and_no_stale_batch() {
    let board = Board::tree(formatted());
    let a = project_files(PROJECT);
    board.push(&a).expect("push a");
    board.settle();
    let a_hash = board.hash("demo");
    // A push that begins, clears and half-writes, then loses its cable.
    {
        let mut client = board.client();
        board.request(&mut client, ClientRequest::StopAllProjects);
        assert!(block_on(begin_batch(&mut LpClientSink::new(&mut client))).unwrap());
        board.request(
            &mut client,
            ClientRequest::Filesystem(FsRequest::DeleteDir {
                path: "/projects/demo".as_path_buf(),
            }),
        );
        let writes = file_requests("demo", &a[0].0, &edited(&a, "lost")[0].1, true);
        board.request(&mut client, writes[0].clone());
    }
    board.reset_link();
    board.idle(1);
    assert_eq!(board.hash("demo"), a_hash, "the old project, whole");
    // The next session's first write lands by itself: no stale batch.
    let mut client = board.client();
    let answer = board.request(
        &mut client,
        ClientRequest::Filesystem(FsRequest::Write {
            path: "/projects/demo/note.txt".as_path_buf(),
            data: b"after".to_vec(),
        }),
    );
    assert!(matches!(
        answer,
        WireServerMsgBody::Filesystem(FsResponse::Write { error: None, .. })
    ));
    assert!(
        board
            .remounted()
            .file_exists("/projects/demo/note.txt".as_path())
            .unwrap()
    );
}

#[test]
fn an_idle_batch_expires_and_the_old_project_stays() {
    let board = Board::tree(formatted());
    let a = project_files(PROJECT);
    board.push(&a).expect("push a");
    board.settle();
    let a_hash = board.hash("demo");
    let mut client = board.client();
    assert!(block_on(begin_batch(&mut LpClientSink::new(&mut client))).unwrap());
    board.request(
        &mut client,
        ClientRequest::Filesystem(FsRequest::DeleteDir {
            path: "/projects/demo".as_path_buf(),
        }),
    );
    board.idle(1);
    board.idle_ms(61_000);
    assert_eq!(board.hash("demo"), a_hash);
    let _ = block_on(abort_batch(&mut LpClientSink::new(&mut client)));
}

#[test]
fn a_corrupt_deflated_chunk_writes_nothing() {
    let board = Board::tree(formatted());
    let mut client = board.client();
    let answer = board.request(
        &mut client,
        ClientRequest::Filesystem(FsRequest::WriteChunkDeflated {
            path: "/projects/x/s.glsl".as_path_buf(),
            offset: 0,
            logical_len: 64,
            data: vec![0xde, 0xad, 0xbe, 0xef],
        }),
    );
    match answer {
        WireServerMsgBody::Filesystem(FsResponse::WriteChunk { error, written, .. }) => {
            assert!(error.is_some());
            assert_eq!(written, 0);
        }
        other => panic!("{other:?}"),
    }
    assert!(
        !board
            .remounted()
            .file_exists("/projects/x/s.glsl".as_path())
            .unwrap()
    );
}

#[test]
fn a_project_the_board_refuses_aborts_and_the_old_one_runs_again() {
    let board = Board::tree(formatted());
    let a = project_files(PROJECT);
    board.push(&a).expect("push a");
    board.settle();
    let a_hash = board.hash("demo");
    let mut refused = a.clone();
    for (path, bytes) in &mut refused {
        if path == "project.json" {
            *bytes = br#"{"format":9999}"#.to_vec();
        }
    }
    let error = board.push(&refused).expect_err("refused");
    let message = error.to_string();
    assert!(
        message.contains("running its previous project (demo) again"),
        "{message}"
    );
    assert_eq!(board.hash("demo"), a_hash, "the old project, whole");
    assert_eq!(board.running(), Some("/projects/demo".into()));
}

/// Power cut at sampled points of one push, every named tear model: the
/// project is old or new after the remount, never a mix.
#[test]
fn every_cut_of_a_server_driven_push_leaves_the_old_project_or_the_new() {
    sweep_push(PROJECT);
}

/// The same sweep over the catalog's largest project (73 KB, multi-chunk
/// files): many more flash ops, sampled.
#[test]
fn every_cut_of_a_big_push_leaves_the_old_project_or_the_new() {
    sweep_push("catalog/projects/playful-choker-tryout");
}

fn sweep_push(project: &str) {
    let a = project_files(project);
    let b = edited(&a, "the new one");
    let board = Board::tree(formatted());
    board.push(&a).expect("push a");
    board.settle();
    let before = board.flash();
    let old = snapshot(&before);

    // Fault-free: the new state and the push's flash op count.
    let board = Board::tree(before.clone());
    board.set_plan(FaultPlan::none());
    board.push(&b).expect("push b");
    let ops = board.ops_since_plan();
    let new = snapshot(&board.flash());
    assert_ne!(old, new);

    let cuts = dial("LP_PUSH_SWEEP_CUTS", 24);
    let mut landed_old = 0;
    let mut landed_new = 0;
    for k in sample(ops, cuts) {
        for tear in TearModel::NAMED {
            let seed = k << 4 | tear as u64;
            let board = Board::tree(before.clone());
            board.set_plan(FaultPlan::cut(k, tear, seed));
            let _ = board.push(&b);
            let mut flash = board.flash();
            flash.power_cycle(FaultPlan::none());
            let got = snapshot(&flash);
            if got == new {
                landed_new += 1;
            } else {
                assert!(
                    got == old,
                    "cut {k}/{ops} {tear:?}: the project is neither old nor new"
                );
                landed_old += 1;
            }
        }
    }
    std::println!(
        "push cut sweep of {project} (simulator, host): {ops} flash ops, {} cuts x {} tear models: \
         {landed_old} old, {landed_new} new",
        sample(ops, cuts).len(),
        TearModel::NAMED.len()
    );
    assert!(landed_old > 0 && landed_new > 0);
}

/// Wire bytes and request counts on the repo's corpus, each file pushed on
/// its own: raw (today), shrink-to-fit deflate (the push), and fixed 4 KiB
/// deflated chunks (the alternative D6 rejected). Printed, and a floor
/// asserted on the gain.
#[test]
fn measure_the_wire_on_the_corpus() {
    let root = repo_root();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for dir in ["catalog", "projects"] {
        collect(&root.join(dir), &root, &mut files);
    }
    let logical: usize = files.iter().map(|(_, b)| b.len()).sum();

    let wire = |requests: &[ClientRequest]| -> usize {
        requests
            .iter()
            .map(|msg| {
                lpc_wire::json::to_string(&ClientMessage {
                    id: 1_000,
                    msg: msg.clone(),
                })
                .unwrap()
                .len()
            })
            .sum()
    };
    let (mut raw_requests, mut raw_bytes) = (0, 0);
    let (mut planned_requests, mut planned_bytes, mut deflated_files) = (0, 0, 0);
    let (mut fixed_requests, mut fixed_bytes) = (0, 0);
    let mut at_rest_planned = 0usize;
    let mut at_rest_fixed = 0usize;
    for (path, bytes) in &files {
        let raw = file_requests("p", path, bytes, false);
        raw_requests += raw.len();
        raw_bytes += wire(&raw);

        let planned = file_requests("p", path, bytes, true);
        let deflated = planned.iter().any(|r| {
            matches!(
                r,
                ClientRequest::Filesystem(FsRequest::WriteChunkDeflated { .. })
            )
        });
        deflated_files += usize::from(deflated);
        planned_requests += planned.len();
        planned_bytes += wire(&planned);
        // At rest on a tree-store board: a chunk is kept deflated when it
        // fits one record (the plan makes it so), else as its plain bytes.
        at_rest_planned += planned
            .iter()
            .map(|r| match r {
                ClientRequest::Filesystem(FsRequest::WriteChunkDeflated { data, .. }) => data.len(),
                ClientRequest::Filesystem(
                    FsRequest::Write { data, .. } | FsRequest::WriteChunk { data, .. },
                ) => data.len(),
                _ => 0,
            })
            .sum::<usize>();

        // Fixed 4 KiB logical chunks, deflated (same 10 % rule per file).
        let chunks: Vec<Vec<u8>> = bytes
            .chunks(4096)
            .map(|c| miniz_oxide::deflate::compress_to_vec(c, 10))
            .collect();
        let total: usize = chunks.iter().map(Vec::len).sum();
        if bytes.len() >= 64 && total * 10 <= bytes.len() * 9 {
            let requests: Vec<ClientRequest> = bytes
                .chunks(4096)
                .zip(&chunks)
                .enumerate()
                .map(|(i, (c, z))| {
                    ClientRequest::Filesystem(FsRequest::WriteChunkDeflated {
                        path: format!("/projects/p/{path}").as_str().as_path_buf(),
                        offset: (i * 4096) as u32,
                        logical_len: c.len() as u32,
                        data: z.clone(),
                    })
                })
                .collect();
            fixed_requests += requests.len();
            fixed_bytes += wire(&requests);
            // 1,024-byte records hold ~1,006 B of deflate; a bigger one is
            // kept plain.
            at_rest_fixed += bytes
                .chunks(4096)
                .zip(&chunks)
                .map(|(c, z)| {
                    if z.len() + 2 <= 1008 {
                        z.len()
                    } else {
                        c.len()
                    }
                })
                .sum::<usize>();
        } else {
            fixed_requests += raw.len();
            fixed_bytes += wire(&raw);
            at_rest_fixed += bytes.len();
        }
    }
    let pct = |a: usize, b: usize| a as f64 * 100.0 / b as f64;
    std::println!(
        "corpus (simulator, host): {} files, {logical} B logical",
        files.len()
    );
    std::println!("| form | requests | wire bytes | vs raw | at rest |");
    std::println!("|---|---:|---:|---:|---:|");
    std::println!("| raw (today) | {raw_requests} | {raw_bytes} | 100.0 % | {logical} |");
    std::println!(
        "| deflate, shrink-to-fit (this push; {deflated_files} files deflated) | \
         {planned_requests} | {planned_bytes} | {:.1} % | {at_rest_planned} ({:.1} %) |",
        pct(planned_bytes, raw_bytes),
        pct(at_rest_planned, logical)
    );
    std::println!(
        "| deflate, fixed 4 KiB | {fixed_requests} | {fixed_bytes} | {:.1} % | {at_rest_fixed} ({:.1} %) |",
        pct(fixed_bytes, raw_bytes),
        pct(at_rest_fixed, logical)
    );
    assert!(planned_bytes < raw_bytes, "deflate must save wire bytes");
}

// ---- the board --------------------------------------------------------------

/// A server on one link, and a loopback client to it.
struct Board {
    inner: Rc<RefCell<Inner>>,
}

struct Inner {
    server: LpServer,
    transport: LinkTransport,
    tree: Option<LpFsTree<NorFlashSim, SoftSha256>>,
}

impl Board {
    fn tree(flash: NorFlashSim) -> Self {
        let tree = LpFsTree::new(mount(flash));
        let handle = tree.handle();
        Self::over(Box::new(tree), Some(handle))
    }

    fn memory() -> Self {
        Self::over(Box::new(LpFsMemory::new()), None)
    }

    fn over(fs: Box<dyn LpFs>, tree: Option<LpFsTree<NorFlashSim, SoftSha256>>) -> Self {
        let server = LpServer::new(
            Rc::new(RefCell::new(MemoryOutputProvider::new())),
            fs,
            "/projects/".as_path(),
            None,
            None,
            Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND))
                as Arc<dyn LpGraphics>,
        );
        Self {
            inner: Rc::new(RefCell::new(Inner {
                server,
                transport: LinkTransport::default(),
                tree,
            })),
        }
    }

    fn client(&self) -> LpClient<Loopback> {
        LpClient::new(Loopback {
            board: Rc::clone(&self.inner),
            pending: Vec::new(),
            answers: VecDeque::new(),
        })
        .on_borrowed_wire()
    }

    fn push(
        &self,
        files: &[(String, Vec<u8>)],
    ) -> lpa_client::ClientResult<lpa_client::PushReport> {
        let mut client = self.client();
        let mut progress = |_label: String, _percent: Option<u8>| {};
        block_on(push_project(
            &mut client,
            files,
            &host_hash(files),
            "demo",
            &mut progress,
        ))
    }

    fn request(
        &self,
        client: &mut LpClient<Loopback>,
        request: ClientRequest,
    ) -> WireServerMsgBody {
        block_on(client.send_request(request))
            .expect("answered")
            .value
            .msg
    }

    /// A few frames, so a loaded project's startup choice is written.
    fn settle(&self) {
        for _ in 0..8 {
            self.idle(1);
        }
    }

    fn idle(&self, ticks: u32) {
        for _ in 0..ticks {
            self.idle_ms(16);
        }
    }

    fn idle_ms(&self, delta_ms: u32) {
        let mut inner = self.inner.borrow_mut();
        let Inner {
            server, transport, ..
        } = &mut *inner;
        block_on(server.tick_and_send(delta_ms, Vec::new(), transport)).expect("tick");
        transport.sent.clear();
    }

    fn reset_link(&self) {
        self.inner
            .borrow_mut()
            .transport
            .reset
            .push(LinkId::PRIMARY);
    }

    fn hash(&self, dir: &str) -> String {
        let inner = self.inner.borrow();
        let view = inner
            .server
            .base_fs()
            .chroot(format!("/projects/{dir}").as_str().as_path())
            .unwrap();
        let view = view.borrow();
        lpc_history::hash_package(&*view).unwrap().0.to_string()
    }

    fn project_dirs(&self) -> Vec<String> {
        let inner = self.inner.borrow();
        let mut dirs: Vec<String> = inner
            .server
            .base_fs()
            .list_dir("/projects".as_path(), false)
            .unwrap()
            .into_iter()
            .map(|p| p.as_str().trim_start_matches("/projects/").to_string())
            .collect();
        dirs.sort();
        dirs
    }

    fn running(&self) -> Option<String> {
        let inner = self.inner.borrow();
        inner
            .server
            .project_manager()
            .list_loaded_projects()
            .first()
            .map(|p| p.path.as_str().to_string())
    }

    fn tree_handle<T>(&self, f: impl FnOnce(&mut TreeStore<NorFlashSim, SoftSha256>) -> T) -> T {
        self.inner
            .borrow()
            .tree
            .as_ref()
            .expect("a tree board")
            .with_store(f)
    }

    fn flash(&self) -> NorFlashSim {
        self.tree_handle(|st| st.flash().clone())
    }

    fn set_plan(&self, plan: FaultPlan) {
        self.tree_handle(|st| st.flash_mut().set_plan(plan));
    }

    fn ops_since_plan(&self) -> u64 {
        self.tree_handle(|st| st.flash().ops_since_plan())
    }

    fn remounted(&self) -> LpFsTree<NorFlashSim, SoftSha256> {
        LpFsTree::new(mount(self.flash()))
    }
}

/// The client's side of the loopback: a request is queued; asking for an
/// answer runs the server's tick over what is queued.
struct Loopback {
    board: Rc<RefCell<Inner>>,
    pending: Vec<Incoming>,
    answers: VecDeque<WireServerMessage>,
}

#[async_trait(?Send)]
impl ClientIo for Loopback {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        self.pending.push(Incoming::on(Link::PRIMARY, msg));
        Ok(())
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        for _ in 0..4 {
            if let Some(answer) = self.answers.pop_front() {
                return Ok(answer);
            }
            let incoming = core::mem::take(&mut self.pending);
            let mut inner = self.board.borrow_mut();
            let Inner {
                server, transport, ..
            } = &mut *inner;
            server
                .tick_and_send(16, incoming, transport)
                .await
                .map_err(|e| TransportError::Other(format!("{e}")))?;
            self.answers
                .extend(transport.sent.drain(..).map(|(_, message)| message));
        }
        self.answers
            .pop_front()
            .ok_or(TransportError::ConnectionLost)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

#[derive(Default)]
struct LinkTransport {
    sent: Vec<(LinkId, WireServerMessage)>,
    reset: Vec<LinkId>,
}

impl ServerTransport for LinkTransport {
    async fn send(&mut self, link: LinkId, msg: WireServerMessage) -> Result<(), TransportError> {
        self.sent.push((link, msg));
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

    fn take_reset_links(&mut self) -> Vec<LinkId> {
        core::mem::take(&mut self.reset)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

// ---- helpers ------------------------------------------------------------------

/// Every file under `/projects/` on `flash`, mounted afresh.
fn snapshot(flash: &NorFlashSim) -> BTreeMap<String, Vec<u8>> {
    let mut st = mount(flash.clone());
    let mut out = BTreeMap::new();
    for path in st.list("/projects/").expect("list") {
        let bytes = st.get(&path).expect("get").expect("listed");
        out.insert(path, bytes);
    }
    out
}

fn project_files(dir: &str) -> Vec<(String, Vec<u8>)> {
    let root = repo_root().join(dir);
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    files
}

/// `files` with the shader changed (a new project the board will load).
fn edited(files: &[(String, Vec<u8>)], marker: &str) -> Vec<(String, Vec<u8>)> {
    files
        .iter()
        .map(|(path, bytes)| {
            let mut bytes = bytes.clone();
            if path.ends_with(".glsl") {
                bytes.extend_from_slice(format!("\n// {marker}\n").as_bytes());
            }
            (path.clone(), bytes)
        })
        .collect()
}

fn host_hash(files: &[(String, Vec<u8>)]) -> String {
    let mirror = LpFsMemory::new();
    for (path, bytes) in files {
        mirror
            .write_file(format!("/{path}").as_str().as_path(), bytes)
            .unwrap();
    }
    lpc_history::hash_package(&mirror).unwrap().0.to_string()
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
                .into_owned();
            out.push((rel, std::fs::read(&path).unwrap()));
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn config() -> StoreConfig {
    StoreConfig::default()
}

fn formatted() -> NorFlashSim {
    let mut flash = NorFlashSim::new(NorGeometry::c6(48));
    TreeStore::format(&mut flash, &mut SoftSha256, &config()).expect("format");
    flash
}

fn mount(flash: NorFlashSim) -> TreeStore<NorFlashSim, SoftSha256> {
    match TreeStore::mount(flash, SoftSha256, config()) {
        Ok(store) => store,
        Err((error, _, _)) => panic!("mount: {error:?}"),
    }
}

fn dial(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn sample(n: u64, max: u64) -> Vec<u64> {
    if n < max {
        return (0..=n).collect();
    }
    let mut v: Vec<u64> = (0..max).map(|i| i * n / (max - 1)).collect();
    v.dedup();
    v
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
