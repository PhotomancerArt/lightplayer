//! The server's fs batch, end to end through `LpServer::tick_and_send`: who
//! owns it, what ends it, what other links are told while it is open, and
//! the deflated write beside it — on the tree store (an atomic backend, on
//! simulated NOR flash) and on memory (a backend that commits each call).
//!
//! The firmware transports' reset hook (`take_reset_links`) is played here
//! by the test transport; its behaviour on the device is the emulator
//! walk's.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_nor_sim::{NorFlashSim, NorGeometry};
use lp_tree_store::{LpFsTree, SoftSha256, StoreConfig, TreeStore};
use lpa_server::batch_state::{BATCH_BUSY, BATCH_IDLE_TIMEOUT_MS};
use lpa_server::{LpGraphics, LpServer};
use lpc_model::{AsLpPath, AsLpPathBuf, FsVersion};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{Incoming, Link, LinkId, LinkTrust, ServerTransport};
use lpc_wire::server::{BatchOp, FileChangeKind, FsRequest, FsResponse};
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireServerMessage, WireServerMsgBody,
};
use lpfs::{LpFs, LpFsMemory};

const USB: Link = Link::PRIMARY;
/// A second trusted link (a second cable): edit tier, so its writes reach
/// the batch's refusal rather than the tier gate's.
const OTHER: Link = Link {
    id: LinkId::new(7),
    trust: LinkTrust::Trusted,
};

#[test]
fn a_board_without_transactions_opens_no_batch() {
    let mut rig = Rig::memory();
    assert_eq!(rig.begin(USB), (true, false, None));
    // Nothing is held: another link writes, and every write landed by itself.
    rig.write(USB, "/projects/a/f.txt", b"a").expect("written");
    rig.write(OTHER, "/projects/a/g.txt", b"g")
        .expect("written");
    assert_eq!(rig.commit(USB).2.as_deref(), Some("no batch is open"));
    assert_eq!(rig.read("/projects/a/f.txt").as_deref(), Some(&b"a"[..]));
}

#[test]
fn a_committed_batch_lands_and_survives_a_remount() {
    let mut rig = Rig::tree();
    assert_eq!(rig.begin(USB), (true, true, None));
    rig.write(USB, "/projects/a/f.txt", b"new")
        .expect("written");
    // The batch's own reads see its writes.
    assert_eq!(rig.read("/projects/a/f.txt").as_deref(), Some(&b"new"[..]));
    assert_eq!(rig.commit(USB), (true, true, None));
    let remounted = rig.remounted();
    assert_eq!(
        remounted.read_file("/projects/a/f.txt".as_path()).unwrap(),
        b"new"
    );
}

#[test]
fn another_links_writes_are_refused_while_its_reads_are_served() {
    let mut rig = Rig::tree();
    rig.seed("/projects/a/f.txt", b"old");
    rig.begin(USB);
    let refused = rig.write(OTHER, "/projects/a/g.txt", b"g").unwrap_err();
    assert_eq!(refused, BATCH_BUSY);
    let refused = rig.delete_dir(OTHER, "/projects/a").unwrap_err();
    assert_eq!(refused, BATCH_BUSY);
    assert_eq!(rig.begin(OTHER).2.as_deref(), Some(BATCH_BUSY));
    assert_eq!(rig.commit(OTHER).2.as_deref(), Some(BATCH_BUSY));
    // Its read goes on.
    match rig.fs(
        OTHER,
        FsRequest::Read {
            path: "/projects/a/f.txt".as_path_buf(),
        },
    ) {
        FsResponse::Read { data, error, .. } => {
            assert_eq!(error, None);
            assert_eq!(data.as_deref(), Some(&b"old"[..]));
        }
        other => panic!("{other:?}"),
    }
    // Once the batch ends, the other link writes again.
    rig.abort(USB);
    rig.write(OTHER, "/projects/a/g.txt", b"g")
        .expect("written");
}

#[test]
fn the_owner_link_closing_drops_its_batch() {
    let mut rig = Rig::tree();
    rig.seed("/projects/a/f.txt", b"old");
    rig.begin(OTHER);
    rig.write(OTHER, "/projects/a/f.txt", b"half")
        .expect("written");
    rig.transport.closed.push(OTHER.id);
    rig.idle(16);
    assert_eq!(rig.read("/projects/a/f.txt").as_deref(), Some(&b"old"[..]));
    // The next link's write is not in a batch: it lands by itself.
    rig.write(USB, "/projects/a/f.txt", b"next")
        .expect("written");
    assert_eq!(
        rig.remounted()
            .read_file("/projects/a/f.txt".as_path())
            .unwrap(),
        b"next"
    );
}

