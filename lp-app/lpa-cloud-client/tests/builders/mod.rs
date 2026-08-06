//! The vocabulary the scenarios are written in.
//!
//! Three nouns — a [`TestWorld`] (the world), a [`User`] (somebody at a keyboard),
//! and a [`Project`] (one copy of one project on one machine) — and every
//! verb on them goes through the crate's real operations against the real
//! in-process service. Nothing here writes a store directly, so no scenario
//! can assert about a state the application could not reach.
//!
//! Handles **re-read**: [`Project::head`], [`Project::shader`],
//! [`Project::server_heads`] and friends ask the local project or the service
//! again every time they are called. A scenario asserts by asking a handle,
//! not by comparing values it captured three lines earlier.
//!
//! Names come from counters, never from randomness: users are `user-1`,
//! `user-2`; projects are `proj-1`, `proj-2` and take their uid from the same
//! counter. Time comes from the service's own clock, one tick per recorded
//! event. Two runs of a scenario therefore produce identical hashes,
//! identical uids, and identical timestamps.

use std::cell::Cell;
use std::rc::Rc;

use lpa_cloud_client::{
    ApplyReport, ClobberReport, ClobberSide, InProcessCloud, InProcessServer, LocalProject,
    ProjectLink, PullReport, PushReport, SyncError, apply_fast_forward, block_on, call,
    open_shared, publish, pull, push, request, resolve_clobber,
};
use lpc_cloud_api::request::{AddMember, GetHeads, SetVisibility};
use lpc_cloud_api::{CloudRequest, SidecarMeta, Visibility};
use lpc_history::{
    ContentHash, EventKind, HistoryEvent, PrefixedUid, ProjectHistory, SyncRelation, UidPrefix,
};
use lpfs::{LpFs, LpFsMemory, LpPath};

/// Where the service's clock starts.
const CLOCK_START: f64 = 1_000.0;
/// How far the clock moves for each recorded event.
const TICK: f64 = 1.0;
/// The project file every project has.
const PROJECT_FILE: &str = "/project.json";
/// The file [`Project::edit`] writes and [`Project::shader`] reads.
const SHADER_FILE: &str = "/shader.glsl";
/// What a fresh project's shader says.
const FIRST_SHADER: &str = "first light";
/// The project format every sidecar declares.
const FORMAT_VERSION: u32 = 4;

/// The test world: one service, and the counters that name everything in it.
pub struct TestWorld {
    world: Rc<World>,
}

impl TestWorld {
    /// A world with an empty service at a fixed clock.
    pub fn new() -> Self {
        Self {
            world: Rc::new(World {
                server: InProcessServer::new(CLOCK_START),
                users: Cell::new(0),
                projects: Cell::new(0),
            }),
        }
    }

    /// Somebody with an account, signed in — `user-1`, then `user-2`.
    pub fn user(&self) -> User {
        invitee(&self.world).sign_in()
    }

    /// Somebody who has an email address and no account yet. Their pending
    /// memberships become access when they [`Invitee::sign_in`].
    pub fn invitee(&self) -> Invitee {
        invitee(&self.world)
    }

    /// Somebody who followed a link without signing in.
    pub fn visitor(&self) -> User {
        User {
            world: self.world.clone(),
            client: Rc::new(InProcessCloud::anonymous(self.world.server.clone())),
        }
    }
}

/// An invitation waiting for its account.
pub struct Invitee {
    world: Rc<World>,
    name: String,
    email: String,
}

impl Invitee {
    /// The address an owner grants access to.
    pub fn email(&self) -> &str {
        &self.email
    }

    /// Log in for the first time, minting the account.
    pub fn sign_in(self) -> User {
        let (client, _) = InProcessCloud::sign_in(
            self.world.server.clone(),
            &format!("sub-{}", self.name),
            &self.email,
            &self.name,
        );
        User {
            world: self.world,
            client: Rc::new(client),
        }
    }
}

/// One person's client: their session, and the projects on their machine.
pub struct User {
    world: Rc<World>,
    client: Rc<InProcessCloud>,
}

impl User {
    /// A new local project with one saved version, not on the cloud yet.
    pub fn project(&self) -> Project {
        let (name, uid) = self.world.next_project();
        let project = Project {
            world: self.world.clone(),
            client: self.client.clone(),
            slot: Slot::new(),
            uid,
            name,
        };
        LocalProject::init(
            uid,
            &project.slot.package,
            &project.slot.history,
            origin(self.world.tick()),
        )
        .expect("start the project's history");
        project.write(PROJECT_FILE, format!("{{\"name\":\"{}\"}}", project.name));
        project.write(SHADER_FILE, FIRST_SHADER);
        project.save();
        project
    }

