//! Core runtime owner: [`Engine`] drives frame state, tree, bindings, resolver, and artifacts.

mod engine;
mod engine_error;
#[cfg(test)]
pub(crate) mod test_support;

pub use engine::Engine;
pub use engine_error::EngineError;