#[test]
fn the_owner_session_resetting_drops_its_batch() {
    let mut rig = Rig::tree();
    rig.seed("/projects/a/f.txt", b"old");
    rig.begin(USB);
    rig.write(USB, "/projects/a/f.txt", b"half")
        .expect("written");
    // The cable replugged: same link, a new session.
    rig.transport.reset.push(USB.id);
    rig.idle(16);
    assert_eq!(rig.read("/projects/a/f.txt").as_deref(), Some(&b"old"[..]));
    // The new session's first write does not join the old batch, and is
    // not held to it.
    rig.write(USB, "/projects/a/f.txt", b"next")
        .expect("written");
    assert_eq!(rig.commit(USB).2.as_deref(), Some("no batch is open"));
    assert_eq!(
        rig.remounted()
            .read_file("/projects/a/f.txt".as_path())
            .unwrap(),
        b"next"
    );
}

#[test]
fn an_idle_batch_is_dropped_and_its_owner_told_why() {
    let mut rig = Rig::tree();
    rig.seed("/projects/a/f.txt", b"old");
    rig.begin(USB);
    rig.write(USB, "/projects/a/f.txt", b"half")
        .expect("written");
    rig.idle(16); // the delta after a handled request is its handling time
    rig.idle((BATCH_IDLE_TIMEOUT_MS / 2) as u32);
    assert!(rig.batch_open(), "half the timeout");
    rig.idle((BATCH_IDLE_TIMEOUT_MS / 2) as u32);
    assert!(!rig.batch_open(), "the timeout");
    assert_eq!(rig.read("/projects/a/f.txt").as_deref(), Some(&b"old"[..]));
    // The owner's late write is refused in words, not landed outside the
    // batch it thinks it holds; so is its commit.
    let late = rig.write(USB, "/projects/a/f.txt", b"late").unwrap_err();
    assert!(late.contains("60 s"), "{late}");
    assert!(rig.commit(USB).2.unwrap().contains("dropped"));
    // An abort clears it; the next write lands.
    assert_eq!(rig.abort(USB).2, None);
    rig.write(USB, "/projects/a/f.txt", b"next")
        .expect("written");
}

/// A `LoadProject` that compiles for twenty seconds blocks the tick, so the
/// next tick's delta is twenty seconds: that is the request's own time and
/// must not count toward the timeout.
#[test]
fn a_long_request_between_requests_is_not_idle_time() {
    let mut rig = Rig::tree();
    rig.begin(USB);
    rig.write(USB, "/projects/a/f.txt", b"x").expect("written");
    // The tick after the long request: twenty seconds of handling.
    rig.idle(20_000);
    // Fifty more seconds of real quiet: 70 s since the last request, 50 s
    // of it idle.
    rig.idle(50_000);
    assert!(rig.batch_open(), "handling time excluded");
    assert_eq!(rig.commit(USB), (true, true, None));
}

#[test]
fn a_second_begin_from_the_owner_starts_a_fresh_batch() {
    let mut rig = Rig::tree();
    rig.begin(USB);
    rig.write(USB, "/projects/a/lost.txt", b"lost")
        .expect("written");
    // The client lost the first answer and begins again.
    assert_eq!(rig.begin(USB), (true, true, None));
    assert_eq!(rig.read("/projects/a/lost.txt"), None, "the old batch went");
    rig.write(USB, "/projects/a/kept.txt", b"kept")
        .expect("written");
    rig.commit(USB);
    let fs = rig.remounted();
    assert!(!fs.file_exists("/projects/a/lost.txt".as_path()).unwrap());
    assert_eq!(
        fs.read_file("/projects/a/kept.txt".as_path()).unwrap(),
        b"kept"
    );
}

#[test]
fn an_aborted_batch_leaves_the_change_log_as_it_was() {
    let mut rig = Rig::tree();
    rig.seed("/projects/a/f.txt", b"old");
    let before = rig.changes("/projects/a");
    rig.begin(USB);
    rig.delete_dir(USB, "/projects/a").expect("deleted");
    rig.write(USB, "/projects/a/g.txt", b"g").expect("written");
    rig.abort(USB);
    assert_eq!(rig.changes("/projects/a"), before);
}

