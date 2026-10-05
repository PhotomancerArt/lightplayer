//! What a host does with a board: the decision, and where it gets an engine.

pub mod decision;
pub mod engine_source;

pub use decision::{Decision, HostFacts, NeedsUsbWhy, decide};
pub use engine_source::{EngineSource, SourceEffect, SourceResult, SourceStep, StoreAnswer};
