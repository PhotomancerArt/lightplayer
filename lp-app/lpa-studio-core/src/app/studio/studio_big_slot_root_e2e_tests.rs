//! A slot root bigger than one read frame, end to end against a real server
//! (`docs/defects/2026-10-31-a-big-playlist-loses-the-panel-after-its-first-step.md`).
//!
//! The board is the PLAYFUL Choker Tryout (`catalog/projects/playful-choker-tryout`,
//! the project on Yona's "flower" C6) with its playlist grown to
//! [`ENTRIES`] entries, so the playlist's def is a slot root well past the
//! 16 KiB a read frame holds — as the 25-entry playlist's own root became
//! once its revision stamps grew a few digits on a board that had run a
//! while. Before the fix the board could not send that root at all: the read
//! that carried it ended in an error, the client dropped every slot root the
//! read had brought, and the card's panel lost its controls (the master
//! brightness first among them) until the board was reset.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpfs::LpFsMemory;

use crate::app::studio::studio_edit_e2e_tests::{
    InProcessServerIo, drive, project_action, project_editor,
};
use crate::{
    ProjectOp, StudioActor, StudioController, StudioServerClient, UiNodeFace, UiPanelGroup,
    UiPanelWidget, UiStudioView,
};

#[test]
fn a_playlist_bigger_than_a_frame_keeps_the_whole_panel() {
    let mut session = Session::connect(None);
    assert_whole_panel(&session.view);
    // The editor's reads after the first sync carry it too, whenever the
    // playlist's def changes.
    session.refresh(3);
    assert_whole_panel(&session.view);
    assert!(
        project_editor(&session.view).sync.issue.is_none(),
        "{:?}",
        project_editor(&session.view).sync.issue
    );
}

/// On a fragmented heap a read goes out in frames of half the largest free
/// block (`lpa_server::read_frame_budget`): here 8 KB frames, a quarter of
/// the playlist's root, which still arrives, a piece at a time.
#[test]
fn a_fragmented_board_still_sends_the_big_playlist() {
    let session = Session::connect(Some(16 * 1024));
    assert_whole_panel(&session.view);
}

/// Entries in the grown playlist: its def root is ~32 KB of wire JSON.
const ENTRIES: usize = 50;

fn assert_whole_panel(view: &UiStudioView) {
    let channels = card_channels(view);
    assert_eq!(
        channels.first().map(String::as_str),
        Some(crate::MASTER_CHANNEL),
        "the master brightness leads the card's panel: {channels:?}"
    );
    assert_eq!(
        pattern_entries(view),
        Some(ENTRIES),
        "the Pattern instrument lists every entry: {channels:?}"
    );
}

thread_local! {
    /// The largest free block the server's probe reports, when set.
    static LARGEST_BLOCK: Cell<Option<u32>> = const { Cell::new(None) };
}

fn largest_block() -> Option<u32> {
    LARGEST_BLOCK.with(Cell::get)
}

type NoTimer = fn(core::time::Duration) -> core::future::Ready<()>;

fn no_timer(_: core::time::Duration) -> core::future::Ready<()> {
    core::future::ready(())
}

struct Session {
    actor: StudioActor<NoTimer>,
    tx: crate::app::studio::studio_view_channel::CommandSender,
    views: crate::app::studio::studio_view_channel::StudioViewReceiver,
    view: UiStudioView,
}

impl Session {
    /// Connect to the grown tryout; `largest_block` is the board's largest
    /// free block, when it reports one.
    fn connect(largest_block: Option<u32>) -> Self {
        LARGEST_BLOCK.with(|cell| cell.set(largest_block));
        let io = InProcessServerIo {
            server: Rc::new(RefCell::new(tryout_server())),
            inbox: Rc::new(RefCell::new(VecDeque::new())),
            sent: Rc::new(RefCell::new(Vec::new())),
        };
        let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
        let controller = StudioController::connected_with_client_for_test(client);
        let (mut actor, handle) = StudioActor::new(controller, no_timer as NoTimer);
        let mut views = handle.view;
        handle
            .tx
            .send(project_action(ProjectOp::ConnectRunningProject));
        drive(actor.run_one_batch_for_test());
        let view = views.try_recv().expect("connect emits a snapshot");
        Self {
            actor,
            tx: handle.tx,
            views,
            view,
        }
    }