#[test]
fn a_deflated_chunk_lands_as_its_logical_bytes_on_either_backend() {
    let bytes: Vec<u8> = (0..300u32)
        .flat_map(|i| alloc::format!("line {i}\n").into_bytes())
        .collect();
    for mut rig in [Rig::memory(), Rig::tree()] {
        rig.begin(USB);
        let mut written = 0;
        for chunk in lp_tree_store::plan_deflated_chunks(&bytes, 1024, 10) {
            let (got, error) = rig.deflated(
                USB,
                "/projects/a/s.glsl",
                chunk.logical_range.start as u32,
                chunk.logical_range.len() as u32,
                chunk.deflated,
            );
            assert_eq!(error, None);
            written += got as usize;
        }
        assert_eq!(written, bytes.len(), "written counts logical bytes");
        rig.commit(USB);
        assert_eq!(rig.read("/projects/a/s.glsl"), Some(bytes.clone()));
    }
}

#[test]
fn a_bad_deflated_chunk_is_refused_with_nothing_written_and_the_batch_kept() {
    for mut rig in [Rig::memory(), Rig::tree()] {
        rig.begin(USB);
        // Not a deflate stream.
        let (_, error) = rig.deflated(USB, "/projects/a/s.glsl", 0, 4, vec![0xff, 0xff]);
        assert!(error.is_some());
        // A lying length, far past the cap: refused before anything is
        // allocated.
        let (_, error) = rig.deflated(USB, "/projects/a/s.glsl", 0, 1 << 30, vec![0x03, 0x00]);
        assert!(error.unwrap().contains("too large"));
        // An offset that is not the file's end.
        let (_, error) = rig.deflated(USB, "/projects/a/s.glsl", 9, 0, vec![0x03, 0x00]);
        assert!(error.is_some());
        assert_eq!(rig.read("/projects/a/s.glsl"), None);
        if rig.atomic {
            assert!(rig.batch_open(), "the client decides to abort");
        }
        rig.abort(USB);
    }
}

// ---- the rig --------------------------------------------------------------

struct Rig {
    server: LpServer,
    transport: LinkTransport,
    /// The tree store's other handle (`None` on memory).
    tree: Option<LpFsTree<NorFlashSim, SoftSha256>>,
    atomic: bool,
    next_id: u64,
}

impl Rig {
    fn memory() -> Self {
        Self::over(Box::new(LpFsMemory::new()), None, false)
    }

    fn tree() -> Self {
        let tree = LpFsTree::new(mount(formatted()));
        let handle = tree.handle();
        Self::over(Box::new(tree), Some(handle), true)
    }

