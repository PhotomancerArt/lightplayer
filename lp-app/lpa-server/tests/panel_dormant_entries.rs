//! Knob values of a dormant playlist entry survive a reboot (multi-pattern
//! vision D12, plan AC6).
//!
//! A playlist keeps only its playing entry loaded, so at boot every other
//! entry is absent from the tree. Until 2026-09-25 `panel_state::restore`
//! dropped every persisted writer whose scope no live node inhabited — a
//! reboot forgot every dormant pattern's knobs
//! (`docs/defects/2026-09-25-panel-restore-drops-dormant-entry-knobs.md`).
//!
//! These tests also prove the server's tick runs the pre-tick residency
//! step: every switch here is applied by `LpServer::advance_frame` →
//! `Project::tick`, the one path every edge ticks through.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;
use core::cell::RefCell;
use std::collections::VecDeque;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::panel_state::{
    self, PANEL_STATE_WRITE_INTERVAL_MS, PanelStateEntry, PanelStateFile,
};
use lpa_server::{LpGraphics, LpServer, Project};
use lpc_engine::Engine;
use lpc_engine::node::{
    DestroyCtx, MemPressureCtx, NodeEntryState, NodeError, NodeRuntime, PressureLevel,
    ResidencyRequest, ScopeRef,
};
use lpc_model::SlotPath;
use lpc_model::{AsLpPath, ChannelName, LpPath, LpPathBuf, LpValue, NodeId, NodeUseLocation};
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::WireProjectHandle;
use lpfs::{LpFs, LpFsMemory, LpFsView};

const IDLE: u32 = 1;
const PULSE: u32 = 2;

#[test]
fn a_dormant_entrys_knobs_survive_a_reboot_and_come_back_with_the_entry() {
    let mut harness = Harness::new("panel-dormant-entry");
    harness.load();
    let queue = harness.install_owner();

    // Play the pattern and turn both of its knobs: the one in its own
    // module scope, and the one in the entry's sink scope.
    queue
        .borrow_mut()
        .push_back(ResidencyRequest::switch(IDLE, PULSE));
    harness.advance(16);
    let playlist = harness.playlist();
    let pulse = harness
        .entry_child(PULSE)
        .expect("the server tick loaded the pattern");
    let module_scope = ScopeRef::Module { owner: pulse };
    let sink_scope = ScopeRef::Sink {
        owner: playlist,
        entry: PULSE,
    };
    let speed = ChannelName(String::from("speed"));
    let engine = harness.project().engine_mut();
    engine.panel_write(module_scope, speed.clone(), LpValue::F32(0.25), None);
    engine.panel_write(sink_scope, speed.clone(), LpValue::F32(0.75), None);

    // Back to idle: the pattern is dormant, and the file is written.
    queue
        .borrow_mut()
        .push_back(ResidencyRequest::switch(PULSE, IDLE));
    harness.advance(PANEL_STATE_WRITE_INTERVAL_MS);
    assert!(
        harness.entry_child(PULSE).is_none(),
        "the pattern is dormant"
    );
    let saved = harness.state_file().expect("panel state written");
    let mut scopes: Vec<(String, LpValue)> = saved
        .entries
        .iter()
        .map(|entry| (entry.scope.clone(), entry.value.clone()))
        .collect();
    scopes.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        scopes,
        vec![
            (
                String::from("/panel_dormant_entry.show/list.playlist/entries[2]"),
                LpValue::F32(0.75)
            ),
            (
                String::from("/panel_dormant_entry.show/list.playlist/pulse.module"),
                LpValue::F32(0.25)
            ),
        ],
        "a dormant entry's writers are written, keyed by persist path"
    );

    // Reboot. The pattern is dormant at boot, and its knobs are still held.
    let mut rebooted = harness.reboot();
    let playlist = rebooted.playlist();
    let sink_scope = ScopeRef::Sink {
        owner: playlist,
        entry: PULSE,
    };
    let engine = rebooted.project().engine();
    assert_eq!(
        engine
            .panel_writers()
            .get(sink_scope, &speed)
            .map(|w| &w.value),
        Some(&LpValue::F32(0.75)),
        "the sink writer of a dormant entry is restored"
    );
    assert_eq!(
        engine.panel_writers().parked().count(),
        1,
        "the module writer waits, parked, for its entry"
    );
    let snapshot = panel_state::snapshot(engine, true);
    assert_eq!(
        snapshot.entries.len(),
        2,
        "a snapshot taken while dormant keeps both: {snapshot:?}"
    );

    // Play the pattern again: its module knob engages on the new module.
    let queue = rebooted.install_owner();
    queue
        .borrow_mut()
        .push_back(ResidencyRequest::switch(IDLE, PULSE));
    rebooted.advance(16);
    let pulse = rebooted.entry_child(PULSE).expect("the pattern loaded");
    let engine = rebooted.project().engine();
    assert_eq!(
        engine
            .panel_writers()
            .get(ScopeRef::Module { owner: pulse }, &speed)
            .map(|w| &w.value),
        Some(&LpValue::F32(0.25)),
        "the module knob is back on the reloaded pattern"
    );
}

