//! End-to-end Pattern instrument tests (multi-pattern plan P7) against an
//! in-process LightPlayer server.
//!
//! The fixture is shaped like the choker tryout: a root module holding a
//! clock, a fixture, an output and ONE playlist whose entries are pattern
//! MODULES (`modules/<name>/module.json`, each a module around a shader that
//! binds its own knob on `bus:tail`) — the shape `catalog/patterns/*`
//! imports into. Only the playing entry is loaded (plan PD1), so every other
//! entry is dormant and known to Studio by its def alone.
//!
//! What is proven here, on the real engine:
//!
//! - the Play-mode order: shared knobs, then the Pattern group, then the
//!   playing pattern's own knobs — and those knobs are the pattern MODULE's
//!   (director ruling 2);
//! - tap, next/prev, on/off and the tour switch all reach the device and
//!   read back;
//! - a pattern that fails to compile reads Failed, through the warning the
//!   engine writes with `lpc_model`'s formatter (director ruling 1).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_model::{AsLpPath, PlaylistTour};
use lpc_shared::output::MemoryOutputProvider;
use lpfs::LpFsMemory;

use crate::app::studio::studio_edit_e2e_tests::{
    InProcessServerIo, drive, project_action, project_editor,
};
use crate::{
    PanelWriteOp, PlaylistActivateOp, ProjectOp, StudioActor, StudioCommand, StudioController,
    StudioServerClient, UiAction, UiNodeFace, UiPanelGroup, UiPanelWidget, UiPatternEntryState,
    UiPatternPicker, UiStudioView,
};

#[test]
fn the_pattern_instrument_sits_between_the_shared_knobs_and_the_patterns_own() {
    let session = Session::connect();
    let view = session.view.clone();

    let panel = root_panel(&view);
    let labels: Vec<&str> = panel
        .groups
        .iter()
        .map(|group| group.label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec!["Clock", "Pattern", "Soft noise"],
        "shared (the clock's instrument, after the flat strip), then the \
         Pattern instrument, then the playing pattern's own knobs"
    );
    assert_eq!(
        panel
            .controls
            .iter()
            .map(|control| control.channel.as_str())
            .collect::<Vec<_>>(),
        vec!["brightness"],
        "the shared strip is unchanged: the fixture's promoted brightness"
    );

    let picker = picker(&view);
    assert_eq!(
        picker
            .entries
            .iter()
            .map(|entry| (entry.key, entry.name.as_str(), entry.state))
            .collect::<Vec<_>>(),
        vec![
            (1, "Soft noise", UiPatternEntryState::Playing),
            (2, "Aurora", UiPatternEntryState::Available),
            (3, "Scanner", UiPatternEntryState::Available),
            (4, "Broken", UiPatternEntryState::Available),
        ],
        "every entry by its display name (the authored node name, as its card \
         reads), dormant ones included, in key order"
    );
    assert_eq!(picker.active, Some(1));
    assert_eq!(picker.tour, PlaylistTour::Hold, "nothing authored: hold");
    let tour_target = picker
        .tour_target
        .clone()
        .expect("the tour is on a channel");
    assert_eq!(tour_target.channel, lpc_model::PLAYLIST_TOUR_CHANNEL);
    let skip_target = picker.skip_target.clone().expect("so is the skip list");
    assert_eq!(skip_target.channel, lpc_model::PLAYLIST_SKIP_CHANNEL);
    assert_eq!(
        tour_target.scope,
        root_panel(&view).target.expect("the root panel's scope"),
        "the playlist's channels live in the module that holds it"
    );

    // Ruling 2: the pattern's knobs are its MODULE's, not the entry sink's.
    let knobs = &panel.groups[2];
    assert_eq!(
        knobs
            .controls
            .iter()
            .map(|control| control.channel.as_str())
            .collect::<Vec<_>>(),
        vec!["tail"]
    );
    let scope = knobs.target.expect("the knobs' group resets its own scope");
    assert!(
        !scope.is_sink(),
        "the group is the pattern module's own scope, got {scope:?}"
    );
    assert_eq!(
        knobs.controls[0]
            .control
            .panel_target
            .as_ref()
            .map(|target| target.scope),
        Some(scope),
        "and its knob writes there"
    );
}

