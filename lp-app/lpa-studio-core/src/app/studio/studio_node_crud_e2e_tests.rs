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
    InProcessServerIo, drive, edit_e2e_files, edit_e2e_server, project_action, workspace_cards,
};
use crate::{
    ControllerId, NodeCopyOp, NodeCreateOp, NodePasteOp, NodeRemoveOp, ProjectController,
    ProjectNodeAddress, ProjectOp, StudioActor, StudioCommand, StudioController,
    StudioServerClient, UiAction, UiAttachTarget, UiPendingEditKind, UiStudioView, UiViewContent,
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
    // Root card restored: the workspace has one top card (the root module)
    // and the clock and fixture ride beneath it.
    assert_eq!(child_card_paths(&snapshot).len(), 2, "clock + fixture");

    // (kind, expected auto-name, expected tree ty). `clock` and `fixture`
    // collide with the fixture project's existing key/file → `_2` dedup.
    let cases: &[(NodeKind, &str, &str)] = &[
        (NodeKind::Shader, "shader", "shader"),
        (NodeKind::Texture, "texture", "texture"),
        (NodeKind::Playlist, "playlist", "playlist"),
        // An embedded module (settled D-C): an empty child def whose node
        // introduces a scope, creatable like anything else.
        (NodeKind::Module, "module_2", "module"),
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
            child_card_paths(&snapshot)
                .iter()
                .any(|path| path.ends_with(&suffix)),
            "{kind:?}: node card {suffix} present, got {:?}",
            child_card_paths(&snapshot)
        );
        // The created node takes focus (the user lands on what they made).
        assert!(
            workspace_cards(&snapshot)
                .iter()
                .any(|card| card.header.path.ends_with(&suffix) && card.focused),
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
    // The fixture starter is a two-file create too: a default mapping
    // document beside the def, referenced as a Map2d mapping — a new
    // fixture is immediately viewable and editable in place.
    assert!(
        file_exists(&server, "fixture_2.map2d.json"),
        "fixture mapping document exists"
    );
    let fixture_def = read_file(&server, "fixture_2.json");
    assert!(
        fixture_def.contains("Map2d") && fixture_def.contains("fixture_2.map2d.json"),
        "fixture def references its mapping document: {fixture_def}"
    );
    // The root module gained every key (the collision case as `clock_2`).
    let module = read_file(&server, "module.json");
    for (_, name, _) in cases {
        assert!(
            module.contains(&format!("\"{name}\"")),
            "module.json carries {name}: {module}"
        );
    }

    // The created embedded module wears the module face: an empty panel of
    // its own scope, nested as a group on the root's panel would be once it
    // has channels — for now the card itself is the assertion.
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    let created = workspace_cards(&snapshot)
        .into_iter()
        .find(|card| card.header.path.ends_with("/module_2.module"))
        .expect("the embedded module card renders");
    assert_eq!(created.header.kind, "Module");
    match &created.face {
        Some(crate::UiNodeFace::Module(face)) => {
            assert!(
                face.panel.is_empty(),
                "a fresh module has no public channels yet"
            );
            assert!(face.panel.target.is_some(), "its scope is real");
        }
        other => panic!("embedded module derives a module face, got {other:?}"),
    }
}

/// P5's Shape declaration moment (D13): a created FIXTURE surfaces the
/// guided card state; nothing else does; pasting decides by what the def
/// carries. The trigger is card-UI state resolved when the created
/// node's tree entry lands — the same overlay every drawer bit rides.
#[test]
fn the_shape_moment_follows_undeclared_fixture_births() {
    let (_server, mut actor, handle) = connected_actor();
    let mut view = handle.view;
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    view.try_recv().expect("connect emits a snapshot");

    let guided_of = |snapshot: &UiStudioView, suffix: &str| {
        workspace_cards(snapshot)
            .into_iter()
            .find(|card| card.header.path.ends_with(suffix))
            .unwrap_or_else(|| panic!("card {suffix} present"))
            .card_ui
            .shape_guided
    };

    // A created fixture is born undeclared: guided.
    handle.tx.send(create_action(
        NodeKind::Fixture,
        UiAttachTarget::ProjectRoot,
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("create emits a snapshot");
    assert!(
        guided_of(&snapshot, "/fixture_2.fixture"),
        "a created fixture surfaces the Shape moment"
    );
    // The pre-existing fixture is untouched: existing projects never see
    // the guided state.
    assert!(
        !guided_of(&snapshot, "/pixels.fixture"),
        "existing fixtures never see the guided state"
    );

    // A created shader gets nothing — the moment is the fixture's.
    handle
        .tx
        .send(create_action(NodeKind::Shader, UiAttachTarget::ProjectRoot));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("create emits a snapshot");
    assert!(!guided_of(&snapshot, "/shader.shader"));

    // Paste of a DECLARED fixture — a 1-row render area is the declared
    // strip idiom (`fixture_carries_2d_coords`): it carries its shape, so
    // no moment.
    let declared = crate::app::share::NodeEnvelope::encode(
        "worn",
        "./worn.json",
        br#"{ "kind": "Fixture", "render_size": { "width": 64, "height": 1 } }"#,
        &[],
    )
    .to_json()
    .expect("encode the declared-strip envelope");
    handle
        .tx
        .send(paste_action(&declared, UiAttachTarget::ProjectRoot));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("paste emits a snapshot");
    assert!(
        !guided_of(&snapshot, "/worn.fixture"),
        "a pasted fixture carries its declaration (a 1-row area is a \
         declared strip)"
    );

    // Paste of a BARE fixture def (older clipboard content: no mapping,
    // no 1-row area): no authored shape, so the moment fires.
    let bare = crate::app::share::NodeEnvelope::encode(
        "bare",
        "./bare.json",
        br#"{ "kind": "Fixture" }"#,
        &[],
    )
    .to_json()
    .expect("encode the bare fixture envelope");
    handle
        .tx
        .send(paste_action(&bare, UiAttachTarget::ProjectRoot));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("paste emits a snapshot");
    assert!(
        guided_of(&snapshot, "/bare.fixture"),
        "an undeclared paste surfaces the Shape moment"
    );

    // The moment resolves through the ordinary card-UI reducer: the web's
    // preset tiles and skip link dispatch exactly this op.
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        crate::ProjectEditorTarget::node_tree().node_id(),
        crate::ProjectEditorOp::NodeUi(crate::NodeUiOp::SetShapeGuided {
            node: workspace_cards(&snapshot)
                .into_iter()
                .find(|card| card.header.path.ends_with("/bare.fixture"))
                .expect("the pasted card")
                .header
                .path,
            guided: false,
        }),
    )));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("dismiss emits a snapshot");
    assert!(
        !guided_of(&snapshot, "/bare.fixture"),
        "a preset pick or dismiss clears the moment"
    );
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
    let playlist_id = child_card_paths(&snapshot)
        .into_iter()
        .find(|path| path.contains(".playlist"))
        .expect("playlist card present");
    // …offers the add-node picker on its card (playlist attach site). The
    // playlist is a NESTED card now, so the picker only survives because it
    // rides `UiNodeChild::add_node_menu`.
    let playlist_card = card_at(&snapshot, &playlist_id);
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
    let entry_child = workspace_cards(&snapshot)
        .into_iter()
        .find(|card| card.header.path.contains("entry_1"))
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
    let playlist_card = card_at(&snapshot, &playlist_id);
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
        child_card_paths(&snapshot)
            .iter()
            .any(|path| path.contains("entry_2")),
        "the re-added entry mounted as entry_2 (staged entries[1] skipped): {:?}",
        child_card_paths(&snapshot)
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
    let clock_id = child_card_paths(&snapshot)
        .into_iter()
        .find(|path| path.ends_with("/clock.clock"))
        .expect("clock card");

    // The clock card offers the ungated delete action with confirmation.
    let delete = card_at(&snapshot, &clock_id)
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
        !child_card_paths(&snapshot).contains(&clock_id),
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
        child_card_paths(&snapshot).contains(&clock_id),
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
#[test]
fn copy_then_paste_round_trips_a_shader_with_its_asset() {
    let (server, mut actor, handle, clipboard) = connected_actor_with_clipboard();
    let mut view = handle.view;
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    view.try_recv().expect("connect emits a snapshot");

    // Make something worth copying: the shader starter is the two-file
    // case (def + scaffold GLSL, with the def referencing the scaffold).
    handle
        .tx
        .send(create_action(NodeKind::Shader, UiAttachTarget::ProjectRoot));
    drive(actor.run_one_batch_for_test());
    view.try_recv().expect("create emits a snapshot");
    assert!(file_exists(&server, "shader.json"));
    assert!(file_exists(&server, "shader.glsl"));

    // -- copy ---------------------------------------------------------------
    handle.tx.send(copy_action("/edit_e2e.show/shader.shader"));
    drive(actor.run_one_batch_for_test());
    let envelope = clipboard
        .borrow()
        .clone()
        .expect("copy writes the envelope to the clipboard sink");
    let decoded = crate::NodeEnvelope::decode(&envelope).expect("a valid lp.node envelope");
    assert_eq!(decoded.assets.len(), 1, "the .glsl travels with the node");
    assert!(
        decoded
            .body_text()
            .expect("a def body is text")
            .contains("shader.glsl"),
        "the copied def still references its asset: {:?}",
        decoded.body_text()
    );

    // -- paste --------------------------------------------------------------
    // The source name is taken here, so the paste must land under a fresh
    // name AND repoint the def at the renamed asset.
    handle
        .tx
        .send(paste_action(&envelope, UiAttachTarget::ProjectRoot));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("paste emits a snapshot");

    assert!(
        file_exists(&server, "shader_2.json"),
        "the pasted def dodges the taken name"
    );
    assert!(
        file_exists(&server, "shader_2.glsl"),
        "and so does its asset"
    );
    let pasted_def = read_file(&server, "shader_2.json");
    assert!(
        pasted_def.contains("shader_2.glsl"),
        "the pasted def points at the RENAMED asset — a stale reference \
         would paste a node whose source file does not exist: {pasted_def}"
    );
    assert!(
        !pasted_def.contains("\"./shader.glsl\""),
        "and no longer at the original: {pasted_def}"
    );
    assert!(
        child_card_paths(&snapshot)
            .iter()
            .any(|path| path.ends_with("/shader_2.shader")),
        "the pasted node is in the tree"
    );
    // Both shaders' GLSL is byte-identical: paste copies content, not a
    // reference to it.
    assert_eq!(
        read_file(&server, "shader.glsl"),
        read_file(&server, "shader_2.glsl")
    );
}

#[test]
fn pasting_a_node_from_another_project_format_is_refused_out_loud() {
    // A bare node carries no project.json, so nothing can migrate it — and
    // the paste flow's silent-classification rule is about the CLIPBOARD
    // holding something else, not about a real `lp.node` that fails the
    // check. This one has to be audible, or a stale shader pastes and
    // fails later as what looks like a bug in the node.
    let (server, mut actor, handle, clipboard) = connected_actor_with_clipboard();
    let mut view = handle.view;
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    view.try_recv().expect("connect emits a snapshot");

    handle
        .tx
        .send(create_action(NodeKind::Shader, UiAttachTarget::ProjectRoot));
    drive(actor.run_one_batch_for_test());
    view.try_recv().expect("create emits a snapshot");

    handle.tx.send(copy_action("/edit_e2e.show/shader.shader"));
    drive(actor.run_one_batch_for_test());
    let envelope = clipboard.borrow().clone().expect("copied");

    // Age the stamp: the same envelope, as an older Studio would have
    // written it.
    let stale = {
        let mut value: serde_json::Value = serde_json::from_str(&envelope).unwrap();
        value["artifact_format"] = serde_json::json!(lpc_model::PROJECT_FORMAT_VERSION - 1);
        value.to_string()
    };

    handle
        .tx
        .send(paste_action(&stale, UiAttachTarget::ProjectRoot));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the refused paste emits a snapshot");

    assert!(
        !file_exists(&server, "shader_2.json"),
        "a refused paste must create nothing"
    );
    let refusal = snapshot
        .console
        .entries
        .iter()
        .find(|entry| entry.message.contains("Cannot paste"))
        .unwrap_or_else(|| {
            panic!(
                "the refusal must reach the user: {:?}",
                snapshot
                    .console
                    .entries
                    .iter()
                    .map(|entry| &entry.message)
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        refusal.message.contains("re-copy"),
        "and it must name the remedy: {}",
        refusal.message
    );
}

#[test]
fn pasting_a_node_into_a_playlist_lands_as_an_entry() {
    let (server, mut actor, handle, clipboard) = connected_actor_with_clipboard();
    let mut view = handle.view;
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    view.try_recv().expect("connect emits a snapshot");

    handle.tx.send(create_action(
        NodeKind::Playlist,
        UiAttachTarget::ProjectRoot,
    ));
    drive(actor.run_one_batch_for_test());
    view.try_recv().expect("create emits a snapshot");

    handle.tx.send(copy_action("/edit_e2e.show/clock.clock"));
    drive(actor.run_one_batch_for_test());
    let envelope = clipboard.borrow().clone().expect("copied");

    let playlist = ProjectNodeAddress::parse("/edit_e2e.show/playlist.playlist")
        .expect("valid playlist address");
    handle.tx.send(paste_action(
        &envelope,
        UiAttachTarget::Playlist { node: playlist },
    ));
    drive(actor.run_one_batch_for_test());
    view.try_recv().expect("paste emits a snapshot");

    let playlist_def = read_file(&server, "playlist.json");
    assert!(
        playlist_def.contains("entries"),
        "the pasted node became a playlist entry: {playlist_def}"
    );
}

#[test]
fn pasting_junk_reports_it_and_changes_nothing() {
    let (server, mut actor, handle) = connected_actor();
    let mut view = handle.view;
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let before = child_card_paths(&view.try_recv().expect("connect")).len();

    for junk in [
        "not json",
        r#"{"kind":"lp.package","format":1,"name":"x","files":{}}"#,
        r#"{"kind":"lp.node","format":99,"label":"x","file":"./a.json","body":{"text":"{}"},"assets":{}}"#,
    ] {
        handle
            .tx
            .send(paste_action(junk, UiAttachTarget::ProjectRoot));
        drive(actor.run_one_batch_for_test());
        // A refused paste must not write anything or disturb the tree.
        assert!(!file_exists(&server, "node.json"), "{junk}");
    }

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let after = child_card_paths(&view.try_recv().expect("refresh")).len();
    assert_eq!(before, after, "a refused paste leaves the tree alone");
}

/// Does replace-as-remove-then-paste compose?
///
/// The two wire ops are deliberately asymmetric: `RemoveNode` **stages** in
/// the overlay (revertible until commit) while `CreateNode` **commits
/// immediately** (`ArtifactOverlay` is slot-XOR-asset, so a staged node body
/// would vanish on reload). This test is the empirical answer the plan
/// asked for before building a one-click Replace.
#[test]
fn replace_probe_remove_then_paste_at_the_same_key() {
    let (server, mut actor, handle, clipboard) = connected_actor_with_clipboard();
    let mut view = handle.view;
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    view.try_recv().expect("connect emits a snapshot");

    handle.tx.send(copy_action("/edit_e2e.show/clock.clock"));
    drive(actor.run_one_batch_for_test());
    let envelope = clipboard.borrow().clone().expect("copied");

    // Stage the removal of the node we are "replacing".
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        NodeRemoveOp {
            node: ProjectNodeAddress::parse("/edit_e2e.show/clock.clock").expect("address"),
        },
    )));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("remove emits a snapshot");
    let staged = project_editor(&snapshot).dirty.persisted;
    assert!(staged > 0, "the removal staged in the overlay");

    // Now paste into the hole. The key `clock` is staged-removed but the
    // BASE file still holds it until Save.
    handle
        .tx
        .send(paste_action(&envelope, UiAttachTarget::ProjectRoot));
    drive(actor.run_one_batch_for_test());
    view.try_recv().expect("paste emits a snapshot");

    // The finding, pinned so a future Replace implementation starts from
    // fact rather than assumption: the paste does NOT reuse the removed
    // key. `taken_node_names` counts overlay-staged names as used (the base
    // file still has them, so the server would reject a create there as
    // TargetOccupied), so the pasted node lands beside the staged removal
    // under a fresh name.
    assert!(
        file_exists(&server, "clock_2.json"),
        "paste lands under a FRESH name, not the staged-removed one"
    );
    assert!(
        file_exists(&server, "clock.json"),
        "the removed node's file is still on disk — removal only staged"
    );
    // So a one-click Replace cannot be remove-then-create as-is: it would
    // leave a committed new node beside a merely-staged removal, and
    // reverting the removal would resurrect the old node alongside the new
    // one. See the plan's P6 notes and the ADR follow-ups.
}

