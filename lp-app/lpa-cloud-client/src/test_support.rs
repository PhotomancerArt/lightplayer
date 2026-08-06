//! Shared fixtures for this crate's unit tests.
//!
//! Deliberately small: one service, a handful of project slots, and helpers
//! that build state **through the real operations**. The full
//! fixture-builder treatment — named users, handles that re-read state,
//! scenario-shaped reading — belongs to the integration tests, which is where
//! the flagship scenarios live.

use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_cloud_api::request::{AddMember, SetVisibility};
use lpc_cloud_api::{SidecarMeta, Visibility};
use lpc_history::{ContentHash, EventKind, HistoryEvent, PrefixedUid, UidPrefix};
use lpfs::{FsError, LpFs, LpFsMemory, LpPath};

use crate::block_on::block_on;
use crate::cloud_port::call;
use crate::in_process_cloud::{InProcessCloud, InProcessServer};
use crate::local_project::LocalProject;
use crate::sync::open_shared::open_shared;

/// A service and a fixed set of project slots.
pub(crate) struct TestWorld {
    server: Rc<InProcessServer>,
    projects: Vec<TestProject>,
}

impl TestWorld {
    /// A world with a service at a fixed clock and four project slots.
    pub fn new() -> Self {
        Self {
            server: InProcessServer::new(1_000.0),
            projects: (1..=4).map(|n| TestProject::new(n as u8)).collect(),
        }
    }

    pub fn server(&self) -> &Rc<InProcessServer> {
        &self.server
    }

    /// A signed-in client. `name` seeds a deterministic identity.
    pub fn user(&self, name: &str) -> InProcessCloud {
        let (client, _) = InProcessCloud::sign_in(
            self.server.clone(),
            &alloc::format!("sub-{name}"),
            &email(name),
            name,
        );
        client
    }

    /// A client with no account.
    pub fn anonymous(&self) -> InProcessCloud {
        InProcessCloud::anonymous(self.server.clone())
    }

    /// A signed-in client that `owner` has added to `project`.
    pub fn member(
        &self,
        owner: &InProcessCloud,
        project: PrefixedUid,
        name: &str,
    ) -> InProcessCloud {
        let client = self.user(name);
        block_on(call(
            owner,
            AddMember {
                uid: project,
                email: email(name),
            },
        ))
        .expect("add member");
        client
    }

    pub fn set_visibility(
        &self,
        owner: &InProcessCloud,
        project: PrefixedUid,
        visibility: Visibility,
    ) {
        block_on(call(
            owner,
            SetVisibility {
                uid: project,
                visibility,
            },
        ))
        .expect("set visibility");
    }

    /// Project slot `n` (1-based).
    pub fn project(&self, n: usize) -> &TestProject {
        &self.projects[n - 1]
    }

    /// Open `project` as a fresh tracking copy in slot `n`.
    pub fn tracking_copy(
        &self,
        client: &InProcessCloud,
        n: usize,
        project: PrefixedUid,
    ) -> &TestProject {
        let slot = self.project(n);
        block_on(open_shared(client, &slot.local_as(project))).expect("open shared");
        slot
    }
}

/// One project's two filesystems, owned so a `LocalProject` can borrow them.
pub(crate) struct TestProject {
    uid: PrefixedUid,
    package: LpFsMemory,
    history: LpFsMemory,
}

impl TestProject {
    pub fn new(n: u8) -> Self {
        Self {
            uid: PrefixedUid::mint(UidPrefix::Project, &[n; 16]),
            package: LpFsMemory::new(),
            history: LpFsMemory::new(),
        }
    }

    pub fn uid(&self) -> PrefixedUid {
        self.uid
    }

    pub fn package(&self) -> &LpFsMemory {
        &self.package
    }

    pub fn history(&self) -> &LpFsMemory {
        &self.history
    }

    /// A view of this project, unstarted.
    pub fn local(&self) -> LocalProject<'_> {
        LocalProject::new(self.uid, &self.package, &self.history)
    }

    /// A view of this project under somebody else's uid — what a tracking
    /// copy is, identity preserved.
    pub fn local_as(&self, uid: PrefixedUid) -> LocalProject<'_> {
        LocalProject::new(uid, &self.package, &self.history)
    }

    /// Start this project's history with a `Created` origin.
    pub fn init(&self) -> LocalProject<'_> {
        LocalProject::init(self.uid, &self.package, &self.history, created(1.0))
            .expect("init project")
    }

    pub fn write(&self, path: &str, bytes: &[u8]) {
        self.package
            .write_file(LpPath::new(path), bytes)
            .expect("write package file");
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self.package.read_file(LpPath::new(path)) {
            Ok(bytes) => Some(bytes),
            Err(FsError::NotFound(_)) => None,
            Err(e) => panic!("read package file: {e}"),
        }
    }

    pub fn delete(&self, path: &str) {
        self.package
            .delete_file(LpPath::new(path))
            .expect("delete package file");
    }
}

/// A `Created` origin event.
pub(crate) fn created(at: f64) -> HistoryEvent {
    HistoryEvent {
        at,
        kind: EventKind::Created,
    }
}

/// Display metadata for a push.
pub(crate) fn sidecar(name: &str) -> SidecarMeta {
    SidecarMeta {
        name: name.to_string(),
        format_version: 4,
        preview_png: None,
    }
}

pub(crate) fn hash(bytes: &[u8]) -> ContentHash {
    ContentHash::of(bytes)
}

fn email(name: &str) -> String {
    alloc::format!("{name}@example.com")
}
