//! Core runtime owner: [`Engine`] drives frame state, tree, bindings, resolver, and artifacts.

mod engine;
mod engine_error;
#[cfg(test)]
pub(crate) mod test_support;

pub use engine::Engine;
#[cfg(test)]
pub(crate) use engine::default_demand_input_path;
pub use engine_error::EngineError;

#[cfg(test)]
pub(crate) use engine::resolve_with_engine_host;
