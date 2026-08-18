//! End-to-end supersede tests (D4): what happens to an open when a newer
//! click lands on top of it.
//!
//! Two shapes, because the two arrive by different routes:
//!
//! - **Same batch.** Two clicks land before the actor wakes. Nothing is
//!   parked yet, so the generation counter alone cannot tell them apart —
//!   the queue's own coalescing is what makes the newest win.
//! - **Mid-open.** The click lands while the open is parked inside the
//!   actor's serial action loop, which is where the real demo failure
//!   lived. `SupersedingHost` reproduces it exactly: the newer click
//!   arrives *while `open_project` is awaiting*, so the open comes back
//!   from the host holding a fresh lock it must give straight back.
//!
//! The second is the one with teeth. Before P1's receipt and this
//! phase's post-lock boundary, that open kept the project lock for the
//! page's lifetime and every later open of the same project was refused
//! with "open in another tab" — with one tab open.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use futures_util::future::LocalBoxFuture;
use lpfs::{LpFs, LpFsMemory};

use crate::app::library::{
    CatalogOp, CatalogOutcome, LibraryHost, LibraryHostError, LibraryStore, MemoryLibraryHost,
    OpenedProject, PackageProvenance, PackageSummary,
};
use crate::app::studio::studio_edit_e2e_tests::{
    InProcessServerIo, drive, edit_e2e_files, edit_e2e_server,
};
use crate::{
    ControllerId, HOME_NODE_ID, HomeOp, StudioActor, StudioCommand, StudioController,
    StudioServerClient, UiAction,
};

/// The shared edit-harness project: the graph the in-process server is
/// built around, so an open that lands really lands.
fn project_files() -> Vec<(String, Vec<u8>)> {
    edit_e2e_files()
        .iter()
        .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
        .collect()
}

fn open(key: &str) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage {
            key: key.to_string(),
        },
    ))
}

/// A store with two installed projects, plus their summaries.
fn two_project_store() -> (LibraryStore, PackageSummary, PackageSummary) {
    let seed = RefCell::new(0u8);
    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(move || {
            *seed.borrow_mut() += 1;
            [*seed.borrow(); 16]
        }),
        Rc::new(|| "2026-08-14-1859".to_string()),
    );
    let first = store
        .install_package("First", &project_files(), PackageProvenance::Created, 1.0)
        .expect("install the first project");
    let second = store
        .install_package("Second", &project_files(), PackageProvenance::Created, 2.0)
        .expect("install the second project");
    (store, first, second)
}

fn controller_over(host: Rc<dyn LibraryHost>) -> StudioController {
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server,
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);
    controller.attach_library(host);
    controller
}

#[test]
fn two_clicks_in_one_batch_open_only_the_newest() {
    crate::app::open_progress::reset_for_test();
    let (store, first, second) = two_project_store();
    let host = Rc::new(MemoryLibraryHost::new(store.clone(), Rc::new(|| 3.0)));
    let controller = controller_over(host.clone());
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));

    handle.tx.send(open(&first.uid.to_string()));
    handle.tx.send(open(&second.uid.to_string()));
    drive(actor.run_one_batch_for_test());

    let view = actor.controller_mut_for_test().view();
    assert_eq!(
        view.open_project_uid.as_deref(),
        Some(second.uid.to_string().as_str()),
        "the LAST click is the one that ends up open"
    );
    assert!(
        host.abandoned_projects().is_empty(),
        "the superseded open never got as far as taking a lock: {:?}",
        host.abandoned_projects()
    );
    assert_eq!(
        crate::app::open_progress::open_stage(),
        crate::app::open_progress::OpenStage::Idle,
        "a landed open leaves no failure behind"
    );
}

/// A host whose `open_project` lets a newer click land while it awaits —
/// the parked-open window, reproduced.
struct SupersedingHost {
    inner: MemoryLibraryHost,
    /// Uids whose open should be superseded from underneath, once each.
    supersede: RefCell<Vec<String>>,
}

impl SupersedingHost {
    fn new(inner: MemoryLibraryHost, supersede: Vec<String>) -> Self {
        Self {
            inner,
            supersede: RefCell::new(supersede),
        }
    }
}

impl LibraryHost for SupersedingHost {
    fn catalog_snapshot(
        &self,
    ) -> LocalBoxFuture<'_, Result<Rc<RefCell<dyn LpFs>>, LibraryHostError>> {
        self.inner.catalog_snapshot()
    }

    fn catalog(
        &self,
        op: CatalogOp,
    ) -> LocalBoxFuture<'_, Result<CatalogOutcome, LibraryHostError>> {
        self.inner.catalog(op)
    }

    fn open_project<'a>(
        &'a self,
        key: &'a str,
    ) -> LocalBoxFuture<'a, Result<OpenedProject, LibraryHostError>> {
        Box::pin(async move {
            let opened = self.inner.open_project(key).await;
            // The lock (here, the receipt) is in hand — and *now* the user
            // clicks somewhere else. Exactly the window the post-lock
            // boundary exists for.
            let mut supersede = self.supersede.borrow_mut();
            if let Some(index) = supersede.iter().position(|uid| {
                opened
                    .as_ref()
                    .is_ok_and(|opened| opened.uid.to_string() == *uid || uid == key)
            }) {
                supersede.remove(index);
                crate::app::open_progress::note_open_requested();
            }
            opened
        })
    }

    fn close_project<'a>(&'a self, uid: &'a str) -> LocalBoxFuture<'a, ()> {
        self.inner.close_project(uid)
    }

    fn open_elsewhere_uids(&self) -> LocalBoxFuture<'_, Vec<String>> {
        self.inner.open_elsewhere_uids()
    }

    fn notify_saved(&self, uid: &str) {
        self.inner.notify_saved(uid);
    }
}

#[test]
fn an_open_superseded_after_it_took_the_lock_gives_the_project_straight_back() {
    crate::app::open_progress::reset_for_test();
    let (store, first, second) = two_project_store();
    let first_uid = first.uid.to_string();
    let inner = MemoryLibraryHost::new(store.clone(), Rc::new(|| 3.0));
    let abandoned_probe = Rc::new(SupersedingHost::new(inner, vec![first_uid.clone()]));
    let controller = controller_over(abandoned_probe.clone());
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));

    // One batch each, so the first open really is parked when the second
    // click's generation bump lands (from inside `open_project`).
    handle.tx.send(open(&first_uid));
    drive(actor.run_one_batch_for_test());

    assert_eq!(
        abandoned_probe.inner.abandoned_projects(),
        vec![first_uid.clone()],
        "the superseded open must give the project (and its lock) back"
    );
    let view = actor.controller_mut_for_test().view();
    assert_eq!(
        view.open_project_uid, None,
        "a superseded open must not leave its project half-open"
    );
    assert!(
        !matches!(
            crate::app::open_progress::open_stage(),
            crate::app::open_progress::OpenStage::Failed(_)
        ),
        "a supersede is not a failure: no error, no Retry, nothing to read"
    );

    // …and the project it gave back opens cleanly on the next click, which
    // is the whole point: the old leak refused every later open of it.
    handle.tx.send(open(&second.uid.to_string()));
    drive(actor.run_one_batch_for_test());
    let view = actor.controller_mut_for_test().view();
    assert_eq!(
        view.open_project_uid.as_deref(),
        Some(second.uid.to_string().as_str())
    );

    handle.tx.send(open(&first_uid));
    drive(actor.run_one_batch_for_test());
    let view = actor.controller_mut_for_test().view();
    assert_eq!(
        view.open_project_uid.as_deref(),
        Some(first_uid.as_str()),
        "the once-superseded project opens like any other"
    );
}