    /// Open a share link as a tracking copy of the same project.
    pub fn open_shared(&self, link: &ProjectLink) -> Project {
        let slot = Slot::new();
        block_on(open_shared(
            &*self.client,
            &LocalProject::new(link.uid, &slot.package, &slot.history),
        ))
        .expect("open the shared project");
        Project {
            world: self.world.clone(),
            client: self.client.clone(),
            slot,
            uid: link.uid,
            name: link.slug.clone(),
        }
    }

    /// Open a share link the way a person does: by pasting the URL.
    pub fn open_url(&self, url: &str) -> Project {
        self.open_shared(&url.parse::<ProjectLink>().expect("a share URL"))
    }

    /// Why this person cannot open that link.
    pub fn open_shared_error(&self, link: &ProjectLink) -> SyncError {
        let slot = Slot::new();
        block_on(open_shared(
            &*self.client,
            &LocalProject::new(link.uid, &slot.package, &slot.history),
        ))
        .expect_err("opening should have been refused")
    }

    /// Cut this person off from the service.
    pub fn go_offline(&self) {
        self.client.go_offline();
    }

    /// Reconnect.
    pub fn go_online(&self) {
        self.client.go_online();
    }
}

/// One copy of one project, on one person's machine.
///
/// Every reader on this handle re-reads: from the working copy, from the
/// event log, or from the service.
pub struct Project {
    world: Rc<World>,
    client: Rc<InProcessCloud>,
    slot: Slot,
    uid: PrefixedUid,
    name: String,
}

impl Project {
    /// The project's uid — its identity, and its share token.
    pub fn uid(&self) -> PrefixedUid {
        self.uid
    }

    /// Change the shader and save. The note is the shader's whole content, so
    /// a scenario can say what an edit was and read it back anywhere.
    pub fn edit(&self, note: &str) {
        self.write(SHADER_FILE, note);
        self.save();
    }

    /// What the working copy's shader says right now.
    pub fn shader(&self) -> String {
        self.read(SHADER_FILE)
    }

    /// What the shader said in a banked version — including one that was set
    /// aside by a clobber.
    pub fn shader_at(&self, version: ContentHash) -> String {
        let restored = LpFsMemory::new();
        self.local()
            .snapshots()
            .materialize(&version, &restored)
            .expect("materialize the version");
        read_file(&restored, SHADER_FILE)
    }

    /// The version the local history is at.
    pub fn head(&self) -> ContentHash {
        self.local()
            .head()
            .expect("read the history")
            .expect("a saved version")
    }

    /// How this copy's history sees a version.
    pub fn relation_to(&self, version: ContentHash) -> SyncRelation {
        self.history().classify(version)
    }

    /// The cloud project this copy tracks, if any.
    pub fn bound_to(&self) -> Option<PrefixedUid> {
        self.local()
            .binding()
            .expect("read the binding")
            .map(|binding| binding.project)
    }

    /// The service's head frontier for this project.
    pub fn server_heads(&self) -> Vec<ContentHash> {
        block_on(call(&*self.client, GetHeads { uid: self.uid }))
            .expect("ask the service for its heads")
            .heads
            .into_iter()
            .map(|head| head.tree)
            .collect()
    }

    /// Put the project on the cloud and return its share link.
    pub fn publish(&self, visibility: Visibility) -> ProjectLink {
        block_on(publish(
            &*self.client,
            &self.local(),
            visibility,
            self.name.clone(),
            &self.sidecar(),
        ))
        .expect("publish the project")
        .link()
    }

    /// Change who can reach the project.
    pub fn set_visibility(&self, visibility: Visibility) {
        self.send(SetVisibility {
            uid: self.uid,
            visibility,
        });
    }

    /// Grant access by email — to an account that may not exist yet.
    pub fn add_member(&self, email: &str) {
        self.send(AddMember {
            uid: self.uid,
            email: email.to_string(),
        });
    }

    /// Somebody invited to this project and signed in: [`add_member`] plus
    /// the login that resolves it.
    ///
    /// [`add_member`]: Project::add_member
    pub fn collaborator(&self) -> User {
        let invited = invitee(&self.world);
        self.add_member(invited.email());
        invited.sign_in()
    }

    /// Send this copy's work to the service.
    pub fn push(&self) -> PushReport {
        block_on(push(&*self.client, &self.local(), &self.sidecar())).expect("push")
    }