#[test]
fn the_pattern_instruments_gestures_drive_the_real_playlist() {
    let mut session = Session::connect();

    // -- next: an activate of the neighbouring key, which loads it --------
    let next = picker(&session.view).next.expect("a next entry");
    assert_eq!(activated(&next), 2);
    session.act(next);
    session.refresh_until("Aurora plays", |view| picker(view).active == Some(2));
    let view = session.view.clone();
    assert_eq!(
        root_panel(&view).groups[2].label,
        "Aurora",
        "this pattern's knobs follow the switch"
    );
    assert_eq!(activated(picker(&view).prev.as_ref().expect("prev")), 1);

    // -- on/off: switch Scanner off; the tour and next pass it by ---------
    let toggle = picker(&view).entries[2].toggle.clone().expect("a switch");
    let op = toggle
        .op_as::<PanelWriteOp>()
        .expect("a panel write")
        .clone();
    assert_eq!(op.channel, lpc_model::PLAYLIST_SKIP_CHANNEL);
    session.act(toggle);
    session.refresh_until("Scanner reads skipped", |view| {
        picker(view).entries[2].state == UiPatternEntryState::Skipped
    });
    let picker_now = picker(&session.view);
    assert!(!picker_now.entries[2].enabled);
    assert_eq!(
        activated(picker_now.next.as_ref().expect("next")),
        4,
        "next passes the skipped entry"
    );
    assert!(
        picker_now
            .skip_target
            .as_ref()
            .is_some_and(|target| target.engaged),
        "the panel holds the skip list"
    );
    let panel = root_panel(&session.view);
    let pattern = panel
        .groups
        .iter()
        .find(|group| group.label == "Pattern")
        .expect("the Pattern group");
    assert_eq!(
        pattern.controls[0].state,
        crate::UiPanelControlState::Engaged,
        "a held switch set holds the instrument, so the panel's reset sees it"
    );

    // -- tour: switch it on; the playlist reads it back -------------------
    let tour = picker_now.tour_toggle.clone().expect("a tour switch");
    session.act(tour);
    session.refresh_until("the tour runs", |view| picker(view).touring());
    let picker_now = picker(&session.view);
    assert_eq!(
        picker_now.tour,
        PlaylistTour::Cycle {
            step_seconds: 20.0,
            fade_seconds: 0.25,
        },
        "the default step, with the playlist's own default_fade"
    );
    assert!(picker_now.step_longer.is_some() && picker_now.step_shorter.is_some());

    // -- a tap on a dormant entry plays it --------------------------------
    let tap = picker_now.entries[0].play.clone().expect("tap to play");
    session.act(tap);
    session.refresh_until("Soft Noise plays again", |view| {
        picker(view).active == Some(1)
    });
}

/// P8, D11: activating a dormant entry — from the strip chip's action, the
/// same `PlaylistActivateOp` the Pattern picker's tap sends — loads AND
/// opens the entry's card. `aurora` starts dormant (Soft Noise is idle);
/// tapping it plays it, and once its subtree lands in a synced view, its
/// card is the focused one, with no second click.
#[test]
fn activating_a_dormant_entry_loads_it_and_opens_its_card() {
    let mut session = Session::connect();

    assert!(
        node_focused(&session.view, "Aurora").is_none(),
        "aurora starts dormant: absent from the tree entirely (AC1)"
    );

    let tap = picker(&session.view).entries[1]
        .play
        .clone()
        .expect("tap to play aurora");
    session.act(tap);
    session.refresh_until("aurora's card lands, focused", |view| {
        node_focused(view, "Aurora") == Some(true)
    });

    // The card is really open, not just marked focused in passing: its
    // own knob panel is the group the switch already proved follows.
    assert_eq!(root_panel(&session.view).groups[2].label, "Aurora");
}

