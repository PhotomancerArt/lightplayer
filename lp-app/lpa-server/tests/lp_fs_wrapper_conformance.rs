//! Every `LpFs` wrapper forwards every method to what it wraps.
//!
//! A wrapper that leaves a defaulted trait method to its default answers for
//! the filesystem underneath without asking it. That is how a batch begun
//! through `AccessGuardedFs` became a silent per-call commit (the guard took
//! `begin_batch`'s `Ok(())` default) — the class this test closes
//! (`docs/defects/2026-10-10-lp-fs-wrappers-silently-take-trait-defaults.md`).
//!
//! The wrapped filesystem here is a recorder: every method notes its name
//! and answers from an in-memory map. Each wrapper is driven through
//! [`EVERY_METHOD`] and the recorder must have heard each call, except where
//! the wrapper's own contract says otherwise (a view's change-log
//! bookkeeping belongs to its parent; the guard refuses a write-only file's
//! read; a view's chroot is a longer prefix over the same parent). Both the recorder and the wrappers carry
//! `#[deny(clippy::missing_trait_methods)]`, so a new `LpFs` method fails
//! clippy until each of them implements it — and when you add one, add it to
//! [`EVERY_METHOD`] and [`call`] below.

use std::cell::RefCell;
use std::rc::Rc;

use lpa_server::AccessGuardedFs;
use lpfs::{
    AsLpPath, FsError, FsEvent, FsEventKind, FsVersion, LpFs, LpFsMemory, LpFsView, LpPath,
    LpPathBuf,
};

#[test]
fn a_view_forwards_every_method_to_its_parent() {
    for &method in EVERY_METHOD {
        let (recorder, heard) = Recorder::new();
        let parent: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(recorder));
        let mut view = LpFsView::new(parent, "/projects/demo/".as_path());
        call(&mut view, method, "/a.txt");
        // A view of a view is a longer prefix over the same parent, and the
        // change log's bookkeeping is the parent's own.
        let local = matches!(
            method,
            "chroot" | "clear_changes_before" | "record_changes"
        );
        assert_eq!(
            heard.borrow().contains(&method),
            !local,
            "LpFsView::{method}: heard {:?}",
            heard.borrow()
        );
    }
}

#[test]
fn the_access_guard_forwards_every_method_on_an_ordinary_path() {
    for &method in EVERY_METHOD {
        let (recorder, heard) = Recorder::new();
        let mut guard = AccessGuardedFs::new(Rc::new(RefCell::new(recorder)));
        call(&mut guard, method, "/a.txt");
        assert!(
            heard.borrow().contains(&method),
            "AccessGuardedFs::{method}: heard {:?}",
            heard.borrow()
        );
    }
}

#[test]
fn the_access_guard_refuses_only_the_read_of_a_write_only_file() {
    for &method in EVERY_METHOD {
        let (recorder, heard) = Recorder::new();
        let mut guard = AccessGuardedFs::new(Rc::new(RefCell::new(recorder)));
        call(&mut guard, method, "/.lp/access.json");
        let refused = method == "read_file";
        assert_eq!(
            heard.borrow().contains(&method),
            !refused,
            "AccessGuardedFs::{method} on a write-only file: heard {:?}",
            heard.borrow()
        );
    }
}

/// Every method of `LpFs`, by name. Keep in step with the trait (and
/// [`call`]): the recorder's `missing_trait_methods` lint names any it lacks.
const EVERY_METHOD: &[&str] = &[
    "read_file",
    "write_file",
    "append_file",
    "file_size",
    "file_exists",
    "is_dir",
    "list_dir",
    "delete_file",
    "delete_dir",
    "chroot",
    "begin_batch",
    "commit_batch",
    "abort_batch",
    "batches_are_atomic",
    "write_deflated_chunk",
    "current_version",
    "get_changes_since",
    "get_events_since",
    "clear_changes_before",
    "record_changes",
];

/// Call `method` on `fs` with `path`. Errors are ignored: the question is
/// whether the call reached the recorder, not what it answered.
fn call(fs: &mut dyn LpFs, method: &str, path: &str) {
    let path = path.as_path();
    let since = FsVersion::default();
    match method {
        "read_file" => drop(fs.read_file(path)),
        "write_file" => drop(fs.write_file(path, b"x")),
        "append_file" => drop(fs.append_file(path, b"x")),
        "file_size" => drop(fs.file_size(path)),
        "file_exists" => drop(fs.file_exists(path)),
        "is_dir" => drop(fs.is_dir(path)),
        "list_dir" => drop(fs.list_dir(path, true)),
        "delete_file" => drop(fs.delete_file(path)),
        "delete_dir" => drop(fs.delete_dir(path)),
        "chroot" => drop(fs.chroot("/sub".as_path())),
        "begin_batch" => drop(fs.begin_batch()),
        "commit_batch" => drop(fs.commit_batch()),
        "abort_batch" => drop(fs.abort_batch()),
        "batches_are_atomic" => drop(fs.batches_are_atomic()),
        "write_deflated_chunk" => drop(fs.write_deflated_chunk(path, 0, 2, &STORED_HI)),
        "current_version" => drop(fs.current_version()),
        "get_changes_since" => drop(fs.get_changes_since(since)),
        "get_events_since" => drop(fs.get_events_since(since)),
        "clear_changes_before" => fs.clear_changes_before(since),
        "record_changes" => fs.record_changes(vec![FsEvent {
            path: LpPathBuf::from("/a.txt"),
            kind: FsEventKind::Modify,
        }]),
        other => panic!("no call for {other}: add it here"),
    }
}

