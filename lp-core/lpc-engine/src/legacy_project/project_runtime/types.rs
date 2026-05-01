use crate::gfx::LpGraphics;
use crate::nodes::LegacyNodeRuntime;
use crate::output::OutputProvider;
use crate::runtime::frame_time::FrameTime;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;
use core::cell::RefCell;
use lpc_model::{FrameId, LpPathBuf, NodeId};
use lpc_shared::time::TimeProvider;
use lpfs::LpFs;
use lpl_model::{NodeConfig, NodeKind};

/// Optional callback for memory stats (free_bytes, used_bytes). Used for shed logging on ESP32.
pub type MemoryStatsFn = fn() -> Option<(u32, u32)>;

/// Project runtime - manages nodes and rendering
pub struct LegacyProjectRuntime {
    /// Current frame ID
    pub frame_id: FrameId,
    /// Frame timing information
    pub frame_time: FrameTime,
    /// Filesystem (shared via Rc<RefCell<>> to allow external modifications in tests)
    pub fs: Rc<RefCell<dyn LpFs>>,
    /// Output provider (shared across nodes)
    pub output_provider: Rc<RefCell<dyn OutputProvider>>,
    /// Node entries
    pub nodes: BTreeMap<NodeId, NodeEntry>,
    /// Next handle to assign
    pub next_handle: u32,
    /// Optional memory stats for shed logging (ESP32 passes, others None)
    pub memory_stats: Option<MemoryStatsFn>,
    /// Optional time provider for perf timing (e.g. shader comp duration). ESP32/emu pass, others None.
    pub time_provider: Option<Rc<dyn TimeProvider>>,
    /// Shader / graphics backend (Cranelift, WASM, …).
    pub graphics: Arc<dyn LpGraphics>,
}

/// Node entry in runtime
pub struct NodeEntry {
    /// Node path
    pub path: LpPathBuf,
    /// Node kind
    pub kind: NodeKind,
    /// Node config
    pub config: Box<dyn NodeConfig>,
    /// Frame when config was last updated
    pub config_ver: FrameId,
    /// Node status
    pub status: NodeStatus,
    /// Frame when status was last changed
    pub status_ver: FrameId,
    /// Node runtime (None until initialized)
    pub runtime: Option<Box<dyn LegacyNodeRuntime>>,
    /// Last frame state updates occurred
    pub state_ver: FrameId,
}

/// Node status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeStatus {
    /// Created but not yet initialized
    Created,
    /// Error initializing the node
    InitError(String),
    /// Node is running normally
    Ok,
    /// Node is running, but something is wrong
    Warn(String),
    /// Node cannot run
    Error(String),
}