/// Depth-first search of the project editor's card tree by label: `None`
/// when no node (top-level card or nested child) carries it, `Some(focused)`
/// when one does.
fn node_focused(view: &UiStudioView, label: &str) -> Option<bool> {
    fn walk_children(children: &[crate::UiNodeChild], label: &str) -> Option<bool> {
        for child in children {
            if child.label == label {
                return Some(child.focused);
            }
            if let Some(found) = walk_children(&child.children, label) {
                return Some(found);
            }
        }
        None
    }
    for node in &project_editor(view).nodes {
        if node.header.title == label {
            return Some(node.focused);
        }
        if let Some(found) = walk_children(&node.children, label) {
            return Some(found);
        }
    }
    None
}

#[test]
fn a_pattern_that_fails_to_compile_reads_failed() {
    let mut session = Session::connect();

    let tap = picker(&session.view).entries[3]
        .play
        .clone()
        .expect("tap to play Broken");
    session.act(tap);
    session.refresh_until("Broken reads failed", |view| {
        picker(view).entries[3].state == UiPatternEntryState::Failed
    });
    let picker_now = picker(&session.view);
    assert!(
        picker_now.entries[3].play.is_some(),
        "a failed entry can be tapped to try again"
    );
    assert_ne!(picker_now.active, Some(4), "and it is not what plays");
    let from = picker_now.active.expect("something plays");
    for step in [picker_now.next.as_ref(), picker_now.prev.as_ref()] {
        let target = activated(step.expect("a neighbour"));
        assert!(
            target != 4 && target != from,
            "next/prev pass the failed entry, got {target}"
        );
    }
}

/// One connected Studio over the fixture server, with the latest snapshot.
struct Session {
    actor: StudioActor<NoTimer>,
    tx: crate::app::studio::studio_view_channel::CommandSender,
    views: crate::app::studio::studio_view_channel::StudioViewReceiver,
    view: UiStudioView,
}

/// The actor's timer factory, nameable: these tests never wait on a timer.
type NoTimer = fn(core::time::Duration) -> core::future::Ready<()>;

fn no_timer(_: core::time::Duration) -> core::future::Ready<()> {
    core::future::ready(())
}

impl Session {
    fn connect() -> Self {
        let server = Rc::new(RefCell::new(pattern_set_e2e_server()));
        let io = InProcessServerIo {
            server,
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

    fn act(&mut self, action: UiAction) {
        self.tx.send(StudioCommand::Action(action));
        drive(self.actor.run_one_batch_for_test());
        if let Some(view) = self.views.try_recv() {
            self.view = view;
        }
    }

    /// Refresh (each read ticks the engine a frame) until `done` holds.
    fn refresh_until(&mut self, what: &str, done: impl Fn(&UiStudioView) -> bool) {
        for _ in 0..40 {
            if done(&self.view) {
                return;
            }
            self.tx.send(project_action(ProjectOp::RefreshProject));
            drive(self.actor.run_one_batch_for_test());
            if let Some(view) = self.views.try_recv() {
                self.view = view;
            }
        }
        panic!("{what}: never happened; picker = {:#?}", picker(&self.view));
    }
}

/// The root module card's panel.
fn root_panel(view: &UiStudioView) -> UiPanelGroup {
    let Some(UiNodeFace::Module(face)) = project_editor(view)
        .nodes
        .first()
        .expect("the root module card")
        .face
        .clone()
    else {
        panic!("the root card wears a module face");
    };
    face.panel
}

/// The Pattern group's instrument.
fn picker(view: &UiStudioView) -> UiPatternPicker {
    let panel = root_panel(view);
    let group = panel
        .groups
        .iter()
        .find(|group| group.label == "Pattern")
        .expect("a Pattern group");
    let [control] = group.controls.as_slice() else {
        panic!("the Pattern group is one instrument");
    };
    let UiPanelWidget::PatternPicker { picker } = &control.control.widget else {
        panic!("the instrument is the pattern picker");
    };
    picker.clone()
}

fn activated(action: &UiAction) -> u32 {
    action
        .op_as::<PlaylistActivateOp>()
        .expect("an activate")
        .entry
}

const PATTERN_SET_DIR: &str = "/projects/pattern-set-e2e";

/// A pattern module: a module around one shader that binds its own `tail`
/// knob in the module's scope, like `catalog/patterns/*/effect`.
const PATTERN_MODULE: &str = r#"{
  "kind": "Module",
  "nodes": { "shader": { "ref": "./shader.json" } }
}"#;
const PATTERN_SHADER: &str = r#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "bindings": {
    "output": { "target": "bus:visual.out" },
    "tail": { "source": "bus:tail" }
  },
  "consumed": {
    "tail": {
      "kind": "value",
      "value": "f32",
      "default": 0.4,
      "min": 0.05,
      "max": 1,
      "label": "Tail"
    }
  }
}"#;
const PATTERN_GLSL: &str = "layout(binding = 0) uniform float tail;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(pos.x * tail, pos.y, 0.5, 1.0);\n}\n";
const BROKEN_GLSL: &str = "layout(binding = 0) uniform float tail;\n\nvec4 render_2d(vec2 pos) {\n    return not_a_function(pos);\n}\n";