    fn over(
        fs: Box<dyn LpFs>,
        tree: Option<LpFsTree<NorFlashSim, SoftSha256>>,
        atomic: bool,
    ) -> Self {
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
            server,
            transport: LinkTransport::default(),
            tree,
            atomic,
            next_id: 100,
        }
    }

    /// Write `path` outside any batch, straight to the filesystem.
    fn seed(&mut self, path: &str, data: &[u8]) {
        self.server
            .base_fs_mut()
            .write_file(path.as_path(), data)
            .unwrap();
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.server.base_fs().read_file(path.as_path()).ok()
    }

    /// Asked of the server, not by a request: a request would itself reset
    /// the idle clock.
    fn batch_open(&self) -> bool {
        self.server.fs_batch_owner().is_some()
    }

    /// The flash as it is now, mounted afresh (what a reboot would see).
    fn remounted(&self) -> LpFsTree<NorFlashSim, SoftSha256> {
        let flash = self
            .tree
            .as_ref()
            .expect("a tree rig")
            .with_store(|st| st.flash().clone());
        LpFsTree::new(mount(flash))
    }

    fn changes(&mut self, prefix: &str) -> Vec<(String, FileChangeKind)> {
        match self.fs(
            USB,
            FsRequest::ChangesSince {
                prefix: prefix.as_path_buf(),
                since: FsVersion::new(1),
                cursor: None,
            },
        ) {
            FsResponse::Changes { entries, error, .. } => {
                assert_eq!(error, None);
                entries
                    .into_iter()
                    .map(|e| (e.path.as_str().to_string(), e.kind))
                    .collect()
            }
            other => panic!("{other:?}"),
        }
    }

    /// `(answered as a batch, atomic, error)`.
    fn begin(&mut self, link: Link) -> (bool, bool, Option<String>) {
        self.batch(link, FsRequest::BeginBatch, BatchOp::Begin)
    }

    fn commit(&mut self, link: Link) -> (bool, bool, Option<String>) {
        self.batch(link, FsRequest::CommitBatch, BatchOp::Commit)
    }

    fn abort(&mut self, link: Link) -> (bool, bool, Option<String>) {
        self.batch(link, FsRequest::AbortBatch, BatchOp::Abort)
    }

    fn batch(
        &mut self,
        link: Link,
        request: FsRequest,
        expected: BatchOp,
    ) -> (bool, bool, Option<String>) {
        match self.fs(link, request) {
            FsResponse::Batch { op, atomic, error } => {
                assert_eq!(op, expected);
                (true, atomic, error)
            }
            other => panic!("{other:?}"),
        }
    }

    fn write(&mut self, link: Link, path: &str, data: &[u8]) -> Result<(), String> {
        match self.fs(
            link,
            FsRequest::Write {
                path: path.as_path_buf(),
                data: data.to_vec(),
            },
        ) {
            FsResponse::Write { error: None, .. } => Ok(()),
            FsResponse::Write { error: Some(e), .. } => Err(e),
            other => panic!("{other:?}"),
        }
    }

    fn delete_dir(&mut self, link: Link, path: &str) -> Result<(), String> {
        match self.fs(
            link,
            FsRequest::DeleteDir {
                path: path.as_path_buf(),
            },
        ) {
            FsResponse::DeleteDir { error: None, .. } => Ok(()),
            FsResponse::DeleteDir { error: Some(e), .. } => Err(e),
            other => panic!("{other:?}"),
        }
    }

    /// `(written, error)`.
    fn deflated(
        &mut self,
        link: Link,
        path: &str,
        offset: u32,
        logical_len: u32,
        data: Vec<u8>,
    ) -> (u32, Option<String>) {
        match self.fs(
            link,
            FsRequest::WriteChunkDeflated {
                path: path.as_path_buf(),
                offset,
                logical_len,
                data,
            },
        ) {
            FsResponse::WriteChunk { written, error, .. } => (written, error),
            other => panic!("{other:?}"),
        }
    }

    fn fs(&mut self, link: Link, request: FsRequest) -> FsResponse {
        match self.request(link, ClientRequest::Filesystem(request)) {
            WireServerMsgBody::Filesystem(response) => response,
            other => panic!("{other:?}"),
        }
    }

    fn request(&mut self, link: Link, request: ClientRequest) -> WireServerMsgBody {
        self.next_id += 1;
        let id = self.next_id;
        self.transport.sent.clear();
        let incoming = Incoming::on(link, ClientMessage { id, msg: request });
        block_on(
            self.server
                .tick_and_send(16, vec![incoming], &mut self.transport),
        )
        .expect("tick");
        let (reply_link, reply) = self
            .transport
            .sent
            .pop()
            .expect("every request gets a reply");
        assert_eq!(reply_link, link.id);
        assert_eq!(reply.id, id);
        reply.msg
    }

    fn idle(&mut self, delta_ms: u32) {
        block_on(
            self.server
                .tick_and_send(delta_ms, Vec::new(), &mut self.transport),
        )
        .expect("tick");
    }
}

fn config() -> StoreConfig {
    StoreConfig::default()
}

fn formatted() -> NorFlashSim {
    let mut flash = NorFlashSim::new(NorGeometry::c6(32));
    TreeStore::format(&mut flash, &mut SoftSha256, &config()).expect("format");
    flash
}

fn mount(flash: NorFlashSim) -> TreeStore<NorFlashSim, SoftSha256> {
    match TreeStore::mount(flash, SoftSha256, config()) {
        Ok(store) => store,
        Err((error, _, _)) => panic!("mount: {error:?}"),
    }
}

/// Two trusted links, closed and reset when the test says so.
#[derive(Default)]
struct LinkTransport {
    sent: Vec<(LinkId, WireServerMessage)>,
    closed: Vec<LinkId>,
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
        vec![USB, OTHER]
    }

    fn take_closed_links(&mut self) -> Vec<LinkId> {
        core::mem::take(&mut self.closed)
    }

    fn take_reset_links(&mut self) -> Vec<LinkId> {
        core::mem::take(&mut self.reset)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
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