#[test]
fn a_writer_for_an_entry_the_playlist_no_longer_authors_is_dropped() {
    let mut harness = Harness::new("panel-dormant-unknown");
    let entry = |scope: &str| PanelStateEntry {
        scope: format!("/panel_dormant_unknown.show/list.playlist/{scope}"),
        channel: String::from("speed"),
        value: LpValue::F32(0.5),
    };
    let file = PanelStateFile {
        version: panel_state::PANEL_STATE_VERSION,
        auto_save: true,
        entries: vec![
            entry("entries[9]"),
            entry("gone.module"),
            entry("entries[2]"),
        ],
    };
    harness.write_state_file(lpc_wire::json::to_string(&file).expect("encode").as_bytes());
    harness.load();
    let engine = harness.project().engine();
    assert_eq!(
        engine.panel_writers().len(),
        1,
        "only the authored (dormant) entry 2's sink writer is kept"
    );
    assert_eq!(engine.panel_writers().parked().count(), 0);
}

// --- harness ---

struct Harness {
    server: LpServer,
    project_path: LpPathBuf,
    base_fs: Rc<RefCell<dyn LpFs>>,
    handle: Option<WireProjectHandle>,
}

impl Harness {
    fn new(name: &str) -> Self {
        let base_fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let harness = Self {
            server: build_server(base_fs.clone()),
            project_path: LpPathBuf::from("/projects").join(name),
            base_fs,
            handle: None,
        };
        harness.write_project_files();
        harness
    }

    fn reboot(&self) -> Harness {
        let mut rebooted = Harness {
            server: build_server(self.base_fs.clone()),
            project_path: self.project_path.clone(),
            base_fs: self.base_fs.clone(),
            handle: None,
        };
        rebooted.load();
        rebooted
    }

    fn load(&mut self) {
        self.handle = Some(
            self.server
                .load_project(self.project_path.as_path())
                .expect("load"),
        );
    }

    fn advance(&mut self, delta_ms: u32) {
        self.server.advance_frame(delta_ms).expect("tick");
    }

    fn project(&mut self) -> &mut Project {
        let handle = self.handle.expect("loaded");
        self.server
            .project_manager_mut()
            .get_project_mut(handle)
            .expect("loaded project")
    }

    fn playlist(&mut self) -> NodeId {
        let location =
            NodeUseLocation::root().child(SlotPath::parse("nodes[list]").expect("slot path"));
        self.project()
            .engine()
            .project_runtime_index()
            .node_id(&location)
            .expect("playlist projected")
    }