/// Four pattern entries; the fourth does not compile. Idle is the first.
fn pattern_set_e2e_server() -> LpServer {
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

    let module_json = r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "playlist": { "ref": "./playlist.json" },
    "pixels": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#;
    let clock_json = r#"{
  "kind": "Clock",
  "transport": { "play_state": "playing", "rate": 1.0 }
}"#;
    let playlist_json = r#"{
  "kind": "Playlist",
  "bindings": {
    "time": { "source": "bus:time" }
  },
  "idle_entry": 1,
  "default_fade": 0.25,
  "entries": {
    "1": { "name": "soft_noise", "node": { "ref": "./modules/noise/module.json" } },
    "2": { "name": "aurora", "node": { "ref": "./modules/aurora/module.json" } },
    "3": { "name": "scanner", "node": { "ref": "./modules/scanner/module.json" } },
    "4": { "name": "broken", "node": { "ref": "./modules/broken/module.json" } }
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
    let output_json = r#"{
  "kind": "Output",
  "ports": {
    "0": { "endpoint": "ws281x:local:D10" }
  },
  "bindings": {
    "input": { "source": "bus:control.out" }
  }
}"#;
    let mut files: Vec<(String, &str)> = vec![
        ("project.json".into(), "{\n  \"format\": 11\n}\n"),
        ("module.json".into(), module_json),
        ("clock.json".into(), clock_json),
        ("playlist.json".into(), playlist_json),
        ("fixture.json".into(), fixture_json),
        ("output.json".into(), output_json),
    ];
    for (folder, glsl) in [
        ("noise", PATTERN_GLSL),
        ("aurora", PATTERN_GLSL),
        ("scanner", PATTERN_GLSL),
        ("broken", BROKEN_GLSL),
    ] {
        files.push((format!("modules/{folder}/module.json"), PATTERN_MODULE));
        files.push((format!("modules/{folder}/shader.json"), PATTERN_SHADER));
        files.push((format!("modules/{folder}/shader.glsl"), glsl));
    }
    for (name, body) in &files {
        server
            .base_fs_mut()
            .write_file(
                format!("{PATTERN_SET_DIR}/{name}").as_path(),
                body.as_bytes(),
            )
            .expect("write project file");
    }
    server
        .load_project(PATTERN_SET_DIR.as_path())
        .expect("load pattern-set-e2e project");
    server.advance_frame(16).expect("tick");
    server
}
