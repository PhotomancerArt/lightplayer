//! Shared fixture for the entry-residency tests: a three-entry playlist and
//! a stand-in owner that asks for loads and unloads and records every
//! answer.
//!
//! The stand-in replaces the playlist's own runtime, because the playlist
//! does not produce requests until the switch sequence lands (plan P4). It
//! demands nothing, so a tick never reaches an unloaded child through it.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::node::{
    DestroyCtx, MemPressureCtx, NodeEntryState, NodeError, NodeRuntime, PressureLevel,
    ResidencyRequest, ScopeRef,
};
use lpc_engine::{Engine, EngineServices, ProjectLoader};
use lpc_model::{NodeId, NodeUseLocation, SlotPath, TreePath};
use lpfs::{AsLpPath, LpFs, LpFsMemory};

/// Entry keys of [`project_fs`].
pub const IDLE: u32 = 1;
pub const PATTERN: u32 = 2;
pub const BROKEN: u32 = 3;

/// A module with a clock and a playlist (`list`, idle entry 1):
///
/// - entry 1 `idle`: a shader;
/// - entry 2 `pulse`: the catalog's real `pulse` pattern module
///   (`catalog/patterns/pulse/effect`), whose shader consumes `bus:speed`
///   inside the module's own scope;
/// - entry 3 `broken`: a ref to a file that does not exist.
pub fn project_fs() -> LpFsMemory {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", b"{\n  \"format\": 11\n}\n");
    write(
        &fs,
        "/module.json",
        br#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "list": { "ref": "./playlist.json" }
  }
}"#,
    );
    write(&fs, "/clock.json", br#"{ "kind": "Clock" }"#);
    write(
        &fs,
        "/playlist.json",
        br#"{
  "kind": "Playlist",
  "idle_entry": 1,
  "entries": {
    "1": { "name": "idle", "node": { "ref": "./idle.json" } },
    "2": { "name": "pulse", "node": { "ref": "./pulse/module.json" } },
    "3": { "name": "broken", "node": { "ref": "./missing.json" } }
  }
}"#,
    );
    write(
        &fs,
        "/idle.json",
        br#"{ "kind": "Shader", "source": { "path": "idle.glsl" } }"#,
    );
    write(
        &fs,
        "/idle.glsl",
        b"vec4 render_2d(vec2 p) { return vec4(1.0); }",
    );
    write(
        &fs,
        "/pulse/module.json",
        include_bytes!("../../../../catalog/patterns/pulse/effect/module.json"),
    );
    write(
        &fs,
        "/pulse/shader.json",
        include_bytes!("../../../../catalog/patterns/pulse/effect/shader.json"),
    );
    write(
        &fs,
        "/pulse/shader.glsl",
        include_bytes!("../../../../catalog/patterns/pulse/effect/shader.glsl"),
    );
    fs
}

pub fn load(fs: &LpFsMemory) -> LoadedProjectRuntime {
    let services = EngineServices::new(TreePath::parse("/residency.show").expect("path"));
    ProjectLoader::load_from_root(fs, services).expect("load residency project")
}

pub fn playlist_use() -> NodeUseLocation {
    NodeUseLocation::root().child(SlotPath::parse("nodes[list]").expect("slot path"))
}

pub fn playlist_id(engine: &Engine) -> NodeId {
    engine
        .project_runtime_index()
        .node_id(&playlist_use())
        .expect("playlist projected")
}

/// The node entry `entry` plays, if it is loaded.
pub fn entry_child(engine: &Engine, entry: u32) -> Option<NodeId> {
    let owner = playlist_id(engine);
    let scope = ScopeRef::Sink { owner, entry };
    engine
        .tree()
        .entries()
        .find(|node| node.parent == Some(owner) && node.scope == Some(scope))
        .map(|node| node.id)
}

/// Entry keys whose child is in the tree, ascending.
pub fn loaded_entries(engine: &Engine) -> Vec<u32> {
    [IDLE, PATTERN, BROKEN]
        .into_iter()
        .filter(|entry| entry_child(engine, *entry).is_some())
        .collect()
}

/// An answer the engine gave the owner, in the order it gave them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerEvent {
    Loaded(u32, NodeId),
    Unloaded(u32),
    LoadFailed(u32, String),
    Refused(ResidencyRequest, String),
}

/// The test's handle on the stand-in owner.
#[derive(Clone, Default)]
pub struct Owner {
    queue: Rc<RefCell<VecDeque<ResidencyRequest>>>,
    log: Rc<RefCell<Vec<OwnerEvent>>>,
}

impl Owner {
    /// Replace the playlist's runtime with the stand-in.
    pub fn install(engine: &mut Engine) -> Self {
        let owner = Self::default();
        let id = playlist_id(engine);
        let entry = engine.tree_mut().get_mut(id).expect("playlist entry");
        let NodeEntryState::Alive(runtime) = entry.state.get_mut() else {
            panic!("playlist is alive");
        };
        *runtime = Box::new(StandInOwner {
            queue: owner.queue.clone(),
            log: owner.log.clone(),
        });
        owner
    }

    pub fn request(&self, request: ResidencyRequest) {
        self.queue.borrow_mut().push_back(request);
    }

    pub fn take_log(&self) -> Vec<OwnerEvent> {
        core::mem::take(&mut *self.log.borrow_mut())
    }
}

struct StandInOwner {
    queue: Rc<RefCell<VecDeque<ResidencyRequest>>>,
    log: Rc<RefCell<Vec<OwnerEvent>>>,
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

    fn entry_loaded(&mut self, entry: u32, child: NodeId, output_slot: &SlotPath) {
        assert_eq!(output_slot.to_string(), "output");
        self.log.borrow_mut().push(OwnerEvent::Loaded(entry, child));
    }

    fn entry_unloaded(&mut self, entry: u32) {
        self.log.borrow_mut().push(OwnerEvent::Unloaded(entry));
    }

    fn entry_load_failed(&mut self, entry: u32, reason: &str) {
        self.log
            .borrow_mut()
            .push(OwnerEvent::LoadFailed(entry, reason.to_string()));
    }

    fn residency_refused(&mut self, request: ResidencyRequest, reason: &str) {
        self.log
            .borrow_mut()
            .push(OwnerEvent::Refused(request, reason.to_string()));
    }
}

fn write(fs: &LpFsMemory, path: &str, bytes: &[u8]) {
    fs.write_file(path.as_path(), bytes).expect("write fixture");
}