    fn entry_child(&mut self, entry: u32) -> Option<NodeId> {
        let owner = self.playlist();
        let scope = ScopeRef::Sink { owner, entry };
        self.project()
            .engine()
            .tree()
            .entries()
            .find(|node| node.parent == Some(owner) && node.scope == Some(scope))
            .map(|node| node.id)
    }

    /// Replace the playlist's runtime with a stand-in that asks for what
    /// the test queues (the playlist produces no requests until plan P4).
    fn install_owner(&mut self) -> Rc<RefCell<VecDeque<ResidencyRequest>>> {
        let queue = Rc::new(RefCell::new(VecDeque::new()));
        let playlist = self.playlist();
        install(self.project().engine_mut(), playlist, queue.clone());
        queue
    }

    fn state_path(&self) -> LpPathBuf {
        self.project_path.join(".lp").join("panel.json")
    }

    fn state_file(&self) -> Option<PanelStateFile> {
        let bytes = self
            .base_fs
            .borrow()
            .read_file(self.state_path().as_path())
            .ok()?;
        Some(lpc_wire::json::from_slice::<PanelStateFile>(&bytes).expect("state file parses"))
    }

    fn write_state_file(&self, body: &[u8]) {
        self.base_fs
            .borrow()
            .write_file(self.state_path().as_path(), body)
            .expect("write state file");
    }

    fn write_project_files(&self) {
        let files: [(&str, &[u8]); 8] = [
            ("project.json", b"{\n  \"format\": 11\n}\n"),
            (
                "module.json",
                br#"{"kind":"Module","nodes":{"clock":{"ref":"./clock.json"},"list":{"ref":"./playlist.json"}}}"#,
            ),
            ("clock.json", br#"{"kind":"Clock"}"#),
            (
                "playlist.json",
                br#"{"kind":"Playlist","idle_entry":1,"entries":{
                    "1":{"name":"idle","node":{"ref":"./idle.json"}},
                    "2":{"name":"pulse","node":{"ref":"./pulse/module.json"}}}}"#,
            ),
            (
                "idle.json",
                br#"{"kind":"Shader","source":{"path":"idle.glsl"}}"#,
            ),
            (
                "pulse/module.json",
                include_bytes!("../../../catalog/patterns/pulse/effect/module.json"),
            ),
            (
                "pulse/shader.json",
                include_bytes!("../../../catalog/patterns/pulse/effect/shader.json"),
            ),
            (
                "pulse/shader.glsl",
                include_bytes!("../../../catalog/patterns/pulse/effect/shader.glsl"),
            ),
        ];
        let fs = self.base_fs.borrow();
        for (path, bytes) in files {
            fs.write_file(self.project_path.join(path).as_path(), bytes)
                .expect("write project file");
        }
        fs.write_file(
            self.project_path.join("idle.glsl").as_path(),
            b"vec4 render_2d(vec2 p) { return vec4(1.0); }",
        )
        .expect("write idle glsl");
    }
}

fn install(engine: &mut Engine, playlist: NodeId, queue: Rc<RefCell<VecDeque<ResidencyRequest>>>) {
    let entry = engine.tree_mut().get_mut(playlist).expect("playlist entry");
    let NodeEntryState::Alive(runtime) = entry.state.get_mut() else {
        panic!("playlist is alive");
    };
    *runtime = Box::new(StandInOwner { queue });
}

struct StandInOwner {
    queue: Rc<RefCell<VecDeque<ResidencyRequest>>>,
}

impl NodeRuntime for StandInOwner {
    fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError> {
        Ok(())
    }

    fn residency_request(&mut self) -> Option<ResidencyRequest> {
        self.queue.borrow_mut().pop_front()
    }
}

fn build_server(base_fs: Rc<RefCell<dyn LpFs>>) -> LpServer {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    LpServer::new(
        output_provider,
        Box::new(LpFsView::new(base_fs, LpPath::new("/"))),
        "projects".as_path(),
        None,
        None,
        graphics,
    )
}
