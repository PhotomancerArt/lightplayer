//! End-to-end edit flow against an in-process LightPlayer server.
//!
//! Harness-level, no UI: a real `LpServer` (simulator session) runs behind a
//! `ClientIo` adapter that pumps every client message through
//! `LpServer::tick_and_send`. The studio actor drives the same command path
//! the web shell uses: connect → `SetValue` on a clock control (transient)
//! and a fixture slot (persisted) → observe DTO dirty states → `SaveOverlay`
//! (def file on disk gains only the persisted edit) → `RevertAllEdits`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::{Pin, pin};
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_client::ClientIo;
use lpa_server::{LpGraphics, LpServer};
use lpc_model::{AsLpPath, LpValue, SlotPath};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::ServerTransport;
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireMessage, WireProjectCommand,
    WireServerMessage,
};
use lpfs::LpFsMemory;

use crate::{
    ControllerId, ProjectController, ProjectOp, SlotEditOp, StudioActor, StudioCommand,
    StudioController, StudioServerClient, UiAction, UiConfigSlot, UiConfigSlotBody,
    UiNodeDirtyState, UiNodeSection, UiNodeTabBody, UiNodeView, UiSlotEditorHint, UiStudioView,
    UiViewContent,
};

#[test]
fn simulator_session_edit_save_and_revert_end_to_end() {
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    // Connect the running project through the real client path so the
    // inventory read installs the node → def-artifact map.
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    // Root card restored over the real wire: the project root is the ONE
    // top-level card and the clock and fixture ride its children.
    // Post-mitosis the root module def carries ONLY `nodes` (role `Fixed`);
    // format/uid/name live in the project.json container manifest, so they
    // must NOT surface as root slots.
    let editor = project_editor(&snapshot);
    assert_eq!(editor.nodes.len(), 1, "one top card: the root module");
    assert_eq!(
        editor.nodes[0].children.len(),
        2,
        "clock and fixture ride the root card"
    );
    let root_slot = |path: &str| {
        editor.root_slots.iter().find(|slot| {
            slot.address
                .as_ref()
                .is_some_and(|address| address.path.to_string() == path)
        })
    };
    assert!(!root_slot("nodes").expect("nodes root slot").state.editable);
    assert!(
        root_slot("format").is_none() && root_slot("name").is_none(),
        "container-manifest fields must not surface as root slots"
    );

    // P5 (clock-tape-hero): the tape face CLAIMED the clock's three
    // `transport.*` Debug rows — no Debug section renders on a clock at
    // all, and the face's transport block is the one read/dispatch
    // surface. (D4 flattening itself stays covered by the partition's own
    // tests and the output node's `test_pattern`.)
    let clock_sections = node_sections(&snapshot, "/edit_e2e.show/clock.clock");
    assert!(
        !clock_sections
            .iter()
            .any(|section| matches!(section, UiNodeSection::DebugSlots(_))),
        "the tape face claimed the transport rows — a clock renders no Debug drawer"
    );
    let settings_labels = section_slot_labels(&clock_sections, |section| {
        matches!(section, UiNodeSection::ConfigSlots(_))
    });
    assert!(
        !settings_labels.iter().any(|label| label == "Controls"),
        "the face replaces the old `controls` record row, never duplicates it: {settings_labels:?}"
    );

    let transport = clock_transport_block(&snapshot);
    assert_eq!(transport.rate_override, None, "clean transport — no tint");
    let rate_address = transport
        .rate_address
        .clone()
        .expect("transport block carries the rate dispatch address");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert!(!color_order.state.debug, "color order is a persisted slot");
    let color_order_address = color_order
        .address
        .clone()
        .expect("color order slot carries an address");
    assert_eq!(editor_dirty(&snapshot), (0, 0));

    // An oninput flood on the clock rate plus one persisted edit, queued into
    // one actor batch: the flood coalesces to a single mutation per address.
    let mutations_before = count_mutations(&sent);
    for value in [1.2_f32, 1.6, 2.0] {
        handle
            .tx
            .send(set_value_action(rate_address.clone(), LpValue::F32(value)));
    }
    handle.tx.send(set_value_action(
        color_order_address.clone(),
        LpValue::String("rgb".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edits emit a snapshot");

    assert_eq!(
        count_mutations(&sent) - mutations_before,
        2,
        "three queued rate SetValues coalesce with the color-order edit into two mutations"
    );
    let transport = clock_transport_block(&snapshot);
    assert!(
        transport.rate_override.is_some(),
        "the active override lifts its Clear target (the tape's changed tint)"
    );
    assert_eq!(transport.rate, 2.0);
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Dirty);
    assert!(!color_order.state.debug);
    assert_eq!(slot_value_display(color_order), "rgb");
    assert_eq!(
        editor_dirty(&snapshot),
        (1, 0),
        "only the persisted slot is dirty; the debug rate override is not (D7)"
    );

    // Save: the persisted color-order edit commits to fixture.json; the
    // debug rate override stays pending (live), clock.json untouched.
    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    // Pull a refresh so the synced view reflects the committed def.
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("save + refresh emit a snapshot");

    let fixture_json = read_project_file(&server, "fixture.json");
    assert!(
        fixture_json.contains("\"color_order\":\"rgb\""),
        "fixture.json gained the persisted color-order edit: {fixture_json}"
    );
    let clock_json = read_project_file(&server, "clock.json");
    assert!(
        !clock_json.contains("\"rate\":2"),
        "clock.json must not gain the debug rate override: {clock_json}"
    );
    let transport = clock_transport_block(&snapshot);
    assert!(
        transport.rate_override.is_some(),
        "the debug override survives the save, live on the project"
    );
    assert_eq!(transport.rate, 2.0);
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(color_order),
        "rgb",
        "committed value synced back"
    );
    assert_eq!(
        editor_dirty(&snapshot),
        (0, 0),
        "with the persisted edit written the project reads clean — the surviving debug override is not dirty"
    );

    // Revert all: the overlay clears, every slot returns to Clean, and the
    // *gated* refresh (since = last known revision) delivers the reverted
    // def values directly — no reconnect/full resync. Reverting advances the
    // effective def revisions monotonically (studio editing ADR follow-up
    // (e)), so the delta read includes the reverted roots.
    handle.tx.send(project_action(ProjectOp::RevertAllEdits));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert emits a snapshot");

    let transport = clock_transport_block(&snapshot);
    assert_eq!(transport.rate_override, None);
    assert_eq!(
        transport.rate, 1.0,
        "rate reverted to the authored value through the gated refresh"
    );
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(color_order),
        "rgb",
        "revert does not undo committed file changes"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0));
}

/// Row P3-b (detach quiesce): an edit and the lens detach queued into the
/// SAME actor batch. The actor's serialized dispatch IS the quiesce — the
/// edit is fully awaited (its mutation reaches the wire, acked) before
/// the detach drops the mirror — and a re-attach rebuilds the mirror over
/// the server-side overlay with the value intact: nothing lost.
#[test]
fn detach_with_an_edit_in_flight_quiesces_and_loses_nothing() {
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let rate_address = clock_transport_block(&snapshot)
        .rate_address
        .clone()
        .expect("transport block carries the rate dispatch address");

    // One batch: the edit is queued (in flight) when the detach lands
    // behind it.
    let mutations_before = count_mutations(&sent);
    handle
        .tx
        .send(set_value_action(rate_address, LpValue::F32(2.0)));
    handle.tx.send(project_action(ProjectOp::DetachLens));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the batch emits a snapshot");

    assert!(
        snapshot.home.is_some(),
        "the detach landed: the gallery shows"
    );
    assert_eq!(
        count_mutations(&sent) - mutations_before,
        1,
        "the queued edit reached the wire (acked) BEFORE the mirror dropped"
    );

    // Nothing lost: re-attach on the surviving session rebuilds the
    // mirror over the server-side overlay.
    let sim_id = actor
        .controller_mut_for_test()
        .runtime_pool_for_test()
        .sim_session()
        .expect("the sim session survives the detach")
        .id();
    drive(
        actor
            .controller_mut_for_test()
            .attach_lens(sim_id, crate::UxUpdateSink::noop()),
    )
    .expect("re-attach connects");
    let rebuilt = actor.controller_mut_for_test().view();
    assert_eq!(
        clock_transport_block(&rebuilt).rate,
        2.0,
        "the acked edit is visible after detach → re-attach"
    );
}

#[test]
fn home_open_package_pushes_the_library_head_end_to_end() {
    use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
    use crate::{HOME_NODE_ID, HomeOp};

    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);

    // a library with one package holding the same node graph the harness
    // uses; install mints the uid into the manifest
    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [7u8; 16]),
        Rc::new(|| "2026-07-09-1421".to_string()),
    );
    let summary = store
        .install_package(
            "Porch sign",
            &edit_e2e_files()
                .iter()
                .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
            PackageProvenance::Created,
            1.0,
        )
        .expect("install library package");
    let host = Rc::new(MemoryLibraryHost::new(store.clone(), Rc::new(|| 1.0)));
    controller.attach_library(host.clone());

    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage {
            key: summary.uid.to_string(),
        },
    )));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("open emits a snapshot");
    assert!(
        host.abandoned_projects().is_empty(),
        "an open that finished commits its receipt: the close path owns the \
         project's lock from here"
    );

    // the open replaced the running project with the library head: home is
    // gone, the editor shows, and the runtime's manifest carries the
    // package's minted uid (the push is hash-verified inside the open)
    assert!(snapshot.home.is_none(), "an open project leaves home");
    let editor = project_editor(&snapshot);
    assert_eq!(
        editor.nodes[0].children.len(),
        2,
        "clock and fixture panes under the root card"
    );
    let pushed_manifest = {
        let bytes = server
            .borrow()
            .base_fs()
            .read_file("/projects/studio/project.json".as_path())
            .expect("pushed manifest exists in the runtime");
        String::from_utf8(bytes).expect("utf8 manifest")
    };
    assert!(
        pushed_manifest.contains(&summary.uid.to_string()),
        "the runtime holds the library copy (uid pushed): {pushed_manifest}"
    );
}

/// An open must NARRATE while it runs, not only when it lands — the open
/// twin of `a_flash_narrates_its_progress_while_it_runs`
/// (2026-07-28-flash-progress-never-reached-the-ui, same mechanism class).
///
/// The card's whole opening treatment (the dim, the pipeline line that
/// shows the engine download) rides `home.opening`, which reaches the DOM
/// only inside a published view — and the actor is parked inside the open
/// for its whole duration. The dispatch wrapper's two snapshots bracket
/// the action: before it, `pending_open` is not set yet; after it, the
/// open is already over. Unless `open_on_simulator` emits a view of its
/// own after setting `pending_open`, a slow open runs to completion
/// behind a gallery that never acknowledged the click (the live G1 Q1
/// residual: a throttled first click showed no "Downloading the engine…"
/// line, because the line's mount gate never arrived).
#[test]
fn an_open_narrates_on_the_card_while_it_runs() {
    use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
    use crate::{HOME_NODE_ID, HomeOp, UxUpdate, UxUpdateSink};

    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    // A bare studio with a stub sim — NOT `connected_with_client_for_test`,
    // whose already-loaded project hides the home gallery this test is
    // entirely about.
    let mut controller = StudioController::new(|| 1.0);
    controller.install_stub_sim_with_client_for_test(client);

    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [9u8; 16]),
        Rc::new(|| "2026-08-18-1200".to_string()),
    );
    let summary = store
        .install_package(
            "Porch sign",
            &edit_e2e_files()
                .iter()
                .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
            PackageProvenance::Created,
            1.0,
        )
        .expect("install library package");
    controller.attach_library(Rc::new(MemoryLibraryHost::new(store, Rc::new(|| 1.0))));

    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = UxUpdateSink::new({
        let seen = Rc::clone(&seen);
        move |update| seen.borrow_mut().push(update)
    });
    drive(controller.dispatch_with_updates(
        UiAction::from_op(
            ControllerId::new(HOME_NODE_ID),
            HomeOp::OpenPackage {
                key: summary.uid.to_string(),
            },
        ),
        sink,
    ))
    .expect("the open lands");

    let seen = seen.borrow();
    let openings: Vec<Option<String>> = seen
        .iter()
        .filter_map(|update| match update {
            UxUpdate::View(view) => Some(view.home.as_ref().and_then(|home| home.opening.clone())),
            _ => None,
        })
        .collect();
    assert!(
        openings
            .iter()
            .any(|opening| opening.as_deref() == Some(summary.uid.to_string().as_str())),
        "some view published DURING the open must carry home.opening so the \
         card can mount its pipeline line; saw {openings:?}"
    );
    assert_eq!(
        openings.last().and_then(|opening| opening.as_deref()),
        None,
        "the final view clears the opening treatment"
    );
}

