//! End-to-end transient-open tests (examples vision D2 / plan PD1): an
//! embedded example opens as a memory-backed view session through the
//! ordinary funnel — full editor, save/dirty/history — while the library
//! sees **nothing**: no catalog transaction, no package, no lock, no
//! saved-broadcast (the cloud sync engine's trigger).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use lpc_model::LpValue;
use lpfs::LpFsMemory;

use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
use crate::app::studio::studio_edit_e2e_tests::{
    InProcessServerIo, drive, edit_e2e_files, edit_e2e_server, editor_dirty, find_slot,
    project_action, set_value_action,
};
use crate::{
    ControllerId, HOME_NODE_ID, HomeOp, ProjectOp, StudioActor, StudioCommand, StudioController,
    StudioServerClient, UiAction,
};

/// All files under a handle's store root (history payloads included) —
/// the fake "fetched share" for the shared-view rows.
fn store_files(fs: &std::rc::Rc<RefCell<dyn lpfs::LpFs>>) -> Vec<(String, Vec<u8>)> {
    use lpc_model::AsLpPath;
    let view = fs.borrow();
    let mut files = Vec::new();
    for entry in view.list_dir("/".as_path(), true).unwrap_or_default() {
        if view.is_dir(entry.as_path()).unwrap_or(false) {
            continue;
        }
        files.push((
            entry.as_str().trim_start_matches('/').to_string(),
            view.read_file(entry.as_path()).expect("read"),
        ));
    }
    files
}

/// The example every row opens: the demo project, known to load on the
/// host `LpServer` (the docs e2e rows deploy the same files).
const EXAMPLE: &str = crate::STUDIO_DEMO_PROJECT_ID;

fn open_example(id: &str) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenExample { id: id.to_string() },
    ))
}

fn open_package(key: &str) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage {
            key: key.to_string(),
        },
    ))
}

fn empty_store() -> LibraryStore {
    // Per-draw entropy: the transient open mints its uid from the same
    // injected source installs use, and identical seeds would collide.
    let seed = RefCell::new(0u8);
    LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(move || {
            *seed.borrow_mut() += 1;
            [*seed.borrow(); 16]
        }),
        Rc::new(|| "2026-08-28-1600".to_string()),
    )
}

fn controller_over(store: LibraryStore) -> (StudioController, Rc<MemoryLibraryHost>) {
    crate::app::open_progress::reset_for_test();
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server,
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);
    let host = Rc::new(MemoryLibraryHost::new(store, Rc::new(|| 5.0)));
    controller.attach_library(host.clone());
    (controller, host)
}

#[test]
fn opening_an_example_transiently_installs_nothing() {
    let store = empty_store();
    let (controller, host) = controller_over(store.clone());
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle.tx.send(open_example(EXAMPLE));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("open emits a snapshot");

    assert!(snapshot.home.is_none(), "the editor is open");
    assert!(snapshot.open_project_transient, "the session is transient");
    assert_eq!(
        snapshot.open_transient_example.as_deref(),
        Some(EXAMPLE),
        "the view marks the session with its origin example"
    );
    assert!(
        snapshot.open_project_uid.is_some(),
        "the session has (ephemeral) identity for the handle machinery"
    );
    assert_eq!(
        snapshot.open_project_name.as_deref(),
        Some("Fyeah Sign"),
        "the display name comes from the example manifest"
    );

    // D2: the library gained NOTHING.
    assert_eq!(store.list().expect("list").len(), 0, "no package installed");
    assert!(
        host.saved_notifications().is_empty(),
        "no saved-broadcast (the sync engine must never see a transient uid)"
    );
    assert!(host.closed_projects().is_empty(), "no lock was ever taken");
    assert!(host.abandoned_projects().is_empty(), "nothing to abandon");
}