type TestTimer = fn(core::time::Duration) -> core::future::Ready<()>;

fn test_timer(_: core::time::Duration) -> core::future::Ready<()> {
    core::future::ready(())
}

fn connected_actor() -> (
    Rc<RefCell<lpa_server::LpServer>>,
    StudioActor<TestTimer>,
    crate::StudioHandle,
) {
    let (server, actor, handle, _) = connected_actor_with_clipboard();
    (server, actor, handle)
}

/// Same harness plus a capture cell standing in for the browser clipboard,
/// so a copy's envelope text is inspectable (core hands it to the injected
/// sink and never touches a real clipboard).
fn connected_actor_with_clipboard() -> (
    Rc<RefCell<lpa_server::LpServer>>,
    StudioActor<TestTimer>,
    crate::StudioHandle,
    Rc<RefCell<Option<String>>>,
) {
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);
    let clipboard = Rc::new(RefCell::new(None::<String>));
    let sink = Rc::clone(&clipboard);
    controller.set_on_copy_text(move |text| *sink.borrow_mut() = Some(text.to_string()));
    let (actor, handle) = StudioActor::new(controller, test_timer as TestTimer);
    (server, actor, handle, clipboard)
}

fn copy_action(node: &str) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        NodeCopyOp {
            node: ProjectNodeAddress::parse(node).expect("valid node address"),
        },
    ))
}

fn paste_action(envelope: &str, attach: UiAttachTarget) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        NodePasteOp {
            envelope: envelope.to_string(),
            attach,
        },
    ))
}

fn create_action(kind: NodeKind, attach: UiAttachTarget) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        NodeCreateOp { kind, attach },
    ))
}

/// The addresses of every card BELOW the root module card — the workspace
/// as it read before the flat-root reversal restored the root card.
fn child_card_paths(view: &UiStudioView) -> Vec<String> {
    workspace_cards(view)
        .into_iter()
        .skip(1)
        .map(|card| card.header.path)
        .collect()
}

/// The workspace card at a node address.
fn card_at(view: &UiStudioView, path: &str) -> crate::UiNodeView {
    workspace_cards(view)
        .into_iter()
        .find(|card| card.header.path == path)
        .unwrap_or_else(|| panic!("workspace carries a card at {path}"))
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