/// A failed open gives the project straight back to the library (P1).
///
/// The failure staged here is the cheapest one to reach — a below-floor
/// package the migrator refuses — but the shape is the one that ruined
/// demos: the host has already taken the project lock and started its
/// flushers when the *caller's* half of the open fails (a worker boot
/// timeout, in the live repro). The project never reaches the active slot,
/// so nothing would ever queue its close; without the open's receipt the
/// lock is held for the page's lifetime and every retry is refused with
/// "open in this tab" until a reload.
#[test]
fn a_failed_open_gives_the_project_back_to_the_library() {
    use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
    use crate::{HOME_NODE_ID, HomeOp};

    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);

    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [7u8; 16]),
        Rc::new(|| "2026-08-14-1900".to_string()),
    );
    // Format 3 is below this build's floor: the open pre-flight refuses it
    // AFTER the host has handed the project over.
    let summary = store
        .install_package(
            "Ancient",
            &[(
                "project.json".to_string(),
                br#"{"format":3,"name":"ancient"}"#.to_vec(),
            )],
            PackageProvenance::Created,
            1.0,
        )
        .expect("install library package");
    let host = Rc::new(MemoryLibraryHost::new(store, Rc::new(|| 1.0)));
    controller.attach_library(host.clone());

    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage {
            key: summary.uid.to_string(),
        },
    )));
    drive(actor.run_one_batch_for_test());

    assert_eq!(
        host.abandoned_projects(),
        vec![summary.uid.to_string()],
        "the refused open released the library's hold on the project"
    );
    assert!(
        host.closed_projects().is_empty(),
        "and did it through the open's own receipt — nothing else knew the \
         project was open, so nothing would ever have queued its close"
    );
}

/// The D17-deviation gesture (2026-07-27): `HomeOp::CreateProject` mints a
/// pure-blank one-file package and OPENS it — the UI-level regression test
/// for the `LibraryStore::create` `format` fix (without `"format": 1` the
/// minted manifest fails the loader's root gate and this open would err).
#[test]
fn home_create_project_creates_and_opens_a_blank_package_end_to_end() {
    use crate::app::library::{LibraryStore, MemoryLibraryHost};
    use crate::{HOME_NODE_ID, HomeOp};
    use lpc_history::EventKind;

    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);

    // an empty library; create mints the uid and the dated slug
    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [5u8; 16]),
        Rc::new(|| "2026-07-27-0900".to_string()),
    );
    controller.attach_library(Rc::new(MemoryLibraryHost::new(
        store.clone(),
        Rc::new(|| 2.0),
    )));

    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::CreateProject {
            template: crate::ProjectTemplate::Blank,
        },
    )));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("create-and-open emits a snapshot");

    // create-and-open landed in the editor: home is gone and the blank
    // project renders its root card with nothing under it (the add-node
    // picker is the point)
    assert!(snapshot.home.is_none(), "the created project opened");
    let editor = project_editor(&snapshot);
    assert_eq!(editor.nodes.len(), 1, "the root module card");
    assert!(
        editor.nodes[0].children.is_empty(),
        "a blank project has no child cards"
    );

    // the library holds the package: Created origin + the initial save
    let summary = store
        .list()
        .expect("library lists")
        .pop()
        .expect("the create landed exactly one package");
    assert_eq!(summary.slug, "2026-07-27-0900-project");
    let library = store.open(summary.uid).expect("created package opens");
    assert_eq!(library.history.events()[0].kind, EventKind::Created);
    assert!(
        library
            .history
            .events()
            .iter()
            .any(|event| matches!(event.kind, EventKind::Saved { .. })),
        "the initial save snapshot is recorded"
    );

    // the open PUSHED the files: the runtime's manifest is the minted
    // one-file blank (uid + the format the loader's root gate demands)
    let pushed_manifest = {
        let bytes = server
            .borrow()
            .base_fs()
            .read_file("/projects/studio/project.json".as_path())
            .expect("pushed manifest exists in the runtime");
        String::from_utf8(bytes).expect("utf8 manifest")
    };
    assert!(
        pushed_manifest.contains(&summary.uid.to_string()),
        "the runtime holds the created package (uid pushed): {pushed_manifest}"
    );
    assert!(
        pushed_manifest.contains("\"format\""),
        "the minted manifest carries the loader-required format: {pushed_manifest}"
    );

    // detaching the lens returns to the gallery, where the created card
    // lists with Created provenance (no provenance line)
    handle.tx.send(project_action(ProjectOp::DetachLens));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("detach emits a snapshot");
    let home = snapshot.home.expect("the gallery shows after detach");
    let card = home
        .projects
        .iter()
        .find(|card| card.uid == summary.uid.to_string())
        .expect("the created package lists in the gallery");
    assert!(
        card.provenance.is_none(),
        "Created packages carry no provenance line"
    );
}

/// The P4 gesture: `New → 1D pattern project` creates-and-opens a
/// *library* project — the rig cards plus the `effect/` module card are on
/// the canvas, and the manifest that reached the runtime already
/// designates the export (which is what makes P3's exports rail appear
/// with no further gesture).
#[test]
fn home_create_project_from_the_1d_template_opens_a_designated_pattern_project() {
    use crate::app::library::{LibraryStore, MemoryLibraryHost};
    use crate::{HOME_NODE_ID, HomeOp, ProjectTemplate};

    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);

    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [6u8; 16]),
        Rc::new(|| "2026-08-07-0900".to_string()),
    );
    controller.attach_library(Rc::new(MemoryLibraryHost::new(
        store.clone(),
        Rc::new(|| 2.0),
    )));

    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::CreateProject {
            template: ProjectTemplate::Pattern1d,
        },
    )));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("create-and-open emits a snapshot");

    assert!(snapshot.home.is_none(), "the created project opened");
    let editor = project_editor(&snapshot);
    let root = &editor.nodes[0];
    let children: Vec<&str> = root
        .children
        .iter()
        .map(|child| child.label.as_str())
        .collect();
    // card labels are the humanized node names
    for expected in [
        "Clock",
        "Effect",
        "Strip 300",
        "Strip 300 out",
        "Matrix 32x16",
        "Matrix 32x16 out",
    ] {
        assert!(
            children.contains(&expected),
            "the template's {expected} card is on the canvas, got {children:?}"
        );
    }

    // R-A: the manifest's designation groups the CHILD COLUMN — the effect
    // card sits under the EXPORTS header, the rig cards under the other one
    // (P3's on-face rail is gone).
    let exports = root
        .exports
        .clone()
        .expect("the designated template opens with its child column grouped");
    let exported: Vec<&str> = root
        .children
        .iter()
        .filter(|child| exports.keys.contains(&child.detail))
        .map(|child| child.label.as_str())
        .collect();
    assert_eq!(
        exported,
        vec!["Effect"],
        "exactly the export folder's card is grouped as an export"
    );

    // R-E: no bordered group with nothing in it. The template's effect
    // invocation publishes no channel of its own, and an empty "EFFECT" box
    // on the root panel is a label pointing at nothing.
    let Some(crate::UiNodeFace::Module(root_face)) = root.face.clone() else {
        panic!("the root card wears a module face");
    };
    assert!(
        root_face.panel.groups.iter().all(|group| !group.is_empty()),
        "an empty panel group reached the root card: {:?}",
        root_face
            .panel
            .groups
            .iter()
            .map(|group| (group.label.clone(), group.controls.len()))
            .collect::<Vec<_>>()
    );

    // the library slug came from the TEMPLATE's label, not "Project"
    let summary = store
        .list()
        .expect("library lists")
        .pop()
        .expect("the create landed exactly one package");
    assert_eq!(summary.slug, "2026-08-07-0900-1d-pattern");

    // the manifest that reached the RUNTIME already designates the export
    let pushed_manifest = {
        let bytes = server
            .borrow()
            .base_fs()
            .read_file("/projects/studio/project.json".as_path())
            .expect("pushed manifest exists in the runtime");
        String::from_utf8(bytes).expect("utf8 manifest")
    };
    let manifest = lpc_model::ProjectManifest::read_json(&pushed_manifest)
        .expect("the pushed manifest parses");
    assert_eq!(
        manifest.project_kind(),
        lpc_model::ProjectKind::Pattern {
            exports: vec!["effect".to_string()]
        },
        "the template's designation reached the runtime: {pushed_manifest}"
    );
    assert!(
        manifest.uid.is_some(),
        "the library minted an identity over the template's manifest: {pushed_manifest}"
    );
}

#[test]
fn device_connect_pulls_classifies_and_adopts() {
    use crate::app::library::{LibraryStore, MemoryLibraryHost};
    use crate::app::places::DeviceContent;
    use lpc_history::SyncRelation;

    // a "device": an in-process server whose /projects/studio holds a
    // project the library does NOT know, plus a stamped identity at the
    // device's fs ROOT (identity is device-scoped, not project content).
    // Nothing is loaded — the pull's storage discovery falls back to the
    // default slot.
    let server = Rc::new(RefCell::new(device_e2e_server()));
    let device_project_dir = "/projects/studio";
    {
        let server = server.borrow();
        let fs = server.base_fs();
        fs.write_file(
            format!("{device_project_dir}/project.json").as_path(),
            br#"{"format":10,"uid":"prjdev1cedev1cedev1","name":"Porch Wild"}"#,
        )
        .unwrap();
        fs.write_file(
            format!("{device_project_dir}/module.json").as_path(),
            br#"{"kind":"Module","nodes":{}}"#,
        )
        .unwrap();
        fs.write_file(
            format!("{device_project_dir}/shader.glsl").as_path(),
            b"wild",
        )
        .unwrap();
        fs.write_file(
            "/.lp/device.json".as_path(),
            br#"{"uid":"devaaaaaaaaaaaaaaaa","name":"Bench board"}"#,
        )
        .unwrap();
    }
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);
    // Device reconcile targets the DEVICE session (runtime-pool P2), so
    // this stand-in must be device-kind, not the sim stub.
    controller.set_stub_device_for_test(
        crate::app::runtime_pool::runtime_session::ready_state_for_test(),
    );

    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [3u8; 16]),
        Rc::new(|| "2026-07-10-1000".to_string()),
    );
    let host = Rc::new(MemoryLibraryHost::new(store.clone(), Rc::new(|| 5.0)));
    controller.attach_library(host.clone());

    // 1) unknown uid + stamped identity → adoption
    drive(controller.refresh_device_sync_for_test());
    let sync = controller
        .device_sync_for_test()
        .expect("device state cached");
    assert_eq!(
        sync.identity
            .as_ref()
            .map(|identity| identity.name.as_str()),
        Some("Bench board")
    );
    let DeviceContent::Adopted {
        project_uid, slug, ..
    } = &sync.content
    else {
        panic!("unknown project adopts, got {:?}", sync.content);
    };
    assert_eq!(project_uid, "prjdev1cedev1cedev1");
    assert_eq!(slug, "2026-07-10-1000-porch-wild");
    let adopted = store.open("prjdev1cedev1cedev1".parse().unwrap()).unwrap();
    assert!(matches!(
        adopted.history.events().first().unwrap().kind,
        lpc_history::EventKind::PulledFromDevice { .. }
    ));
    let registry = crate::app::places::DeviceRegistry::new(store.fs_handle());
    assert_eq!(registry.list().unwrap().len(), 1);

    // 2) reconnect: now the uid is known and the hashes match → AtHead,
    //    no second adoption
    drive(controller.refresh_device_sync_for_test());
    let sync = controller
        .device_sync_for_test()
        .expect("device state cached");
    let DeviceContent::Known { relation, slug, .. } = &sync.content else {
        panic!("known project classifies, got {:?}", sync.content);
    };
    assert_eq!(*relation, SyncRelation::AtHead);
    assert_eq!(slug, "2026-07-10-1000-porch-wild");
    assert_eq!(store.list().unwrap().len(), 1, "no duplicate adoption");

    // 3) the device copy changes behind our back → diverged, banked
    {
        let server = server.borrow();
        server
            .base_fs()
            .write_file(
                format!("{device_project_dir}/shader.glsl").as_path(),
                b"changed on device",
            )
            .unwrap();
    }
    drive(controller.refresh_device_sync_for_test());
    let sync = controller
        .device_sync_for_test()
        .expect("device state cached");
    let DeviceContent::Known {
        relation, observed, ..
    } = &sync.content
    else {
        panic!("known project classifies, got {:?}", sync.content);
    };
    assert_eq!(*relation, SyncRelation::Diverged);
    let handle = store.open("prjdev1cedev1cedev1".parse().unwrap()).unwrap();
    assert!(
        handle.history.knows(*observed),
        "diverged device copy is banked at connect (push never destroys)"
    );
}