#[test]
fn explicit_save_forks_the_transient_session_into_the_library() {
    let store = empty_store();
    let (controller, host) = controller_over(store.clone());
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle.tx.send(open_example(EXAMPLE));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("open emits a snapshot");
    let session_uid = snapshot
        .open_project_uid
        .clone()
        .expect("the transient session has identity");

    // one persisted edit → dirty; the explicit save is the fork moment
    let color_order = find_slot(&snapshot, "color_order");
    let address = color_order.address.clone().expect("addressed slot");
    handle.tx.send(set_value_action(
        address,
        LpValue::String("bgr".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edit emits a snapshot");
    assert_eq!(editor_dirty(&snapshot), (1, 0), "the edit is unsaved work");
    assert_eq!(
        store.list().expect("list").len(),
        0,
        "play installs nothing"
    );

    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv().expect("save emits a snapshot");
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    assert_eq!(editor_dirty(&snapshot), (0, 0), "the save landed");

    // D7: the session is no longer transient, same identity, no reload.
    assert!(
        !snapshot.open_project_transient,
        "the explicit save forked: the session is an ordinary one now"
    );
    assert_eq!(snapshot.transient_fork_generation, 1, "one fork completed");
    assert_eq!(
        snapshot.open_project_uid.as_deref(),
        Some(session_uid.as_str()),
        "the fork PROMOTES the session uid — identity survives the install"
    );

    // The library holds exactly the fork: session uid, SeededFrom
    // provenance, the saved content, and a non-empty history.
    let installed = store.list().expect("list");
    assert_eq!(installed.len(), 1, "exactly one package installed");
    assert_eq!(installed[0].uid.to_string(), session_uid);
    assert_eq!(installed[0].name, "Fyeah Sign");
    let handle_installed = store.open(installed[0].uid).expect("installed opens");
    let fixture: String = String::from_utf8(
        handle_installed
            .package_fs
            .borrow()
            .read_file(lpc_model::LpPath::new("/fixture.json"))
            .expect("library fixture.json"),
    )
    .expect("utf8")
    .chars()
    .filter(|c| !c.is_whitespace())
    .collect();
    assert!(
        fixture.contains(r#""color_order":"bgr""#),
        "the library copy carries the committed edit; got: {fixture}"
    );
    assert!(
        !handle_installed.history.events().is_empty(),
        "the history installed verbatim"
    );
    let meta = crate::app::library::package_meta::read_meta(&*handle_installed.package_fs.borrow())
        .expect("meta reads")
        .expect("meta exists");
    assert_eq!(
        meta.provenance,
        PackageProvenance::SeededFrom {
            source: EXAMPLE.to_string()
        },
        "the fork records where it came from (the 'Remixed from' line)"
    );

    // A SECOND save flows to the library copy — the session really did
    // become an ordinary one (and now broadcasts saves).
    let color_order = find_slot(&snapshot, "color_order");
    let address = color_order.address.clone().expect("addressed slot");
    handle.tx.send(set_value_action(
        address,
        LpValue::String("grb".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv().expect("second edit emits a snapshot");
    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv().expect("second save emits a snapshot");

    let handle_installed = store.open(installed[0].uid).expect("installed reopens");
    let fixture = String::from_utf8(
        handle_installed
            .package_fs
            .borrow()
            .read_file(lpc_model::LpPath::new("/fixture.json"))
            .expect("library fixture.json"),
    )
    .expect("utf8");
    assert!(
        fixture
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .contains(r#""color_order":"grb""#),
        "the second save pulls into the LIBRARY copy; got: {fixture}"
    );
    assert_eq!(
        host.saved_notifications(),
        vec![session_uid.clone()],
        "post-fork saves broadcast like any ordinary save"
    );
}

#[test]
fn replacing_a_transient_open_queues_no_close() {
    let store = empty_store();
    let files: Vec<(String, Vec<u8>)> = edit_e2e_files()
        .iter()
        .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
        .collect();
    let package = store
        .install_package("Porch sign", &files, PackageProvenance::Created, 1.0)
        .expect("install library package");
    let (controller, host) = controller_over(store.clone());
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle.tx.send(open_example(EXAMPLE));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("example open emits a snapshot");
    assert!(snapshot.open_project_transient);

    handle.tx.send(open_package(&package.uid.to_string()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("package open emits a snapshot");
    assert_eq!(
        snapshot.open_project_uid.as_deref(),
        Some(package.uid.to_string().as_str()),
        "the library package is open"
    );
    assert!(
        !snapshot.open_project_transient,
        "an ordinary open is not transient"
    );
    assert!(
        host.closed_projects().is_empty(),
        "the transient session held no host lock, so none is released: {:?}",
        host.closed_projects()
    );
}

#[test]
fn a_shared_view_link_opens_transiently_and_forks_a_fresh_identity() {
    use crate::app::library::package_meta;

    // The "cloud parent": a real package in a SEPARATE store (its own
    // seed range — two counter-seeded stores starting at 1 would mint
    // colliding uids), read back as the bytes the web edge would have
    // fetched from the service.
    let parent_store = {
        let seed = RefCell::new(100u8);
        LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || {
                *seed.borrow_mut() += 1;
                [*seed.borrow(); 16]
            }),
            Rc::new(|| "2026-08-27-0900".to_string()),
        )
    };
    let files: Vec<(String, Vec<u8>)> = edit_e2e_files()
        .iter()
        .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
        .collect();
    let parent = parent_store
        .install_package("Dome Nights", &files, PackageProvenance::Created, 1.0)
        .expect("install parent");
    let parent_handle = parent_store.open(parent.uid).expect("open parent");
    let package_files = store_files(&parent_handle.package_fs);
    let history_files = store_files(&parent_handle.history_fs);

    // The visitor's browser: an EMPTY library.
    let store = empty_store();
    let (controller, host) = controller_over(store.clone());
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenSharedTransient {
            uid: parent.uid.to_string(),
            name: "Dome Nights".to_string(),
            package_files,
            history_files,
        },
    )));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("shared open emits a snapshot");

    // D1/D2: a View link runs like an example — transiently, nothing
    // installed, under the cloud document's OWN uid.
    assert!(snapshot.home.is_none(), "the editor is open");
    assert!(snapshot.open_project_transient, "the session is transient");
    assert_eq!(
        snapshot.open_transient_example, None,
        "a shared view is not an example — its Project route is honest"
    );
    assert_eq!(
        snapshot.open_project_uid.as_deref(),
        Some(parent.uid.to_string().as_str()),
        "the session runs the parent's uid (D17)"
    );
    assert_eq!(store.list().expect("list").len(), 0, "nothing installed");

    // Edit + explicit save: the fork mints a FRESH identity — the parent's
    // uid stays the parent's — with ForkedFrom provenance.
    let color_order = find_slot(&snapshot, "color_order");
    let address = color_order.address.clone().expect("addressed slot");
    handle.tx.send(set_value_action(
        address,
        LpValue::String("bgr".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv().expect("edit emits a snapshot");
    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("save emits a snapshot");

    assert!(
        !snapshot.open_project_transient,
        "the explicit save forked the view session"
    );
    assert_eq!(snapshot.transient_fork_generation, 1, "one fork completed");
    let installed = store.list().expect("list");
    assert_eq!(installed.len(), 1, "exactly one fork installed");
    assert_ne!(
        installed[0].uid, parent.uid,
        "the fork must NOT claim the parent's identity"
    );
    assert_eq!(
        snapshot.open_project_uid.as_deref(),
        Some(installed[0].uid.to_string().as_str()),
        "the session follows the fork's fresh identity"
    );
    let fork_handle = store.open(installed[0].uid).expect("fork opens");
    let meta = package_meta::read_meta(&*fork_handle.package_fs.borrow())
        .expect("meta reads")
        .expect("meta exists");
    match meta.provenance {
        PackageProvenance::ForkedFrom { parent_project, .. } => {
            assert_eq!(parent_project, parent.uid.to_string())
        }
        other => panic!("fork provenance must be ForkedFrom, got {other:?}"),
    }
    let fixture: String = String::from_utf8(
        fork_handle
            .package_fs
            .borrow()
            .read_file(lpc_model::LpPath::new("/fixture.json"))
            .expect("fork fixture.json"),
    )
    .expect("utf8")
    .chars()
    .filter(|c| !c.is_whitespace())
    .collect();
    assert!(
        fixture.contains(r#""color_order":"bgr""#),
        "the fork carries the saved edit; got: {fixture}"
    );
    assert!(host.closed_projects().is_empty(), "no stray lock releases");
}
