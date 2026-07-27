//! End-to-end node create/remove flow against an in-process LightPlayer
//! server (authoring P4).
//!
//! Reuses the edit-e2e harness (`InProcessServerIo`, `drive`,
//! `project_action`): a real `LpServer` loads the clock + fixture project and
//! the studio actor drives the same command path the web shell uses —
//! [`NodeCreateOp`] for every picker kind (project root and playlist attach
//! sites), [`NodeRemoveOp`] with save-panel rows, row revert, and save
//! materializing the staged deletion on disk.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use lpc_model::{AsLpPath, NodeKind};
use lpfs::LpFsMemory;

use crate::app::studio::studio_edit_e2e_tests::{
    InProcessServerIo, drive, edit_e2e_files, edit_e2e_server, project_action,
};
use crate::{
    ControllerId, NodeCreateOp, NodeRemoveOp, ProjectController, ProjectOp, StudioActor,
    StudioCommand, StudioController, StudioServerClient, UiAction, UiAttachTarget,
    UiPendingEditKind, UiStudioView, UiViewContent,
};

/// The edit-e2e project's storage dir on the in-process server.
const PROJECT_DIR: &str = "/projects/edit-e2e";

#[test]
fn create_every_picker_kind_lands_in_tree_and_on_disk() {
    let (server, mut actor, handle) = connected_actor();
    let mut view = handle.view;
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    assert_eq!(project_editor(&snapshot).nodes.len(), 2, "clock + fixture");

    // (kind, expected auto-name, expected tree ty). `clock` and `fixture`
    // collide with the fixture project's existing key/file → `_2` dedup.
    let cases: &[(NodeKind, &str, &str)] = &[
        (NodeKind::Shader, "shader", "shader"),
        (NodeKind::Texture, "texture", "texture"),
        (NodeKind::Playlist, "playlist", "playlist"),
        (NodeKind::Clock, "clock_2", "clock"),
        (NodeKind::Fixture, "fixture_2", "fixture"),
        (NodeKind::Output, "output", "output"),
        (NodeKind::Fluid, "fluid", "fluid"),
        (NodeKind::ComputeShader, "compute_shader", "compute_shader"),
        (NodeKind::Button, "button", "button"),
        (NodeKind::ControlRadio, "radio", "control_radio"),
    ];
    for (kind, name, ty) in cases {
        handle
            .tx
            .send(create_action(*kind, UiAttachTarget::ProjectRoot));
        drive(actor.run_one_batch_for_test());
        let snapshot = view.try_recv().expect("create emits a snapshot");

        // Def file on disk (commit-immediate), tree DTO carries the node.
        assert!(
            file_exists(&server, &format!("{name}.json")),
            "{kind:?}: def file {name}.json written"
        );
        let suffix = format!("/{name}.{ty}");
        assert!(
            project_editor(&snapshot)
                .nodes
                .iter()
                .any(|node| node.node_id.ends_with(&suffix)),
            "{kind:?}: node card {suffix} present, got {:?}",
            project_editor(&snapshot)
                .nodes
                .iter()
                .map(|node| node.node_id.clone())
                .collect::<Vec<_>>()
        );
        // The created node takes focus (the user lands on what they made).
        assert!(
            project_editor(&snapshot)
                .nodes
                .iter()
                .any(|node| node.node_id.ends_with(&suffix) && node.focused),
            "{kind:?}: the created node is focused"
        );
    }

    // The shader starter is a two-file create: scaffold GLSL beside the def,
    // and the def references it.
    assert!(
        file_exists(&server, "shader.glsl"),
        "shader scaffold exists"
    );
    let shader_def = read_file(&server, "shader.json");
    assert!(
        shader_def.contains("shader.glsl"),
        "shader def references its scaffold: {shader_def}"
    );
    // Project root gained every key (the collision case as `clock_2`).
    let manifest = read_file(&server, "project.json");
    for (_, name, _) in cases {
        assert!(
            manifest.contains(&format!("\"{name}\"")),
            "project.json carries {name}: {manifest}"
        );
    }
}

