//! Browser runtime: `WebAssembly` JS API + host builtin exports (`init_host_exports`).

mod engine;
mod instance;
mod link;
mod marshal;

pub use engine::{BrowserLpvmEngine, BrowserLpvmModule, init_host_exports};
pub use instance::BrowserLpvmInstance;