/// The D30 card sheet's verbs (M7′ P2): adopt-device-copy and
/// keep-both-fork dispatch with NO deploy dialog open — the diverged copy
/// resolves from the live device session's own sync evidence, and the
/// fork names itself after the device's stamped identity.
#[test]
fn d30_verbs_resolve_divergence_without_the_deploy_dialog() {
    use crate::app::device::{DEPLOY_NODE_ID, DeployOp};
    use crate::app::library::{LibraryStore, MemoryLibraryHost};
    use crate::app::places::DeviceContent;
    use lpc_history::SyncRelation;

    // The same "device" fixture as the adopt e2e above: an in-process
    // server holding an unknown project plus a stamped identity.
    let server = Rc::new(RefCell::new(device_e2e_server()));
    let device_project_dir = "/projects/studio";
    {
        let server = server.borrow();
        let fs = server.base_fs();
        fs.write_file(
            format!("{device_project_dir}/project.json").as_path(),
            br#"{"format":10,"uid":"prjdev1cedev1cedev1","name":"Porch Wild"}"#,
        )
        .unwrap();
        fs.write_file(
            format!("{device_project_dir}/module.json").as_path(),
            br#"{"kind":"Module","nodes":{}}"#,
        )
        .unwrap();
        fs.write_file(
            format!("{device_project_dir}/shader.glsl").as_path(),
            b"wild",
        )
        .unwrap();
        fs.write_file(
            "/.lp/device.json".as_path(),
            br#"{"uid":"devaaaaaaaaaaaaaaaa","name":"Bench board"}"#,
        )
        .unwrap();
    }
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);
    controller.set_stub_device_for_test(
        crate::app::runtime_pool::runtime_session::ready_state_for_test(),
    );
    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [3u8; 16]),
        Rc::new(|| "2026-07-10-1000".to_string()),
    );
    let host = Rc::new(MemoryLibraryHost::new(store.clone(), Rc::new(|| 5.0)));
    controller.attach_library(host.clone());

    // connect-as-pull adopts, then the device copy changes behind our
    // back → Diverged (the EditedOnDevice card)
    drive(controller.refresh_device_sync_for_test());
    {
        let server = server.borrow();
        server
            .base_fs()
            .write_file(
                format!("{device_project_dir}/shader.glsl").as_path(),
                b"changed on device",
            )
            .unwrap();
    }
    drive(controller.refresh_device_sync_for_test());
    let sync = controller
        .device_sync_for_test()
        .expect("device state cached");
    let DeviceContent::Known { relation, .. } = &sync.content else {
        panic!("known project classifies, got {:?}", sync.content);
    };
    assert_eq!(*relation, SyncRelation::Diverged);

    // ADOPT, with no dialog ever opened: the device's copy becomes the
    // project's new head, and the handler's own re-sync lands on AtHead.
    drive(controller.dispatch(UiAction::from_op(
        ControllerId::new(DEPLOY_NODE_ID),
        DeployOp::AdoptDeviceCopy {
            target: controller.device_target_for_test(),
        },
    )))
    .expect("adopt works straight from the card sheet");
    let sync = controller
        .device_sync_for_test()
        .expect("device state cached");
    let DeviceContent::Known { relation, .. } = &sync.content else {
        panic!("known project classifies, got {:?}", sync.content);
    };
    assert_eq!(
        *relation,
        SyncRelation::AtHead,
        "adopting made the device copy the head"
    );

    // Diverge again, then KEEP BOTH: the fork lands as a second library
    // project named after the device.
    {
        let server = server.borrow();
        server
            .base_fs()
            .write_file(
                format!("{device_project_dir}/shader.glsl").as_path(),
                b"changed on device again",
            )
            .unwrap();
    }
    drive(controller.refresh_device_sync_for_test());
    drive(controller.dispatch(UiAction::from_op(
        ControllerId::new(DEPLOY_NODE_ID),
        DeployOp::KeepBothFork {
            target: controller.device_target_for_test(),
        },
    )))
    .expect("keep-both works straight from the card sheet");
    let summaries = store.list().unwrap();
    assert_eq!(summaries.len(), 2, "the fork is a second project");
    assert!(
        summaries
            .iter()
            .any(|summary| summary.slug.contains("bench-board")),
        "the fork is named after the device: {:?}",
        summaries
            .iter()
            .map(|summary| summary.slug.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn card_native_stamp_pushes_and_records_end_to_end() {
    use crate::app::device::{DEPLOY_NODE_ID, DeployOp};
    use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
    use crate::app::places::{DeviceContent, DeviceRegistry};
    use lpc_history::{EventKind, SyncRelation};

    // a "device": empty project storage, no identity, firmware answering
    let server = Rc::new(RefCell::new(device_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);
    controller.set_stub_device_for_test(
        crate::app::runtime_pool::runtime_session::ready_state_for_test(),
    );
    // the shell-injected randomness (crypto bytes on the web) is what
    // mints `dev` uids — install a fixed generator to pin the wiring
    controller.set_random(|| [7u8; 16]);

    // a library with one pushable project (the edit-e2e node graph)
    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [4u8; 16]),
        Rc::new(|| "2026-07-10-1100".to_string()),
    );
    let summary = store
        .install_package(
            "Porch",
            &edit_e2e_files()
                .iter()
                .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let host = Rc::new(MemoryLibraryHost::new(store.clone(), Rc::new(|| 5.0)));
    controller.attach_library(host.clone());
    drive(controller.settle_library());
    drive(controller.refresh_device_sync_for_test());

    let deploy_action = |op: DeployOp| UiAction::from_op(ControllerId::new(DEPLOY_NODE_ID), op);

    // firmware yes, identity no → the Needs-a-name evidence (M8′: the
    // name sheet is the stamping surface; there is no dialog — this
    // test runs with a project open, so the card mapping itself is
    // pinned by the roster tests and the link e2e)
    let sync = controller
        .device_sync_for_test()
        .expect("connect-as-pull landed");
    assert_eq!(sync.identity, None, "unstamped");
    assert_eq!(sync.content, DeviceContent::Empty, "empty");

    // stamp (the name sheet's op): writes /.lp/device.json at the
    // device's fs ROOT + registry entry
    drive(controller.dispatch(UiAction::from_op(
        ControllerId::new(crate::app::home::HOME_NODE_ID),
        crate::HomeOp::NameDevice {
            target: controller.device_target_for_test(),
            name: "Luna's porch sign".to_string(),
        },
    )))
    .unwrap();
    let stamped_identity = {
        let bytes = server
            .borrow()
            .base_fs()
            .read_file("/.lp/device.json".as_path())
            .expect("identity stamped at the device's fs root");
        crate::app::places::DeviceIdentity::from_json_bytes(&bytes).unwrap()
    };
    assert_eq!(stamped_identity.name, "Luna's porch sign");
    assert_eq!(
        stamped_identity.uid,
        lpc_history::PrefixedUid::mint(lpc_history::UidPrefix::Device, &[7u8; 16]).to_string(),
        "the uid is minted from the injected randomness"
    );
    let registry = DeviceRegistry::new(store.fs_handle());
    assert_eq!(registry.list().unwrap().len(), 1);

    // push (the Project-tab picker's op): replace-and-load on the
    // device, hash-verified; the ROOT identity survives untouched (push
    // never re-stamps — the replace only clears the storage dir);
    // history + association recorded; device now AtHead
    let outcome = drive(controller.dispatch(deploy_action(DeployOp::PushProject {
        key: summary.uid.to_string(),
        target: controller.device_target_for_test(),
    })))
    .unwrap();
    assert!(
        outcome
            .notices
            .iter()
            .any(|notice| notice.message.contains("Pushed")
                && notice.message.contains("Luna's porch sign")),
        "the push reports its result, got {:?}",
        outcome.notices
    );

    let device_manifest = String::from_utf8(
        server
            .borrow()
            .base_fs()
            .read_file("/projects/studio/project.json".as_path())
            .unwrap(),
    )
    .unwrap();
    assert!(
        device_manifest.contains(&summary.uid.to_string()),
        "the device holds the pushed project"
    );
    let surviving_identity = server
        .borrow()
        .base_fs()
        .read_file("/.lp/device.json".as_path())
        .expect("root identity survives the push");
    assert_eq!(
        crate::app::places::DeviceIdentity::from_json_bytes(&surviving_identity)
            .unwrap()
            .uid,
        stamped_identity.uid,
        "the push did not re-stamp or alter the identity"
    );
    assert!(
        server
            .borrow()
            .base_fs()
            .read_file("/projects/studio/.lp/device.json".as_path())
            .is_err(),
        "no per-project identity copy is written anymore"
    );

    let handle = store.open(summary.uid).unwrap();
    assert!(
        handle
            .history
            .events()
            .iter()
            .any(|event| matches!(event.kind, EventKind::Pushed { .. })),
        "the push is a history event"
    );
    let devices = registry.list().unwrap();
    let association = devices[0]
        .association
        .as_ref()
        .expect("association recorded");
    assert_eq!(association.project, summary.uid);

    let sync = controller
        .device_sync_for_test()
        .expect("re-pulled after push");
    assert!(
        matches!(
            &sync.content,
            DeviceContent::Known {
                relation: SyncRelation::AtHead,
                ..
            }
        ),
        "device is at head after the push, got {:?}",
        sync.content
    );
}

/// Provisioning an ESP-class board performs NO identity write (device
/// identity design §5): the name the user chose lands on the registry row
/// keyed by the uid the board's own efuse MAC derives, and
/// `/.lp/device.json` — the erasable copy the stamp used to leave behind —
/// is never created.
///
/// The Minted counterpart is `card_native_stamp_pushes_and_records_end_to_end`
/// above: its stub board reports no MAC, so it still takes the legacy
/// stamp, file and all.
#[test]
fn naming_a_silicon_identified_board_writes_the_registry_not_the_board() {
    use crate::app::library::{LibraryStore, MemoryLibraryHost};
    use crate::app::places::DeviceRegistry;

    let server = Rc::new(RefCell::new(device_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);
    controller.set_stub_device_for_test(esp_ready_state(SILICON_MAC));
    // If anything still minted a uid here, this is the value it would use —
    // so the assertions below can say which scheme actually ran.
    controller.set_random(|| [7u8; 16]);

    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [4u8; 16]),
        Rc::new(|| "2026-08-04-1800".to_string()),
    );
    let host = Rc::new(MemoryLibraryHost::new(store.clone(), Rc::new(|| 5.0)));
    controller.attach_library(host);
    drive(controller.settle_library());
    drive(controller.refresh_device_sync_for_test());

    let sync = controller
        .device_sync_for_test()
        .expect("connect-as-pull landed");
    let identity = sync.identity.as_ref().expect("silicon is an identity");
    assert_eq!(identity.uid, silicon_uid(SILICON_MAC));
    assert_eq!(identity.name, "", "identified is not named");

    drive(controller.dispatch(UiAction::from_op(
        ControllerId::new(crate::app::home::HOME_NODE_ID),
        crate::HomeOp::NameDevice {
            target: controller.device_target_for_test(),
            name: "Luna's porch sign".to_string(),
        },
    )))
    .expect("naming dispatches");

    assert!(
        server
            .borrow()
            .base_fs()
            .read_file("/.lp/device.json".as_path())
            .is_err(),
        "an ESP board's name is registry data — nothing is stamped onto it"
    );
    let rows = DeviceRegistry::new(store.fs_handle()).list().unwrap();
    assert_eq!(rows.len(), 1, "one board, one row: {rows:?}");
    assert_eq!(
        rows[0].uid,
        silicon_uid(SILICON_MAC),
        "the name landed on the SILICON's uid, not a freshly minted one"
    );
    assert_eq!(rows[0].name, "Luna's porch sign");
    assert_eq!(
        rows[0].hardware_id.as_deref(),
        Some(format!("efuse:{SILICON_MAC}").as_str()),
        "the row records where the identity came from"
    );
    assert_eq!(
        controller
            .device_sync_for_test()
            .and_then(|sync| sync.identity.as_ref().map(|identity| identity.name.clone())),
        Some("Luna's porch sign".to_string()),
        "the live card wears the new name immediately"
    );
}

/// The rename write-back is `Minted`-only too (design §5). Renaming a
/// silicon-identified board updates the registry and the live card, and
/// leaves the board's filesystem exactly as it found it — writing a name
/// there would create a second source of truth that the next erase
/// silently disagrees with.
#[test]
fn renaming_a_silicon_identified_board_never_writes_its_filesystem() {
    use crate::app::library::{LibraryStore, MemoryLibraryHost};
    use crate::app::places::DeviceRegistry;

    let server = Rc::new(RefCell::new(device_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);
    controller.set_stub_device_for_test(esp_ready_state(SILICON_MAC));

    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [4u8; 16]),
        Rc::new(|| "2026-08-04-1800".to_string()),
    );
    let host = Rc::new(MemoryLibraryHost::new(store.clone(), Rc::new(|| 5.0)));
    controller.attach_library(host);
    drive(controller.settle_library());
    drive(controller.refresh_device_sync_for_test());

    drive(controller.dispatch(UiAction::from_op(
        ControllerId::new(crate::app::home::HOME_NODE_ID),
        crate::HomeOp::NameDevice {
            target: controller.device_target_for_test(),
            name: "Porch sign".to_string(),
        },
    )))
    .expect("naming dispatches");

    drive(controller.dispatch(UiAction::from_op(
        ControllerId::new(crate::app::home::HOME_NODE_ID),
        crate::HomeOp::RenameDevice {
            uid: silicon_uid(SILICON_MAC),
            name: "Luna's porch sign".to_string(),
        },
    )))
    .expect("renaming dispatches");

    assert!(
        server
            .borrow()
            .base_fs()
            .read_file("/.lp/device.json".as_path())
            .is_err(),
        "the rename write-back is for boards whose file is still the store"
    );
    let rows = DeviceRegistry::new(store.fs_handle()).list().unwrap();
    assert_eq!(rows.len(), 1, "one board, one row: {rows:?}");
    assert_eq!(rows[0].name, "Luna's porch sign");
    assert_eq!(
        controller
            .device_sync_for_test()
            .and_then(|sync| sync.identity.as_ref().map(|identity| identity.name.clone())),
        Some("Luna's porch sign".to_string()),
        "the live card renames immediately even with no wire write"
    );
}

