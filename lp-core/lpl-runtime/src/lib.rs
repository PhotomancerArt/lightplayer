//! lpl-runtime: legacy node runtimes implementing lpc_engine::NodeRuntime

#![no_std]

extern crate alloc;

mod legacy_hooks;
pub mod nodes;
pub mod output;
pub mod project_hooks;

pub use nodes::{FixtureRuntime, OutputRuntime, ShaderRuntime, TextureRuntime};
pub use output::{MemoryOutputProvider, OutputChannelHandle, OutputFormat, OutputProvider};
pub use project_hooks::install;

pub use lpc_engine::LegacyNodeRuntime;
pub use lpl_model::NodeConfig;