    /// `ticks` passive refreshes, as the editor's lens pulls.
    fn refresh(&mut self, ticks: usize) {
        for _ in 0..ticks {
            self.tx.send(project_action(ProjectOp::RefreshProject));
            drive(self.actor.run_one_batch_for_test());
            while let Some(view) = self.views.try_recv() {
                self.view = view;
            }
        }
    }
}

/// The root module card's panel.
fn root_panel(view: &UiStudioView) -> Option<UiPanelGroup> {
    match project_editor(view).nodes.first()?.face.as_ref()? {
        UiNodeFace::Module(face) => Some(face.panel.clone()),
        _ => None,
    }
}

/// The channels of the connected card's panel, in its order (the master
/// leading), picked from the root panel as the card picks it.
fn card_channels(view: &UiStudioView) -> Vec<String> {
    root_panel(view)
        .map(|panel| {
            crate::board_panel_picks(&panel, None)
                .controls()
                .map(|control| control.view.channel.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// How many entries the Pattern instrument lists, when it is drawn.
fn pattern_entries(view: &UiStudioView) -> Option<usize> {
    fn find(group: &UiPanelGroup) -> Option<usize> {
        group
            .controls
            .iter()
            .find_map(|control| match &control.control.widget {
                UiPanelWidget::PatternPicker { picker } => Some(picker.entries.len()),
                _ => None,
            })
            .or_else(|| group.groups.iter().find_map(find))
    }
    find(&root_panel(view)?)
}

const TRYOUT_DIR: &str = "/projects/tryout";

/// The choker tryout with its playlist grown to [`ENTRIES`] entries (the
/// catalog's 25, then the same patterns again under new names).
fn tryout_server() -> LpServer {
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
    server.set_read_headroom_probe(Some(largest_block));
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../catalog/projects/playful-choker-tryout");
    for path in files_under(&root) {
        let relative = path
            .strip_prefix(&root)
            .expect("under the root")
            .to_string_lossy()
            .into_owned();
        let mut bytes = std::fs::read(&path).expect("read a catalog file");
        if relative == "playlist.json" {
            bytes = grown_playlist(&bytes);
        }
        server
            .base_fs_mut()
            .write_file(format!("{TRYOUT_DIR}/{relative}").as_path(), &bytes)
            .expect("write project file");
    }
    server
        .load_project(TRYOUT_DIR.as_path())
        .expect("load the tryout");
    server.advance_frame(16).expect("tick");
    server
}

/// `playlist.json` with its entries repeated up to [`ENTRIES`].
fn grown_playlist(bytes: &[u8]) -> Vec<u8> {
    let mut playlist: serde_json::Value = serde_json::from_slice(bytes).expect("playlist JSON");
    let entries = playlist["entries"]
        .as_object_mut()
        .expect("the playlist's entries");
    let originals: Vec<serde_json::Value> = entries.values().cloned().collect();
    for key in originals.len() + 1..=ENTRIES {
        let mut entry = originals[(key - 1) % originals.len()].clone();
        let name = entry["name"].as_str().unwrap_or("pattern").to_string();
        entry["name"] = serde_json::Value::String(format!("{name}_{key}"));
        entries.insert(key.to_string(), entry);
    }
    // `kind` leads a def file; a re-serialized map would sort it later.
    let kind = playlist
        .as_object_mut()
        .and_then(|object| object.remove("kind"))
        .expect("the playlist's kind");
    let rest = serde_json::to_string(&playlist).expect("playlist JSON");
    format!("{{\"kind\":{kind},{}", &rest[1..]).into_bytes()
}

fn files_under(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("read the catalog dir") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out
}