#[test]
fn opening_another_package_releases_the_previous_project_lock() {
    use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
    use crate::{HOME_NODE_ID, HomeOp};

    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);

    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [9u8; 16]),
        Rc::new(|| "2026-07-09-1421".to_string()),
    );
    let files: Vec<(String, Vec<u8>)> = edit_e2e_files()
        .iter()
        .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
        .collect();
    let first = store
        .install_package("First", &files, PackageProvenance::Created, 1.0)
        .expect("install first");
    let second = store
        .install_package("Second", &files, PackageProvenance::Created, 2.0)
        .expect("install second");
    let host = Rc::new(MemoryLibraryHost::new(store, Rc::new(|| 1.0)));
    controller.attach_library(host.clone());

    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    let open = |key: String| {
        StudioCommand::Action(UiAction::from_op(
            ControllerId::new(HOME_NODE_ID),
            HomeOp::OpenPackage { key },
        ))
    };
    handle.tx.send(open(first.uid.to_string()));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv().expect("first open emits a snapshot");
    assert!(
        host.closed_projects().is_empty(),
        "the open project holds its lock"
    );

    handle.tx.send(open(second.uid.to_string()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("second open emits a snapshot");
    assert!(snapshot.home.is_none(), "the second project is open");
    assert_eq!(
        host.closed_projects(),
        vec![first.uid.to_string()],
        "switching projects releases the previous lock (and only that one)"
    );
    assert_eq!(
        host.saved_notifications(),
        Vec::<String>::new(),
        "no save happened"
    );
}

#[test]
fn save_after_home_open_pulls_the_edit_into_the_library() {
    use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
    use crate::{HOME_NODE_ID, HomeOp};

    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);

    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(|| [8u8; 16]),
        Rc::new(|| "2026-07-09-1421".to_string()),
    );
    let summary = store
        .install_package(
            "Porch sign",
            &edit_e2e_files()
                .iter()
                .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
            PackageProvenance::Created,
            1.0,
        )
        .expect("install library package");
    controller.attach_library(Rc::new(MemoryLibraryHost::new(
        store.clone(),
        Rc::new(|| 1.0),
    )));

    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage {
            key: summary.uid.to_string(),
        },
    )));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("open emits a snapshot");

    // one persisted edit, committed via Save
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
    let _ = view.try_recv().expect("save emits a snapshot");

    // the runtime committed the edit… (home opens deploy to /projects/studio)
    let runtime_fixture: String = String::from_utf8(
        server
            .borrow()
            .base_fs()
            .read_file("/projects/studio/fixture.json".as_path())
            .expect("runtime fixture.json"),
    )
    .expect("utf8")
    .chars()
    .filter(|c| !c.is_whitespace())
    .collect();
    assert!(
        runtime_fixture.contains(r#""color_order":"bgr""#),
        "the runtime def file carries the committed edit; got: {runtime_fixture}"
    );
    // …and save-as-pull carried it into the library copy + history
    let handle = store.open(summary.uid).expect("library package opens");
    let library_fixture = String::from_utf8(
        handle
            .package_fs
            .borrow()
            .read_file("/fixture.json".as_path())
            .expect("library fixture.json"),
    )
    .expect("utf8");
    assert!(
        library_fixture
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .contains(r#""color_order":"bgr""#),
        "save-as-pull must land the committed edit in the library copy; got: {library_fixture}"
    );
    assert!(
        handle.history.events().len() >= 2,
        "the save records a history event"
    );
}

#[test]
fn per_slot_clear_restores_the_debug_default_through_gated_refresh() {
    // The per-slot Clear affordance on a debug control (the clock `rate`
    // slider): SetValue then `SlotEditOp::Clear` must bring the DTO back to
    // the default through a *gated* refresh, without a reconnect.
    // The intermediate refresh below syncs the mutated def into the view
    // first, so the final assertion can only pass if the refresh after the
    // revert delivers the *reverted* def root (monotonic revisions, studio
    // editing ADR follow-up (e)) — not because a stale mirror or buffer
    // entry happened to shadow the right value.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let transport = clock_transport_block(&snapshot);
    assert_eq!(transport.rate, 1.0);
    let rate_address = transport
        .rate_address
        .clone()
        .expect("transport block carries the rate dispatch address");

    // Edit the debug control, then pull a gated refresh so the synced
    // view itself holds the edited value.
    handle
        .tx
        .send(set_value_action(rate_address.clone(), LpValue::F32(2.0)));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edit + refresh emit a snapshot");
    let transport = clock_transport_block(&snapshot);
    assert!(transport.rate_override.is_some());
    assert_eq!(transport.rate, 2.0);

    // Per-value Clear: drop the debug override, then a gated refresh must
    // show the default again. For a Debug slot the authored default IS the
    // shape default, so Clear and reset-to-authored coincide. This is the
    // exact op the tape's `clear` affordance dispatches per override.
    handle.tx.send(clear_action(rate_address));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("clear + refresh emit a snapshot");

    let transport = clock_transport_block(&snapshot);
    assert_eq!(transport.rate_override, None);
    assert_eq!(
        transport.rate, 1.0,
        "per-value Clear restores the default through the gated refresh"
    );
}

#[test]
fn set_back_to_base_normalizes_to_clean_without_overlay_fetch() {
    // Minimal-diff normalization, user scenario: pick a choice value
    // (diagnostic-mode style), use it, set it back to the authored value —
    // the edited highlight must clear. The server elides the base-equal
    // assignment (NormalizedToRemoval) and the mirror must learn that from
    // the ack alone: the overlay revision may not advance, so a corrective
    // ReadOverlay would never fire.
    //
    // The refresh between the two edits is load-bearing: it syncs the edited
    // value into the project view, so the set-back ack opens the stale-view
    // window (the view still holds the old effective value until the next
    // gated read). The DTO must keep showing the value the user typed through
    // that window — the buffer entry parks as `AwaitingRefresh` instead of
    // releasing — not jitter back to the superseded value.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(slot_value_display(color_order), "grb", "authored default");
    let address = color_order
        .address
        .clone()
        .expect("color order slot carries an address");

    // Change the choice: dirty, counted; the refresh syncs the edited value
    // into the project view (the stale-window precondition).
    handle.tx.send(set_value_action(
        address.clone(),
        LpValue::String("rgb".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edit + refresh emit a snapshot");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(
        slot_value_display(color_order),
        "rgb",
        "the synced view holds the edited effective value"
    );
    assert_eq!(editor_dirty(&snapshot), (1, 0));

    // Set it back to the authored value. The ack normalizes the edit away,
    // but the synced view still holds "rgb" until the next gated read: the
    // DTO must keep showing the typed value ("grb"), not jitter back.
    let overlay_reads_before = count_overlay_reads(&sent);
    handle.tx.send(set_value_action(
        address,
        LpValue::String("grb".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("set-back emits a snapshot");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(
        slot_value_display(color_order),
        "grb",
        "the typed base value stays visible through the stale-view window"
    );
    assert_eq!(
        color_order.state.dirty,
        UiNodeDirtyState::Saving,
        "the normalized edit keeps the Saving treatment until the view catches up"
    );

    // The next refresh delivers the reverted def: highlight cleared, value
    // stable, and no overlay fetch corrected the mirror — the ack effect
    // alone did it.
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(
        color_order.state.dirty,
        UiNodeDirtyState::Clean,
        "setting a slot back to its base value clears the edited state"
    );
    assert_eq!(
        slot_value_display(color_order),
        "grb",
        "the value never rubber-bands through the whole set-back"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0));
    assert_eq!(
        count_overlay_reads(&sent) - overlay_reads_before,
        0,
        "the mirror is corrected by the ack effect, not a ReadOverlay"
    );
}

#[test]
fn composite_gesture_cycle_ends_clean_end_to_end() {
    // The M3 composite gesture cycle on the fixture `mapping` slot, driven
    // through the same actor command path the web shell uses: switch the
    // enum variant (EnsurePresent mapping.PathPoints), add a map entry
    // (EnsurePresent mapping.PathPoints.paths[0]), remove it again
    // (RemoveValue — the server normalizes the add-then-remove away, D2),
    // then revert the variant switch — the project must end clean.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(mapping.detail.as_deref(), Some("variant Unset"));
    let mapping_address = mapping
        .address
        .clone()
        .expect("mapping slot carries an address");
    assert_eq!(editor_dirty(&snapshot), (0, 0));

    // Switch the variant. The overlay edit is stored at a path with no row
    // yet (the base variant is still Unset until the refresh applies), so
    // the enum row reads dirty through the prefix join immediately.
    let variant_address = child_address(&mapping_address, "mapping.PathPoints");
    handle
        .tx
        .send(ensure_present_action(variant_address.clone()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("variant switch emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(
        mapping.state.dirty,
        UiNodeDirtyState::Dirty,
        "the acked variant switch surfaces on the enum row before any refresh"
    );
    assert_eq!(mapping.detail.as_deref(), Some("variant Unset"));
    assert_eq!(
        mapping.edit_entry_address,
        Some(variant_address.clone()),
        "the enum row offers the variant-switch entry as its revert target \
         even before the view's active variant catches up"
    );
    assert_eq!(editor_dirty(&snapshot), (1, 0));

    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.detail.as_deref(), Some("variant PathPoints"));
    assert_eq!(mapping.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(
        mapping.edit_entry_address,
        Some(variant_address.clone()),
        "after the switch round-trips, the enum row still offers a working \
         Revert (the entry lives at the variant child path, not the row's own)"
    );

    // Add a path entry with server-built defaults, then pull the new row.
    let entry_address = child_address(&mapping_address, "mapping.PathPoints.paths[0]");
    handle.tx.send(ensure_present_action(entry_address.clone()));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view
        .try_recv()
        .expect("entry add + refresh emit a snapshot");
    let entry = find_slot(&snapshot, "mapping.PathPoints.paths[0]");
    assert_eq!(
        entry.state.dirty,
        UiNodeDirtyState::Dirty,
        "the added entry row exists with a server-built default and reads dirty"
    );
    assert_eq!(editor_dirty(&snapshot), (2, 0));

    // Remove it again: add-then-remove cancels on the server (D2). Between
    // the normalized ack and the refresh, the stale view still shows the
    // row — it must read Saving (the AwaitingRefresh bridge), not flash a
    // clean row that then vanishes.
    handle.tx.send(remove_value_action(entry_address));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("entry remove emits a snapshot");
    let entry = find_slot(&snapshot, "mapping.PathPoints.paths[0]");
    assert_eq!(
        entry.state.dirty,
        UiNodeDirtyState::Saving,
        "the normalized removal keeps the Saving treatment until the view catches up"
    );

    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view
        .try_recv()
        .expect("entry remove + refresh emit a snapshot");
    assert!(
        try_find_slot(&snapshot, "mapping.PathPoints.paths[0]").is_none(),
        "the removed entry has no surviving row"
    );
    assert_eq!(
        editor_dirty(&snapshot),
        (1, 0),
        "only the variant switch remains"
    );

    // Revert the variant switch from the enum row itself, exactly as the UI
    // would: dispatch Revert at the row's projected `edit_entry_address`.
    // The overlay empties and the project is clean again, back on the
    // authored Unset variant.
    let mapping = find_slot(&snapshot, "mapping");
    let row_revert_target = mapping
        .edit_entry_address
        .clone()
        .expect("the enum row offers a revert target for the pending switch");
    assert_eq!(row_revert_target, variant_address);
    handle.tx.send(revert_action(row_revert_target));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert + refresh emit a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(mapping.detail.as_deref(), Some("variant Unset"));
    assert_eq!(
        editor_dirty(&snapshot),
        (0, 0),
        "the gesture cycle ends clean"
    );
}

#[test]
fn variant_dropdown_switch_away_and_back_ends_clean_from_acks_alone() {
    // The dropdown repro: switch the mapping enum away from its base variant
    // (EnsurePresent mapping.PathPoints), then re-select the base variant
    // (EnsurePresent mapping.Unset). The switch-back normalizes away on the
    // server *and* clears the pending sibling switch; the Materialized ack
    // is the mirror's only source — no ReadOverlay may fire. Without the
    // sibling clearing, the stored mapping.PathPoints entry would survive
    // and the dropdown would stay stuck on PathPoints forever.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.detail.as_deref(), Some("variant Unset"));
    assert_eq!(mapping.state.dirty, UiNodeDirtyState::Clean);
    let mapping_address = mapping
        .address
        .clone()
        .expect("mapping slot carries an address");
    let overlay_reads_before = count_overlay_reads(&sent);

    // Switch away, then refresh so the user-visible dropdown really shows
    // the pending variant before switching back.
    handle.tx.send(ensure_present_action(child_address(
        &mapping_address,
        "mapping.PathPoints",
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("switch + refresh emit a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.detail.as_deref(), Some("variant PathPoints"));
    assert_eq!(mapping.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(editor_dirty(&snapshot), (1, 0));

    // Re-select the base variant from the dropdown: the pending switch must
    // go away entirely, not normalize into a stuck sibling entry.
    handle.tx.send(ensure_present_action(child_address(
        &mapping_address,
        "mapping.Unset",
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view
        .try_recv()
        .expect("switch-back + refresh emit a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(
        mapping.detail.as_deref(),
        Some("variant Unset"),
        "the effective def is back on the base variant"
    );
    assert_eq!(
        mapping.state.dirty,
        UiNodeDirtyState::Clean,
        "no pending sibling switch survives the switch-back"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0), "the cycle ends clean");
    assert_eq!(
        count_overlay_reads(&sent) - overlay_reads_before,
        0,
        "the mirror is corrected by the ack effects alone, not a ReadOverlay"
    );
}

#[test]
fn option_toggle_off_then_on_ends_clean_from_acks_alone() {
    // The dead-click repro on the fixture `brightness` option (base-present:
    // the shape default is Some(0.25)): toggle OFF (RemoveValue brightness —
    // stores `Remove` at the option path), refresh, toggle back ON
    // (EnsurePresent brightness.some — normalizes away against base at a
    // DIFFERENT path). The counteracting-entry sweep clears the stored
    // Remove and the Materialized ack is the mirror's only source — no
    // ReadOverlay may fire. Without it, the stored Remove survives and the
    // toggle-on click does nothing, forever.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let brightness = find_slot(&snapshot, "brightness");
    assert_eq!(brightness.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(brightness),
        "0.25",
        "base default is Some(0.25)"
    );
    let brightness_address = brightness
        .address
        .clone()
        .expect("brightness slot carries an address");
    assert_eq!(editor_dirty(&snapshot), (0, 0));
    let overlay_reads_before = count_overlay_reads(&sent);

    // Toggle off, then refresh so the user-visible row really shows the
    // excluded state before toggling back on.
    handle
        .tx
        .send(remove_value_action(brightness_address.clone()));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view
        .try_recv()
        .expect("toggle-off + refresh emit a snapshot");
    let brightness = find_slot(&snapshot, "brightness");
    assert!(
        matches!(brightness.body, UiConfigSlotBody::Empty),
        "the toggled-off option row has no value body"
    );
    assert_eq!(brightness.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(editor_dirty(&snapshot), (1, 0));

    // Toggle back on: the EnsurePresent at brightness.some normalizes away
    // and must clear the stored Remove at the option path — the exact user
    // symptom was this click doing nothing.
    handle.tx.send(ensure_present_action(child_address(
        &brightness_address,
        "brightness.some",
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view
        .try_recv()
        .expect("toggle-on + refresh emit a snapshot");
    let brightness = find_slot(&snapshot, "brightness");
    assert_eq!(
        slot_value_display(brightness),
        "0.25",
        "the effective option is back to the base value"
    );
    assert_eq!(
        brightness.state.dirty,
        UiNodeDirtyState::Clean,
        "no counteracting Remove survives the off-then-on cycle"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0), "the cycle ends clean");
    assert_eq!(
        count_overlay_reads(&sent) - overlay_reads_before,
        0,
        "the mirror is corrected by the ack effects alone, not a ReadOverlay"
    );
}

#[test]
fn bind_and_unbind_gestures_present_authored_state_from_acks_alone() {
    // The slot detail popover's binding story (D26 follow-up): the bind
    // gesture (EnsurePresent entry → EnsurePresent endpoint option →
    // SetValue) and the unbind gesture (RemoveValue on the entry) must flip
    // the per-slot binding presentation — authored endpoint, Unbind
    // affordance — from the mutation acks alone. No RefreshProject runs in
    // this test: before the ack-time re-derivation, the presentation lagged
    // the passive read cadence by up to tens of seconds.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let color_order = find_slot(&snapshot, "color_order");
    let authoring = color_order
        .authoring
        .as_ref()
        .expect("bindable def row carries authoring");
    assert!(authoring.authored.is_none(), "nothing is bound yet");
    let color_order_address = color_order
        .address
        .clone()
        .expect("color order slot carries an address");

    // The popover's bind gesture, exactly as BindingAuthoringSection
    // dispatches it.
    let entry_address = child_address(&color_order_address, "bindings[color_order]");
    let endpoint_address = child_address(&color_order_address, "bindings[color_order].source.some");
    handle.tx.send(ensure_present_action(entry_address.clone()));
    handle
        .tx
        .send(ensure_present_action(endpoint_address.clone()));
    handle.tx.send(set_value_action(
        endpoint_address,
        LpValue::String("bus:wave".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("bind gesture emits a snapshot");

    let color_order = find_slot(&snapshot, "color_order");
    let authoring = color_order.authoring.as_ref().expect("authoring");
    assert_eq!(
        authoring.authored.as_ref().map(|e| e.label.as_str()),
        Some("bus:wave"),
        "the acked bind reads authored (Retarget/Unbind) with no refresh in between"
    );
    assert!(
        matches!(&color_order.source, crate::UiSlotSourceState::Bound(endpoint)
            if endpoint.label == "bus:wave" && !endpoint.default_origin),
        "the row presents the authored wiring, not default-origin: {:?}",
        color_order.source
    );

    // Unbind from the same popover: the entry removal must clear the
    // authored presentation from its ack alone as well.
    handle.tx.send(remove_value_action(entry_address));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("unbind emits a snapshot");

    let color_order = find_slot(&snapshot, "color_order");
    assert!(
        color_order
            .authoring
            .as_ref()
            .expect("authoring")
            .authored
            .is_none(),
        "the acked unbind drops the authored entry with no refresh in between"
    );
    assert!(
        matches!(color_order.source, crate::UiSlotSourceState::Direct),
        "with no declared default, the slot reads direct again: {:?}",
        color_order.source
    );
}

#[test]
fn removing_an_added_and_edited_entry_ends_clean_from_the_ack_alone() {
    // Mirror fidelity for the subtree-clearing structural remove: add a map
    // entry, edit a leaf under it, remove the entry again. The remove
    // normalizes away on the server and also clears the stranded descendant
    // assignment; the ack (`MutationEffect::Materialized` listing every
    // removed overlay entry) is the mirror's only source — no ReadOverlay
    // may fire. If either side kept the stranded edit, re-applying it would
    // resurrect the entry and the project could never read clean again.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    let mapping_address = mapping
        .address
        .clone()
        .expect("mapping slot carries an address");
    assert_eq!(editor_dirty(&snapshot), (0, 0));
    let overlay_reads_before = count_overlay_reads(&sent);

    // Switch the variant, add an entry, edit a leaf under the added entry.
    let variant_address = child_address(&mapping_address, "mapping.PathPoints");
    handle
        .tx
        .send(ensure_present_action(variant_address.clone()));
    drive(actor.run_one_batch_for_test());
    let entry_address = child_address(&mapping_address, "mapping.PathPoints.paths[0]");
    handle.tx.send(ensure_present_action(entry_address.clone()));
    drive(actor.run_one_batch_for_test());
    let leaf_address = child_address(
        &mapping_address,
        "mapping.PathPoints.paths[0].PointList.first_channel",
    );
    handle
        .tx
        .send(set_value_action(leaf_address, LpValue::U32(7)));
    drive(actor.run_one_batch_for_test());

    // Remove the entry again: the server clears the entry *and* the
    // stranded leaf edit, and the mirror follows from the Materialized ack.
    handle.tx.send(remove_value_action(entry_address));
    drive(actor.run_one_batch_for_test());

    // Revert the remaining variant switch: with the subtree really gone on
    // both sides this empties the overlay entirely.
    handle.tx.send(revert_action(variant_address));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    let mapping = find_slot(&snapshot, "mapping");
    assert_eq!(mapping.detail.as_deref(), Some("variant Unset"));
    assert_eq!(
        mapping.state.dirty,
        UiNodeDirtyState::Clean,
        "no stranded edit may keep the mapping dirty or resurrect the entry"
    );
    assert!(
        try_find_slot(&snapshot, "mapping.PathPoints.paths[0]").is_none(),
        "the removed entry has no surviving row"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0), "the cycle ends clean");
    assert_eq!(
        count_overlay_reads(&sent) - overlay_reads_before,
        0,
        "the mirror is corrected by the ack effects alone, not a ReadOverlay"
    );
}

#[test]
fn special_editor_values_round_trip_save_and_revert() {
    // M4 special editors: the fixture's `render_size` (Dim2u, `Dimensions`
    // hint) and `transform` (Affine2d, wire `Mat3x3`, `Affine2d` hint) reach
    // the DTO with their specialized editor hints, and whole-value writes
    // composed exactly the way the editors compose them (struct name and
    // field order preserved; the inactive Mat3x3 row fixed to [0, 0, 1])
    // round-trip through save and revert.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    let render_size = find_slot(&snapshot, "render_size");
    assert_eq!(
        slot_editor_hint(render_size),
        &UiSlotEditorHint::Dimensions,
        "render_size carries the Dimensions editor hint"
    );
    let render_size_address = render_size
        .address
        .clone()
        .expect("render_size slot carries an address");
    let transform = find_slot(&snapshot, "transform");
    assert_eq!(
        slot_editor_hint(transform),
        &UiSlotEditorHint::Affine2d,
        "transform carries the Affine2d editor hint"
    );
    let transform_address = transform
        .address
        .clone()
        .expect("transform slot carries an address");

    // Whole-value writes as the editors dispatch them.
    handle.tx.send(set_value_action(
        render_size_address.clone(),
        LpValue::Struct {
            name: Some("Dim2u".to_string()),
            fields: vec![
                ("width".to_string(), LpValue::U32(12)),
                ("height".to_string(), LpValue::U32(10)),
            ],
        },
    ));
    handle.tx.send(set_value_action(
        transform_address,
        LpValue::Mat3x3([[1.0, 0.0, 4.5], [0.0, 1.0, -2.0], [0.0, 0.0, 1.0]]),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edits emit a snapshot");

    let render_size = find_slot(&snapshot, "render_size");
    assert_eq!(render_size.state.dirty, UiNodeDirtyState::Dirty);
    assert!(!render_size.state.debug, "render_size is a persisted slot");
    let transform = find_slot(&snapshot, "transform");
    assert_eq!(transform.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(editor_dirty(&snapshot), (2, 0));

    // Save: both persisted edits materialize into fixture.json.
    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("save + refresh emit a snapshot");

    let fixture_json = read_project_file(&server, "fixture.json");
    assert!(
        fixture_json.contains("\"width\":12"),
        "fixture.json gained the dimensions edit: {fixture_json}"
    );
    assert!(
        fixture_json.contains("\"transform\":[[1,0,4.5],[0,1,-2],[0,0,1]]"),
        "fixture.json gained the affine transform edit: {fixture_json}"
    );
    let render_size = find_slot(&snapshot, "render_size");
    assert_eq!(render_size.state.dirty, UiNodeDirtyState::Clean);
    assert!(slot_value_display(render_size).contains("12"));
    let transform = find_slot(&snapshot, "transform");
    assert_eq!(transform.state.dirty, UiNodeDirtyState::Clean);
    assert!(slot_value_display(transform).contains("4.5"));
    assert_eq!(editor_dirty(&snapshot), (0, 0));

    // Revert: a fresh edit on top of the saved values is discarded and the
    // gated refresh restores the saved (committed) values.
    handle.tx.send(set_value_action(
        render_size_address,
        LpValue::Struct {
            name: Some("Dim2u".to_string()),
            fields: vec![
                ("width".to_string(), LpValue::U32(20)),
                ("height".to_string(), LpValue::U32(10)),
            ],
        },
    ));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RevertAllEdits));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert emits a snapshot");

    let render_size = find_slot(&snapshot, "render_size");
    assert_eq!(render_size.state.dirty, UiNodeDirtyState::Clean);
    assert!(
        slot_value_display(render_size).contains("12"),
        "revert restores the saved dimensions, not the fresh edit: {}",
        slot_value_display(render_size)
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0));
}

#[test]
fn power_option_gains_the_editor_and_round_trips_save() {
    // The fixture `power` option (base-absent): toggling it on materializes
    // the server-constructed default (WS2812B at 1000 mA), the present value
    // carries the `Power` editor hint so the lamp/budget pair is editable
    // rather than an opaque struct display, and a whole-struct SetValue —
    // composed exactly the way `PowerSlotField` composes it — reaches
    // fixture.json on save.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    let power = find_slot(&snapshot, "power");
    assert!(
        matches!(power.body, UiConfigSlotBody::Empty),
        "power starts absent (the default guard is engine-side, not authored)"
    );
    let power_address = power
        .address
        .clone()
        .expect("power slot carries an address");

    // Toggle on: the server constructs the default value.
    handle.tx.send(ensure_present_action(child_address(
        &power_address,
        "power.some",
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("toggle-on emits a snapshot");

    let power = find_slot(&snapshot, "power");
    assert_eq!(
        slot_editor_hint(power),
        &UiSlotEditorHint::Power,
        "the present power value carries the Power editor hint"
    );
    assert!(
        slot_value_display(power).contains("1000"),
        "the server default budget materializes: {}",
        slot_value_display(power)
    );

    // Whole-struct write as PowerSlotField dispatches it.
    handle.tx.send(set_value_action(
        child_address(&power_address, "power.some"),
        LpValue::Struct {
            name: Some("FixturePower".to_string()),
            fields: vec![
                (
                    "lamp_type".to_string(),
                    LpValue::String("ws2811_12v".to_string()),
                ),
                ("budget_ma".to_string(), LpValue::U32(4000)),
            ],
        },
    ));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("save + refresh emit a snapshot");

    let fixture_json = read_project_file(&server, "fixture.json");
    assert!(
        fixture_json.contains("\"lamp_type\":\"ws2811_12v\""),
        "fixture.json gained the lamp edit: {fixture_json}"
    );
    assert!(
        fixture_json.contains("\"budget_ma\":4000"),
        "fixture.json gained the budget edit: {fixture_json}"
    );
    let power = find_slot(&snapshot, "power");
    assert_eq!(power.state.dirty, UiNodeDirtyState::Clean);
    assert!(slot_value_display(power).contains("4000"));
}

// --- Harness ---------------------------------------------------------------

#[test]
fn accepted_apply_tightens_the_next_refresh_delay() {
    use crate::{DEVICE_REFRESH_INTERVAL, VERDICT_CHASE_INTERVAL, VERDICT_CHASE_TICKS};

    let server = Rc::new(RefCell::new(asset_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);
    // Cadence is per-session KIND (runtime-pool P2): a device-kind lens
    // runs the calm interval the chase window tightens; the sim's 33 ms
    // interval is already tighter than the chase.
    controller.set_stub_device_for_test(
        crate::app::runtime_pool::runtime_session::ready_state_for_test(),
    );
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    // Completion-based pacing: a never-pulled lens is immediately due, so
    // prime one passive pull; its completion stamp arms the device gap.
    assert_eq!(
        handle.delay.get(),
        core::time::Duration::ZERO,
        "a never-pulled lens session is immediately due"
    );
    handle.tx.send(StudioCommand::RefreshTick);
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    assert_eq!(
        handle.delay.get(),
        DEVICE_REFRESH_INTERVAL,
        "a device-kind lens session runs at device cadence before any apply"
    );

    let tab = find_asset_editor(&snapshot);
    handle.tx.send(StudioCommand::Action(tab.fetch_action()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("fetch emits a snapshot");
    let tab = find_asset_editor(&snapshot);

    // An accepted apply opens the verdict-chase window. The published delay
    // is the tighter of the device gap and the chase interval — with the
    // completion-paced 150 ms device gap the chase no longer tightens
    // anything (it guards a future retune of the gap above the chase), so
    // the delay stays at the device gap throughout.
    let chase_gap = DEVICE_REFRESH_INTERVAL.min(VERDICT_CHASE_INTERVAL);
    handle
        .tx
        .send(StudioCommand::Action(tab.apply_action(ASSET_SHADER_V2)));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv().expect("apply emits a snapshot");
    assert_eq!(
        handle.delay.get(),
        chase_gap,
        "an accepted apply never loosens the tick below the chase interval"
    );

    // Each full pull consumes one chase tick; the cadence then relaxes.
    for _ in 0..VERDICT_CHASE_TICKS {
        assert_eq!(handle.delay.get(), chase_gap);
        handle.tx.send(project_action(ProjectOp::RefreshProject));
        drive(actor.run_one_batch_for_test());
        let _ = view.try_recv();
    }
    assert_eq!(
        handle.delay.get(),
        DEVICE_REFRESH_INTERVAL,
        "the cadence relaxes once the chase window is consumed"
    );
}

#[test]
fn shader_asset_editor_fetch_apply_save_and_revert_end_to_end() {
    let server = Rc::new(RefCell::new(asset_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    // The shader node's editor tab exists but its content is unresolved
    // until the editor dispatches the fetch (base bodies are not pulled
    // eagerly for every asset in the project).
    let tab = find_asset_editor(&snapshot);
    assert_eq!(tab.source, "shader.glsl");
    assert!(tab.content.is_none(), "content resolves only on fetch");
    // The consumed uniforms ride the editor DTO for completions, typed as
    // the generated uniform header declares them.
    assert_eq!(
        tab.uniforms,
        vec![crate::UiShaderUniform {
            name: String::from("time"),
            glsl_type: String::from("float"),
        }],
        "the shader's consumed map projects as editor uniforms"
    );

    // Fetch → the effective content is the base file body, clean.
    handle.tx.send(StudioCommand::Action(tab.fetch_action()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("fetch emits a snapshot");
    let tab = find_asset_editor(&snapshot);
    let content = tab.content.as_ref().expect("fetched content");
    assert!(!content.dirty);
    assert_eq!(content.text(), Some(ASSET_SHADER_V1));
    assert_eq!(editor_dirty(&snapshot), (0, 0));

    // Apply an edited body: overlay-backed dirty (persisted-class), the
    // effective content shadows to the applied text, save panel lists it.
    handle
        .tx
        .send(StudioCommand::Action(tab.apply_action(ASSET_SHADER_V2)));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("apply emits a snapshot");
    let tab = find_asset_editor(&snapshot);
    let content = tab.content.as_ref().expect("applied content");
    assert!(content.dirty, "applied body is overlay-dirty");
    assert_eq!(content.text(), Some(ASSET_SHADER_V2));
    assert_eq!(
        editor_dirty(&snapshot),
        (1, 0),
        "asset edits are persisted-class"
    );

    // Save: the .glsl on disk gains the applied source and dirty clears.
    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("save + refresh emit a snapshot");
    let saved = read_project_file(&server, "shader.glsl");
    assert!(
        saved.contains("v2marker"),
        "shader.glsl gained the applied body: {saved}"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0));

    // The save invalidated the cached base body; the editor re-fetches and
    // sees the committed text, clean.
    let tab = find_asset_editor(&snapshot);
    assert!(tab.content.is_none(), "save invalidates the cached body");
    handle.tx.send(StudioCommand::Action(tab.fetch_action()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("re-fetch emits a snapshot");
    let tab = find_asset_editor(&snapshot);
    let content = tab.content.as_ref().expect("re-fetched content");
    assert!(!content.dirty);
    assert_eq!(content.text(), Some(ASSET_SHADER_V2));

    // Apply again, then per-entry revert: the overlay entry clears, dirty
    // returns to zero, and the re-fetched content is the saved body.
    handle
        .tx
        .send(StudioCommand::Action(tab.apply_action(ASSET_SHADER_V3)));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("second apply emits a snapshot");
    assert_eq!(editor_dirty(&snapshot), (1, 0));
    let tab = find_asset_editor(&snapshot);
    let revert = UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::AssetEditOp::Revert {
            artifact: tab.artifact.clone(),
        },
    );
    handle.tx.send(StudioCommand::Action(revert));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert emits a snapshot");
    assert_eq!(editor_dirty(&snapshot), (0, 0));
    let tab = find_asset_editor(&snapshot);
    handle.tx.send(StudioCommand::Action(tab.fetch_action()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("post-revert fetch emits a snapshot");
    let tab = find_asset_editor(&snapshot);
    let content = tab.content.as_ref().expect("post-revert content");
    assert!(!content.dirty);
    assert_eq!(
        content.text(),
        Some(ASSET_SHADER_V2),
        "revert returns to the saved body, not the pre-save one"
    );
}

#[test]
fn the_output_card_gets_a_debug_section_for_test_pattern() {
    // P5, over the real wire: `OutputDef.test_pattern` is `SlotRole::Debug`,
    // and NOTHING output-specific exists in the UI layer — the same
    // role-keyed partition that gives the Clock its Debug section (P3) gives
    // the output card one, with the toggle in it. Channel endpoints stay Settings.
    let server = Rc::new(RefCell::new(asset_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    let sections = node_sections(&snapshot, "/edit_e2e.show/output.output");
    assert_eq!(
        section_slot_labels(&sections, |section| matches!(
            section,
            UiNodeSection::DebugSlots(_)
        )),
        vec!["Test pattern", "Highlight"],
        "the output's Debug fields render in the Debug section"
    );
    assert!(
        !section_slot_labels(&sections, |section| matches!(
            section,
            UiNodeSection::ConfigSlots(_)
        ))
        .iter()
        .any(|label| label == "Test pattern"),
        "a Debug field is never also a Setting row"
    );

    let test_pattern = find_slot(&snapshot, "test_pattern");
    assert!(test_pattern.state.debug, "test_pattern is a Debug slot");
    assert!(
        test_pattern.state.editable,
        "a Debug slot is writable — that is the whole point of the toggle"
    );
    assert_eq!(test_pattern.state.dirty, UiNodeDirtyState::Clean);
    let endpoint = find_slot(&snapshot, "ports[0].endpoint");
    assert!(
        !endpoint.state.debug,
        "a channel endpoint is authored config, not debug"
    );
}

#[test]
fn successive_shader_applies_each_reach_the_engine() {
    // Regression: an overlay→overlay body change (second Apply before any
    // Save) must recompile just like the first (base→overlay) one. Observed
    // live 2026-07-06: the engine kept reporting the first apply's compile
    // error after later applies.
    let server = Rc::new(RefCell::new(asset_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let tab = find_asset_editor(&snapshot);

    // First apply: an unknown identifier. Frames advance between edits in
    // production; mirror that here — the mutation must stamp a revision
    // strictly newer than the last compile's, and the engine compiles
    // lazily on render, so tick before and after.
    server.borrow_mut().advance_frame(16).expect("tick");
    handle.tx.send(StudioCommand::Action(tab.apply_action(
        "vec4 render_2d(vec2 pos) { return vec4(first_bad, 0.0, 0.0, 1.0); }",
    )));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    server.borrow_mut().advance_frame(16).expect("tick");
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    let error = find_asset_editor(&snapshot)
        .shader_error
        .expect("first bad apply surfaces a compile error");
    assert!(
        error.raw.contains("first_bad"),
        "engine error names the first bad identifier: {}",
        error.raw
    );

    // Second apply while the first is still pending in the overlay: the new
    // body must recompile and the error must move to the new identifier.
    let snapshot_tab = find_asset_editor(&snapshot);
    server.borrow_mut().advance_frame(16).expect("tick");
    handle
        .tx
        .send(StudioCommand::Action(snapshot_tab.apply_action(
            "vec4 render_2d(vec2 pos) { return vec4(second_bad, 0.0, 0.0, 1.0); }",
        )));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    server.borrow_mut().advance_frame(16).expect("tick");
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("second refresh emits a snapshot");
    let error = find_asset_editor(&snapshot)
        .shader_error
        .expect("second bad apply surfaces a compile error");
    assert!(
        error.raw.contains("second_bad"),
        "the second applied body reached the engine: {}",
        error.raw
    );

    // And a valid third apply recovers: the error clears.
    let snapshot_tab = find_asset_editor(&snapshot);
    server.borrow_mut().advance_frame(16).expect("tick");
    handle.tx.send(StudioCommand::Action(
        snapshot_tab.apply_action(ASSET_SHADER_V1),
    ));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    server.borrow_mut().advance_frame(16).expect("tick");
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("third refresh emits a snapshot");
    assert_eq!(
        find_asset_editor(&snapshot).shader_error,
        None,
        "a valid re-apply clears the compile error"
    );
}

pub(crate) const ASSET_SHADER_V1: &str = "uniform float time;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(pos.x, pos.y, 0.5, 1.0);\n}\n";
const ASSET_SHADER_V2: &str = "// v2marker\nuniform float time;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(pos.y, pos.x, 0.25, 1.0);\n}\n";
const ASSET_SHADER_V3: &str = "// v3marker\nuniform float time;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(0.1, 0.2, 0.3, 1.0);\n}\n";

/// Find the shader node's inline asset editor anywhere in the editor DTO
/// tree: it rides `UiSlotAsset::inline_editor` on the node's (or a child
/// node's) asset slot sections.
pub(crate) fn find_asset_editor(view: &UiStudioView) -> crate::UiAssetEditor {
    let editor = view
        .panes
        .iter()
        .find_map(|pane| match &pane.body {
            UiViewContent::ProjectEditor(editor) => Some(editor),
            _ => None,
        })
        .expect("project editor pane");

    fn in_slots(slots: &[crate::UiConfigSlot]) -> Option<crate::UiAssetEditor> {
        slots.iter().find_map(|slot| match &slot.body {
            crate::UiConfigSlotBody::Asset(asset) => asset.inline_editor.clone(),
            crate::UiConfigSlotBody::Record(record) => in_slots(&record.fields),
            _ => None,
        })
    }
    fn in_sections(sections: &[UiNodeSection]) -> Option<crate::UiAssetEditor> {
        sections.iter().find_map(|section| match section {
            UiNodeSection::AssetSlots(slots)
            | UiNodeSection::ConfigSlots(slots)
            | UiNodeSection::DebugSlots(slots) => in_slots(slots),
            _ => None,
        })
    }
    fn in_children(children: &[crate::UiNodeChild]) -> Option<crate::UiAssetEditor> {
        children
            .iter()
            .find_map(|child| in_sections(&child.sections).or_else(|| in_children(&child.children)))
    }

    editor
        .nodes
        .iter()
        .find_map(|node| {
            node.tabs
                .iter()
                .find_map(|tab| match &tab.body {
                    UiNodeTabBody::Sections(sections) => in_sections(sections),
                    _ => None,
                })
                .or_else(|| in_children(&node.children))
        })
        .expect("shader node exposes an inline asset editor")
}

pub(crate) fn asset_e2e_server() -> LpServer {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    let mut server = LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "projects".as_path(),
        None,
        None,
        graphics,
    );

    // The shader publishes to the visual bus and a fixture consumes it —
    // without a consumer the shader never renders, so it would never
    // (re)compile and compile errors would never surface.
    //
    // `speed` is wired to a channel with no def record yet: that is the
    // agent e2e's repair shape (declare the uniform, upsert the record) and,
    // since Q13, the binding is also what will put the repaired param on the
    // panel — publicity is the binding, not an authored flag.
    let shader_json = r#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "bindings": {
    "speed": { "source": "bus:speed" },
    "output": { "target": "bus:visual.out" }
  },
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0,
      "label": "Time",
      "description": "Project clock time in seconds"
    }
  }
}"#;
    let fixture_json = r#"{
  "kind": "Fixture",
  "render_size": { "width": 4, "height": 4 },
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  }
}"#;
    // The output node drives the demand chain (output pulls control →
    // fixture pulls visual → shader renders/compiles); the memory output
    // provider accepts any authored endpoint.
    let output_json = r#"{
  "kind": "Output",
  "ports": {
    "0": {
      "endpoint": "ws281x:local:D10"
    }
  },
  "bindings": {
    "input": { "source": "bus:control.out" }
  }
}"#;
    let project_json = "{\n  \"format\": 10\n}\n";
    let module_json = r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "shader": { "ref": "./shader.json" },
    "pixels": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#;
    let clock_json = r#"{
  "kind": "Clock",
  "transport": {
    "play_state": "playing",
    "rate": 1.0
  }
}"#;
    let files: &[(&str, &str)] = &[
        ("project.json", project_json),
        ("module.json", module_json),
        ("clock.json", clock_json),
        ("shader.json", shader_json),
        ("fixture.json", fixture_json),
        ("output.json", output_json),
        ("shader.glsl", ASSET_SHADER_V1),
    ];
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(format!("{PROJECT_DIR}/{name}").as_path(), body.as_bytes())
            .expect("write project file");
    }
    server
        .load_project(PROJECT_DIR.as_path())
        .expect("load asset-e2e project");
    server.advance_frame(16).expect("tick");
    server
}

const PROJECT_DIR: &str = "/projects/edit-e2e";

/// A real server with a loaded clock + fixture project (no shader, so the
/// simulator session runs entirely host-side).
/// A "device" fixture: a real in-process server with NOTHING loaded.
/// Connect-time pulls discover the device's LOADED project, so device
/// tests must not run the edit-e2e project — an idle device falls back to
/// the default storage slot, an empty one classifies Empty.
pub(crate) fn device_e2e_server() -> LpServer {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "projects".as_path(),
        None,
        None,
        graphics,
    )
}

pub(crate) fn edit_e2e_server() -> LpServer {
    let mut server = device_e2e_server();

    for (name, body) in edit_e2e_files() {
        server
            .base_fs_mut()
            .write_file(format!("{PROJECT_DIR}/{name}").as_path(), body.as_bytes())
            .expect("write project file");
    }
    server
        .load_project(PROJECT_DIR.as_path())
        .expect("load edit-e2e project");
    server.advance_frame(16).expect("tick");
    server
}

pub(crate) fn edit_e2e_files() -> &'static [(&'static str, &'static str)] {
    &[
        ("project.json", "{\n  \"format\": 10\n}\n"),
        (
            "module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "pixels": { "ref": "./fixture.json" }
  }
}"#,
        ),
        (
            "clock.json",
            r#"{
  "kind": "Clock",
  "transport": {
    "play_state": "playing",
    "rate": 1.0
  }
}"#,
        ),
        (
            "fixture.json",
            r#"{
  "kind": "Fixture",
  "render_size": { "width": 10, "height": 10 },
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  }
}"#,
        ),
    ]
}

fn read_project_file(server: &Rc<RefCell<LpServer>>, name: &str) -> String {
    let bytes = server
        .borrow()
        .base_fs()
        .read_file(format!("{PROJECT_DIR}/{name}").as_path())
        .expect("read project file");
    // Normalize whitespace so assertions are formatting-independent.
    String::from_utf8(bytes)
        .expect("utf8 project file")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

pub(crate) fn project_action(op: ProjectOp) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        op,
    ))
}

fn set_value_action(address: crate::ProjectSlotAddress, value: LpValue) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::SetValue { address, value },
    ))
}

fn revert_action(address: crate::ProjectSlotAddress) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::Revert { address },
    ))
}

/// The per-value scope of the Clear verb (D7) — same mechanism as
/// `revert_action`, the vocabulary debug slots use.
fn clear_action(address: crate::ProjectSlotAddress) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::Clear { address },
    ))
}

fn ensure_present_action(address: crate::ProjectSlotAddress) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::EnsurePresent { address },
    ))
}

fn remove_value_action(address: crate::ProjectSlotAddress) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::RemoveValue { address },
    ))
}

/// An address under the same node and slot root as `base`, at `path`.
fn child_address(base: &crate::ProjectSlotAddress, path: &str) -> crate::ProjectSlotAddress {
    crate::ProjectSlotAddress::new(
        base.node.clone(),
        base.root.clone(),
        SlotPath::parse(path).unwrap(),
    )
}

fn count_mutations(sent: &Rc<RefCell<Vec<ClientMessage>>>) -> usize {
    sent.borrow()
        .iter()
        .filter(|message| {
            matches!(
                &message.msg,
                ClientRequest::ProjectCommand {
                    command: WireProjectCommand::MutateOverlay { .. },
                    ..
                }
            )
        })
        .count()
}

fn count_overlay_reads(sent: &Rc<RefCell<Vec<ClientMessage>>>) -> usize {
    sent.borrow()
        .iter()
        .filter(|message| {
            matches!(
                &message.msg,
                ClientRequest::ProjectCommand {
                    command: WireProjectCommand::ReadOverlay { .. },
                    ..
                }
            )
        })
        .count()
}

/// Every workspace card, root first and then depth-first through the nested
/// cards, each promoted exactly the way the renderer promotes it
/// ([`crate::UiNodeChild::into_node_view`]).
///
/// Since the flat-root reversal the editor carries ONE top-level card — the
/// root module — and every other node is a `UiNodeChild` beneath it, so a
/// scan over `editor.nodes` alone would only ever see the project root.
pub(crate) fn workspace_cards(view: &UiStudioView) -> Vec<UiNodeView> {
    fn walk(card: UiNodeView, out: &mut Vec<UiNodeView>) {
        let children = card.children.clone();
        out.push(card);
        for child in children {
            walk(child.into_node_view(), out);
        }
    }
    let mut cards = Vec::new();
    for card in project_editor(view).nodes.iter().cloned() {
        walk(card, &mut cards);
    }
    cards
}

/// The one workspace card matching `pick`, anywhere in the nested card tree.
pub(crate) fn card_matching(
    view: &UiStudioView,
    what: &str,
    pick: impl Fn(&UiNodeView) -> bool,
) -> UiNodeView {
    let cards = workspace_cards(view);
    cards
        .iter()
        .find(|card| pick(card))
        .unwrap_or_else(|| {
            panic!(
                "workspace carries a {what} card; got {:?}",
                cards
                    .iter()
                    .map(|card| (card.header.kind.clone(), card.header.path.clone()))
                    .collect::<Vec<_>>()
            )
        })
        .clone()
}

/// The workspace card at `path` (a node address).
pub(crate) fn card_at(view: &UiStudioView, path: &str) -> UiNodeView {
    card_matching(view, path, |card| card.header.path == path)
}

/// The first clock face's transport block anywhere in the workspace —
/// since the tape face claimed the clock's Debug rows (clock-tape-hero
/// P5), this is the transport's ONLY read surface: no slot row renders
/// its values anywhere.
pub(crate) fn clock_transport_block(view: &UiStudioView) -> crate::UiClockTransport {
    let card = card_matching(view, "clock-faced", |card| {
        matches!(card.face, Some(crate::UiNodeFace::Clock(_)))
    });
    let Some(crate::UiNodeFace::Clock(face)) = card.face else {
        unreachable!("picked by face kind");
    };
    face.transport
        .expect("clock face carries the transport block")
}

/// The project editor DTO from a studio snapshot.
pub(crate) fn project_editor(view: &UiStudioView) -> &crate::ProjectEditorView {
    view.panes
        .iter()
        .find_map(|pane| match &pane.body {
            UiViewContent::ProjectEditor(editor) => Some(&**editor),
            _ => None,
        })
        .expect("project editor pane")
}

/// The editor DTO's dirty counts as `(persisted, failed)`. There is no debug
/// bucket (D7): a debug override never enters the summary at all.
pub(crate) fn editor_dirty(view: &UiStudioView) -> (usize, usize) {
    let editor = project_editor(view);
    (editor.dirty.persisted, editor.dirty.failed)
}

/// The main-tab sections of one workspace card, by node address.
pub(crate) fn node_sections(view: &UiStudioView, node_id: &str) -> Vec<UiNodeSection> {
    match &card_at(view, node_id).tabs[0].body {
        UiNodeTabBody::Sections(sections) => sections.clone(),
        UiNodeTabBody::Text { .. } => panic!("expected node sections"),
    }
}

/// Top-level row labels of the first section matching `pick` (empty when the
/// node renders no such section).
pub(crate) fn section_slot_labels(
    sections: &[UiNodeSection],
    pick: impl Fn(&UiNodeSection) -> bool,
) -> Vec<String> {
    sections
        .iter()
        .find(|section| pick(section))
        .map(|section| match section {
            UiNodeSection::ConfigSlots(slots)
            | UiNodeSection::DebugSlots(slots)
            | UiNodeSection::AssetSlots(slots) => {
                slots.iter().map(|slot| slot.label.clone()).collect()
            }
            _ => Vec::new(),
        })
        .unwrap_or_default()
}

/// Find a config slot anywhere in the editor DTO tree by its address path.
pub(crate) fn find_slot<'a>(view: &'a UiStudioView, path: &str) -> &'a UiConfigSlot {
    try_find_slot(view, path).unwrap_or_else(|| panic!("config slot with path {path} should exist"))
}