    /// Why this copy's work did not reach the service.
    pub fn push_error(&self) -> SyncError {
        block_on(push(&*self.client, &self.local(), &self.sidecar()))
            .expect_err("the push should have been refused")
    }

    /// Ask the service what it has. Banks content; applies nothing.
    pub fn pull(&self) -> PullReport {
        block_on(pull(&*self.client, &self.local())).expect("pull")
    }

    /// Why this copy heard nothing back.
    pub fn pull_error(&self) -> SyncError {
        block_on(pull(&*self.client, &self.local())).expect_err("the pull should have been refused")
    }

    /// Adopt a pulled fast-forward into the working copy.
    pub fn fast_forward(&self, pulled: &PullReport) -> ApplyReport {
        apply_fast_forward(&self.local(), pulled).expect("fast-forward")
    }

    /// Resolve a pulled divergence by keeping one side, and push the join.
    pub fn resolve(&self, pulled: &PullReport, keep: ClobberSide) -> ClobberReport {
        let theirs = pulled
            .remote_head
            .expect("the service has a version to resolve against");
        block_on(resolve_clobber(
            &*self.client,
            &self.local(),
            theirs,
            keep,
            self.world.tick(),
            &self.sidecar(),
        ))
        .expect("resolve the divergence")
    }

    /// Start a new project from this copy's current version: new uid, fresh
    /// history, no cloud binding.
    pub fn fork(&self) -> Project {
        let (name, uid) = self.world.next_project();
        let slot = Slot::new();
        let parent = self.local();
        LocalProject::fork_from(
            &parent,
            self.head(),
            uid,
            &slot.package,
            &slot.history,
            self.world.tick(),
        )
        .expect("fork the project");
        Project {
            world: self.world.clone(),
            client: self.client.clone(),
            slot,
            uid,
            name,
        }
    }

    fn local(&self) -> LocalProject<'_> {
        LocalProject::new(self.uid, &self.slot.package, &self.slot.history)
    }

    fn history(&self) -> ProjectHistory {
        self.local().history().expect("read the history")
    }

    fn save(&self) {
        self.local().save(self.world.tick()).expect("save");
    }

    fn sidecar(&self) -> SidecarMeta {
        SidecarMeta {
            name: self.name.clone(),
            format_version: FORMAT_VERSION,
            preview_png: None,
        }
    }

    fn send(&self, what: impl Into<CloudRequest>) {
        block_on(request(&*self.client, what.into())).expect("the service accepted the request");
    }

    fn write(&self, path: &str, contents: impl AsRef<[u8]>) {
        self.slot
            .package
            .write_file(LpPath::new(path), contents.as_ref())
            .expect("write a package file");
    }

    fn read(&self, path: &str) -> String {
        read_file(&self.slot.package, path)
    }
}

/// One project's two filesystems: the working copy, and the history root.
struct Slot {
    package: LpFsMemory,
    history: LpFsMemory,
}

impl Slot {
    fn new() -> Self {
        Self {
            package: LpFsMemory::new(),
            history: LpFsMemory::new(),
        }
    }
}

/// The service, and the counters that keep every name deterministic.
struct World {
    server: Rc<InProcessServer>,
    users: Cell<u8>,
    projects: Cell<u8>,
}

impl World {
    /// The next timestamp. The service's clock is the only clock these tests
    /// have.
    fn tick(&self) -> f64 {
        self.server.advance_clock(TICK);
        self.server.now()
    }

    fn next_user(&self) -> String {
        format!("user-{}", bump(&self.users))
    }

    fn next_project(&self) -> (String, PrefixedUid) {
        let n = bump(&self.projects);
        (
            format!("proj-{n}"),
            PrefixedUid::mint(UidPrefix::Project, &[n; 16]),
        )
    }
}

fn invitee(world: &Rc<World>) -> Invitee {
    let name = world.next_user();
    Invitee {
        world: world.clone(),
        email: format!("{name}@example.com"),
        name,
    }
}

fn bump(counter: &Cell<u8>) -> u8 {
    let next = counter.get() + 1;
    counter.set(next);
    next
}

fn origin(at: f64) -> HistoryEvent {
    HistoryEvent {
        at,
        kind: EventKind::Created,
    }
}

fn read_file(fs: &LpFsMemory, path: &str) -> String {
    let bytes = fs
        .read_file(LpPath::new(path))
        .expect("read a package file");
    String::from_utf8(bytes).expect("package files in these scenarios are text")
}