/// `b"hi"` as one stored deflate block (RFC 1951 §3.2.4): BFINAL=1, BTYPE=00,
/// LEN=2, NLEN=!2, the bytes. A wrapper that defaulted the deflated write
/// would inflate it and call `write_file` instead.
const STORED_HI: [u8; 7] = [0x01, 0x02, 0x00, 0xfd, 0xff, b'h', b'i'];

type Heard = Rc<RefCell<Vec<&'static str>>>;

/// An `LpFs` that notes every call and answers from memory.
struct Recorder {
    heard: Heard,
    files: Rc<LpFsMemory>,
}

impl Recorder {
    fn new() -> (Self, Heard) {
        let heard: Heard = Rc::default();
        let recorder = Self {
            heard: Rc::clone(&heard),
            files: Rc::new(LpFsMemory::new()),
        };
        (recorder, heard)
    }

    fn hit(&self, method: &'static str) {
        assert!(
            EVERY_METHOD.contains(&method),
            "{method} is not in EVERY_METHOD: add it there and to `call`"
        );
        self.heard.borrow_mut().push(method);
    }
}

#[deny(clippy::missing_trait_methods)]
impl LpFs for Recorder {
    fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, FsError> {
        self.hit("read_file");
        self.files.read_file(path)
    }

    fn write_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        self.hit("write_file");
        self.files.write_file(path, data)
    }

    fn append_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        self.hit("append_file");
        self.files.append_file(path, data)
    }

    fn file_size(&self, path: &LpPath) -> Result<u64, FsError> {
        self.hit("file_size");
        self.files.file_size(path)
    }

    fn file_exists(&self, path: &LpPath) -> Result<bool, FsError> {
        self.hit("file_exists");
        self.files.file_exists(path)
    }

    fn is_dir(&self, path: &LpPath) -> Result<bool, FsError> {
        self.hit("is_dir");
        self.files.is_dir(path)
    }

    fn list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>, FsError> {
        self.hit("list_dir");
        self.files.list_dir(path, recursive)
    }

    fn delete_file(&self, path: &LpPath) -> Result<(), FsError> {
        self.hit("delete_file");
        self.files.delete_file(path)
    }

    fn delete_dir(&self, path: &LpPath) -> Result<(), FsError> {
        self.hit("delete_dir");
        self.files.delete_dir(path)
    }

    fn chroot(&self, subdir: &LpPath) -> Result<Rc<RefCell<dyn LpFs>>, FsError> {
        self.hit("chroot");
        let again = Self {
            heard: Rc::clone(&self.heard),
            files: Rc::clone(&self.files),
        };
        Ok(Rc::new(RefCell::new(LpFsView::new(
            Rc::new(RefCell::new(again)),
            subdir,
        ))))
    }

    fn begin_batch(&self) -> Result<(), FsError> {
        self.hit("begin_batch");
        Ok(())
    }

    fn commit_batch(&self) -> Result<(), FsError> {
        self.hit("commit_batch");
        Ok(())
    }

    fn abort_batch(&self) -> Result<(), FsError> {
        self.hit("abort_batch");
        Ok(())
    }

    fn batches_are_atomic(&self) -> bool {
        self.hit("batches_are_atomic");
        true
    }

    fn write_deflated_chunk(
        &self,
        _path: &LpPath,
        _offset: u32,
        _logical_len: u32,
        _deflated: &[u8],
    ) -> Result<(), FsError> {
        self.hit("write_deflated_chunk");
        Ok(())
    }

    fn current_version(&self) -> FsVersion {
        self.hit("current_version");
        self.files.current_version()
    }

    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsEvent> {
        self.hit("get_changes_since");
        self.files.get_changes_since(since_version)
    }

    fn get_events_since(&self, since_version: FsVersion) -> Vec<FsEvent> {
        self.hit("get_events_since");
        self.files.get_events_since(since_version)
    }

    fn clear_changes_before(&mut self, _before_version: FsVersion) {
        self.hit("clear_changes_before");
    }

    fn record_changes(&mut self, _changes: Vec<FsEvent>) {
        self.hit("record_changes");
    }
}