/// Like [`find_slot`], but `None` when no row carries the address path.
///
/// Walks the workspace cards (the root's child panes under the flat-root
/// model) and, for root-own slots, the project popup's `root_slots` rows.
fn try_find_slot<'a>(view: &'a UiStudioView, path: &str) -> Option<&'a UiConfigSlot> {
    let editor = project_editor(view);

    fn in_slots<'a>(slots: &'a [UiConfigSlot], path: &str) -> Option<&'a UiConfigSlot> {
        for slot in slots {
            if slot
                .address
                .as_ref()
                .is_some_and(|address| address.path.to_string() == path)
            {
                return Some(slot);
            }
            if let UiConfigSlotBody::Record(record) = &slot.body
                && let Some(found) = in_slots(&record.fields, path)
            {
                return Some(found);
            }
        }
        None
    }

    fn in_sections<'a>(sections: &'a [UiNodeSection], path: &str) -> Option<&'a UiConfigSlot> {
        sections.iter().find_map(|section| match section {
            UiNodeSection::ConfigSlots(slots)
            | UiNodeSection::AssetSlots(slots)
            | UiNodeSection::DebugSlots(slots) => in_slots(slots, path),
            _ => None,
        })
    }

    fn in_children<'a>(children: &'a [crate::UiNodeChild], path: &str) -> Option<&'a UiConfigSlot> {
        children.iter().find_map(|child| {
            in_sections(&child.sections, path).or_else(|| in_children(&child.children, path))
        })
    }

    editor
        .nodes
        .iter()
        .find_map(|node| {
            node.tabs
                .iter()
                .find_map(|tab| match &tab.body {
                    UiNodeTabBody::Sections(sections) => in_sections(sections, path),
                    _ => None,
                })
                .or_else(|| in_children(&node.children, path))
        })
        .or_else(|| in_slots(&editor.root_slots, path))
}

