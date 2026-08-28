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
    assert_eq!(
        snapshot.open_project_transient.as_deref(),
        Some(EXAMPLE),
        "the view marks the session transient with its origin example"
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
fn transient_save_lands_in_memory_not_the_library() {
    let store = empty_store();
    let (controller, host) = controller_over(store.clone());
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle.tx.send(open_example(EXAMPLE));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("open emits a snapshot");

    // one persisted edit → dirty; explicit save → clean, in memory only
    let color_order = find_slot(&snapshot, "color_order");
    let address = color_order.address.clone().expect("addressed slot");
    handle.tx.send(set_value_action(
        address,
        LpValue::String("bgr".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edit emits a snapshot");
    assert_eq!(editor_dirty(&snapshot), (1, 0), "the edit is unsaved work");

    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv().expect("save emits a snapshot");
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    assert_eq!(editor_dirty(&snapshot), (0, 0), "the save landed");
    assert_eq!(
        snapshot.open_project_transient.as_deref(),
        Some(EXAMPLE),
        "a save alone does not fork: the session stays transient (P3 owns the fork)"
    );

    // save-as-pull wrote the MEMORY copy…
    let fixture = actor
        .controller_mut_for_test()
        .project_for_test()
        .active_package_file_for_test("/fixture.json")
        .expect("active package fixture.json");
    let fixture: String = String::from_utf8(fixture)
        .expect("utf8")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert!(
        fixture.contains(r#""color_order":"bgr""#),
        "the memory copy carries the committed edit; got: {fixture}"
    );
    // …and the library STILL gained nothing.
    assert_eq!(store.list().expect("list").len(), 0, "no package installed");
    assert!(
        host.saved_notifications().is_empty(),
        "a transient save must not broadcast"
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
    assert_eq!(snapshot.open_project_transient.as_deref(), Some(EXAMPLE));

    handle.tx.send(open_package(&package.uid.to_string()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("package open emits a snapshot");
    assert_eq!(
        snapshot.open_project_uid.as_deref(),
        Some(package.uid.to_string().as_str()),
        "the library package is open"
    );
    assert_eq!(
        snapshot.open_project_transient, None,
        "an ordinary open is not transient"
    );
    assert!(
        host.closed_projects().is_empty(),
        "the transient session held no host lock, so none is released: {:?}",
        host.closed_projects()
    );
}