#[test]
fn create_into_playlist_adds_entry_and_child() {
    let (server, mut actor, handle) = connected_actor();
    let mut view = handle.view;
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv().expect("connect emits a snapshot");

    // A fresh playlist at the project root…
    handle.tx.send(create_action(
        NodeKind::Playlist,
        UiAttachTarget::ProjectRoot,
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("playlist create emits a snapshot");
    let playlist_id = project_editor(&snapshot)
        .nodes
        .iter()
        .find(|node| node.node_id.contains(".playlist"))
        .expect("playlist card present")
        .node_id
        .clone();
    // …offers the add-node picker on its card (playlist attach site).
    let playlist_card = project_editor(&snapshot)
        .nodes
        .iter()
        .find(|node| node.node_id == playlist_id)
        .unwrap();
    let menu = playlist_card
        .add_node_menu
        .as_ref()
        .expect("playlist cards carry the add-node picker");
    let entry = menu
        .entries
        .iter()
        .find(|entry| entry.kind == NodeKind::Texture)
        .expect("texture entry offered");

    // Create into the playlist by dispatching the picker entry's own action
    // (pane grammar: the controller-produced action is the whole gesture).
    handle.tx.send(StudioCommand::Action(entry.action.clone()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("entry create emits a snapshot");

    // The playlist def gained `entries[1]` (entries are 1-based so the first
    // one lands on the bare default's idle key) referencing the new def
    // file, and the child node mounted under the playlist in the tree.
    let playlist_def = read_file(&server, "playlist.json");
    assert!(
        playlist_def.contains("texture.json"),
        "playlist entry references the created def: {playlist_def}"
    );
    assert!(
        file_exists(&server, "texture.json"),
        "the entry's def file exists"
    );
    let tree = &project_editor(&snapshot).tree;
    let playlist_item = tree
        .roots
        .iter()
        .flat_map(|root| root.children.iter().chain(core::iter::once(root)))
        .find(|item| item.node_id == playlist_id)
        .expect("playlist in the tree");
    assert!(
        playlist_item
            .children
            .iter()
            .any(|child| child.node_id.contains("entry_1")),
        "the created entry's child mounted under the playlist: {:?}",
        playlist_item
            .children
            .iter()
            .map(|child| child.node_id.clone())
            .collect::<Vec<_>>()
    );

    // Removing the entry via its child card's delete action stages the
    // playlist-site removal as a labeled NodeRemoved row (`entries[<k>]`),
    // same presentation as a root-child removal.
    let entry_child = project_editor(&snapshot)
        .nodes
        .iter()
        .flat_map(|node| node.children.iter())
        .find(|child| child.detail.contains("entry_1"))
        .expect("entry child card present");
    let delete = entry_child
        .header_actions
        .iter()
        .find(|action| action.icon == "remove")
        .expect("entry child offers the delete action")
        .action
        .clone();
    handle.tx.send(StudioCommand::Action(delete));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("entry remove emits a snapshot");
    let editor = project_editor(&snapshot);
    let removed_row = editor
        .pending_edits
        .iter()
        .find(|edit| edit.kind == UiPendingEditKind::NodeRemoved)
        .unwrap_or_else(|| {
            panic!(
                "playlist-site removal lists as a NodeRemoved row: {:#?}",
                editor.pending_edits
            )
        });
    assert_eq!(removed_row.slot_path_display, "entries[1]");
    assert!(
        editor.pending_edits.iter().any(|edit| {
            matches!(&edit.kind, UiPendingEditKind::AssetBody { detail } if detail == "deleted")
                && edit.slot_path_display == "/texture.json"
        }),
        "the entry's def stages as a deleted file row: {:?}",
        editor.pending_edits
    );

    // Adding again BEFORE saving must work: the base file still holds the
    // staged-removed `entries[1]`, so the next create skips to `entries[2]`
    // (a create at 1 would reject as TargetOccupied — review-found bug).
    let playlist_card = project_editor(&snapshot)
        .nodes
        .iter()
        .find(|node| node.node_id == playlist_id)
        .expect("playlist card still present");
    let entry = playlist_card
        .add_node_menu
        .as_ref()
        .expect("playlist still offers the picker")
        .entries
        .iter()
        .find(|entry| entry.kind == NodeKind::Clock)
        .expect("clock entry offered");
    handle.tx.send(StudioCommand::Action(entry.action.clone()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("re-add emits a snapshot");
    let playlist_def = read_file(&server, "playlist.json");
    assert!(
        playlist_def.contains("\"2\"") && playlist_def.contains("clock_2.json"),
        "the re-added entry landed in the base def at key 2: {playlist_def}"
    );
    assert!(
        project_editor(&snapshot)
            .nodes
            .iter()
            .flat_map(|node| node.children.iter())
            .any(|child| child.detail.contains("entry_2")),
        "the re-added entry mounted as entry_2 (staged entries[1] skipped): {:?}",
        project_editor(&snapshot)
            .nodes
            .iter()
            .flat_map(|node| node.children.iter())
            .map(|child| child.detail.clone())
            .collect::<Vec<_>>()
    );
    assert!(
        project_editor(&snapshot)
            .pending_edits
            .iter()
            .any(|edit| edit.kind == UiPendingEditKind::NodeRemoved),
        "the staged removal of entries[1] survives the re-add"
    );
}

#[test]
fn remove_stages_rows_revert_restores_and_save_deletes_on_disk() {
    let (server, mut actor, handle) = connected_actor();
    let mut view = handle.view;
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let clock_id = project_editor(&snapshot)
        .nodes
        .iter()
        .find(|node| node.node_id.ends_with("/clock.clock"))
        .expect("clock card")
        .node_id
        .clone();

    // The clock card offers the ungated delete action with confirmation.
    let delete = project_editor(&snapshot)
        .nodes
        .iter()
        .find(|node| node.node_id == clock_id)
        .unwrap()
        .header_actions
        .iter()
        .find(|action| action.icon == "remove")
        .expect("delete header action")
        .action
        .clone();
    assert!(delete.meta().confirmation.is_some());
    assert!(delete.op_as::<NodeRemoveOp>().is_some());

    // Remove: the node leaves the tree, the save panel lists the NodeRemoved
    // row plus the staged file deletion, and nothing is deleted on disk yet.
    handle.tx.send(StudioCommand::Action(delete.clone()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("remove emits a snapshot");
    let editor = project_editor(&snapshot);
    assert!(
        !editor.nodes.iter().any(|node| node.node_id == clock_id),
        "the removed node left the workspace"
    );
    let removed_row = editor
        .pending_edits
        .iter()
        .find(|edit| edit.kind == UiPendingEditKind::NodeRemoved)
        .expect("NodeRemoved row listed");
    assert_eq!(removed_row.node_label, "Clock");
    assert_eq!(removed_row.slot_path_display, "nodes[clock]");
    assert!(
        editor.pending_edits.iter().any(|edit| {
            matches!(&edit.kind, UiPendingEditKind::AssetBody { detail } if detail == "deleted")
                && edit.slot_path_display == "/clock.json"
        }),
        "the staged deletion lists as a deleted file row: {:?}",
        editor.pending_edits
    );
    assert_eq!(
        editor.dirty.persisted,
        editor
            .pending_edits
            .iter()
            .filter(|edit| matches!(edit.phase, crate::UiPendingEditPhase::Persisted))
            .count(),
        "rows and dirty counts stay consistent"
    );
    assert!(
        file_exists(&server, "clock.json"),
        "staged removal deletes nothing before save"
    );

    // Revert from the row: the node comes back whole.
    let revert = removed_row.revert.clone().expect("row revert offered");
    handle.tx.send(StudioCommand::Action(revert));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert + refresh emit a snapshot");
    let editor = project_editor(&snapshot);
    assert!(
        editor.nodes.iter().any(|node| node.node_id == clock_id),
        "revert restored the node"
    );
    assert!(
        editor.pending_edits.is_empty(),
        "the staged removal's rows are gone: {:?}",
        editor.pending_edits
    );

    // Remove again and SAVE: the deletion materializes on disk.
    handle.tx.send(StudioCommand::Action(delete));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv().expect("second remove emits a snapshot");
    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("save emits a snapshot");
    assert!(
        !file_exists(&server, "clock.json"),
        "save deleted the removed node's def file"
    );
    let manifest = read_file(&server, "project.json");
    assert!(
        !manifest.contains("clock"),
        "project.json no longer references the removed node: {manifest}"
    );
    assert!(
        project_editor(&snapshot).pending_edits.is_empty(),
        "nothing stays pending after the save"
    );
}

#[test]
fn create_records_a_saved_library_event_and_copies_match() {
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
        Rc::new(|| [6u8; 16]),
        Rc::new(|| "2026-07-27-1200".to_string()),
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
        Rc::new(|| 2.0),
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
    let _ = view.try_recv().expect("open emits a snapshot");
    let events_before = store
        .open(summary.uid)
        .expect("library package opens")
        .history
        .events()
        .len();

    // Create a texture: creation commits server-side and the save-pull lands
    // it in the library as a Saved event.
    handle.tx.send(create_action(
        NodeKind::Texture,
        UiAttachTarget::ProjectRoot,
    ));
    let outcome = drive(actor.run_one_batch_for_test());
    let _ = outcome; // batch outcome surfaces through the snapshot below
    let snapshot = view.try_recv().expect("create emits a snapshot");
    assert!(
        !snapshot_mentions_library_warning(&snapshot),
        "no library-divergence warning after the create"
    );

    let library = store.open(summary.uid).expect("library package re-opens");
    assert!(
        library.history.events().len() > events_before,
        "the creation recorded a library history event"
    );
    let library_texture = library
        .package_fs
        .borrow()
        .read_file("/texture.json".as_path())
        .expect("library copy gained texture.json");
    let runtime_texture = server
        .borrow()
        .base_fs()
        .read_file("/projects/studio/texture.json".as_path())
        .expect("runtime holds texture.json");
    assert_eq!(
        library_texture, runtime_texture,
        "library copy byte-matches the runtime (hash tripwire held)"
    );
}

// --- helpers -----------------------------------------------------------------

/// The tests' no-op quiet-gap timer factory, as a nameable fn-pointer type so
/// the harness helper can return the concrete `StudioActor` generic.
type TestTimer = fn(core::time::Duration) -> core::future::Ready<()>;

fn test_timer(_: core::time::Duration) -> core::future::Ready<()> {
    core::future::ready(())
}

fn connected_actor() -> (
    Rc<RefCell<lpa_server::LpServer>>,
    StudioActor<TestTimer>,
    crate::StudioHandle,
) {
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (actor, handle) = StudioActor::new(controller, test_timer as TestTimer);
    (server, actor, handle)
}

fn create_action(kind: NodeKind, attach: UiAttachTarget) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        NodeCreateOp { kind, attach },
    ))
}

fn project_editor(view: &UiStudioView) -> &crate::ProjectEditorView {
    view.panes
        .iter()
        .find_map(|pane| match &pane.body {
            UiViewContent::ProjectEditor(editor) => Some(&**editor),
            _ => None,
        })
        .expect("project editor pane")
}

fn file_exists(server: &Rc<RefCell<lpa_server::LpServer>>, name: &str) -> bool {
    server
        .borrow()
        .base_fs()
        .file_exists(format!("{PROJECT_DIR}/{name}").as_path())
        .unwrap_or(false)
}

fn read_file(server: &Rc<RefCell<lpa_server::LpServer>>, name: &str) -> String {
    let bytes = server
        .borrow()
        .base_fs()
        .read_file(format!("{PROJECT_DIR}/{name}").as_path())
        .expect("read project file");
    String::from_utf8(bytes).expect("utf8 project file")
}

fn snapshot_mentions_library_warning(view: &UiStudioView) -> bool {
    // The save-pull tripwire surfaces as a warning notice; notices ride the
    // dispatch outcome (not the snapshot), so check the console ring text.
    view.console
        .entries
        .iter()
        .any(|entry| entry.message.contains("differs from the running project"))
}