pub(crate) fn slot_value_display(slot: &UiConfigSlot) -> &str {
    let UiConfigSlotBody::Value(value) = &slot.body else {
        panic!("expected a value body for {}", slot.label);
    };
    &value.display
}

fn slot_editor_hint(slot: &UiConfigSlot) -> &UiSlotEditorHint {
    let UiConfigSlotBody::Value(value) = &slot.body else {
        panic!("expected a value body for {}", slot.label);
    };
    &value.editor
}

/// `ClientIo` that pumps each client message through the in-process server's
/// `tick_and_send` and queues the produced frames for `receive`.
pub(crate) struct InProcessServerIo {
    pub(crate) server: Rc<RefCell<LpServer>>,
    pub(crate) inbox: Rc<RefCell<VecDeque<WireServerMessage>>>,
    pub(crate) sent: Rc<RefCell<Vec<ClientMessage>>>,
}

impl ClientIo for InProcessServerIo {
    fn send<'life0, 'async_trait>(
        &'life0 mut self,
        msg: ClientMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        self.sent.borrow_mut().push(msg.clone());
        let server = Rc::clone(&self.server);
        let inbox = Rc::clone(&self.inbox);
        Box::pin(async move {
            let mut transport = CollectTransport::default();
            server
                .borrow_mut()
                .tick_and_send(16, vec![WireMessage::Client(msg)], &mut transport)
                .await
                .map_err(|error| TransportError::Other(format!("server error: {error}")))?;
            inbox.borrow_mut().extend(transport.sent);
            Ok(())
        })
    }

