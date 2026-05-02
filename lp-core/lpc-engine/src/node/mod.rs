//! New runtime spine contracts (tick/destroy/memory pressure, narrow contexts).
//! Legacy runtimes live in [`crate::nodes`].

mod contexts;
mod node;
mod node_error;
mod pressure_level;

pub use contexts::{DestroyCtx, MemPressureCtx, NodeResourceInitContext, TickContext};
pub use node::{FixtureProjectionInfo, Node, ShaderProjectionWire};
pub use node_error::NodeError;
pub use pressure_level::PressureLevel;