    fn receive<'life0, 'async_trait>(
        &'life0 mut self,
    ) -> Pin<Box<dyn Future<Output = Result<WireServerMessage, TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let response = self
            .inbox
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| TransportError::Other("in-process server inbox empty".to_string()));
        Box::pin(async move { response })
    }

    fn close<'life0, 'async_trait>(
        &'life0 mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { Ok(()) })
    }
}

/// In-memory server transport that records every sent frame.
#[derive(Default)]
struct CollectTransport {
    sent: Vec<WireServerMessage>,
}

impl ServerTransport for CollectTransport {
    async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
        self.sent.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        Ok(Vec::new())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

/// The bench board's factory MAC for the device-identity rows.
const SILICON_MAC: &str = "60:55:f9:0a:0b:0c";

/// A board whose hello carries its factory base MAC (rule A1).
fn esp_ready_state(mac: &str) -> lpa_link::DeviceState {
    let mut state = crate::app::runtime_pool::runtime_session::ready_state_for_test();
    if let lpa_link::DeviceState::Ready { hello } = &mut state {
        hello.hardware.base_mac = Some(mac.to_string());
    }
    state
}

/// The uid `mac` derives to, through the production derivation — the rows
/// assert the RELATIONSHIP, never a hand-copied string.
fn silicon_uid(mac: &str) -> String {
    crate::app::places::HardwareId::from_base_mac(mac)
        .expect("a well-formed MAC")
        .device_uid()
        .to_string()
}

/// Drive a future to completion with a self-waking waker (bounded, so a hung
/// future fails the test instead of the suite).
pub(crate) fn drive<F: Future>(future: F) -> F::Output {
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    for _ in 0..100_000 {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
    panic!("e2e future did not complete");
}
