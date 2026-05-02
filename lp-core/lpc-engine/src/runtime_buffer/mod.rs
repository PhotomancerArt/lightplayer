//! Runtime buffer storage: opaque IDs, typed payloads, and a versioned store.

mod runtime_buffer;
mod runtime_buffer_id;
mod runtime_buffer_store;

pub use runtime_buffer::{
    RuntimeBuffer, RuntimeBufferKind, RuntimeBufferMetadata, RuntimeChannelSampleFormat,
    RuntimeColorLayout, RuntimeTextureFormat,
};
pub use runtime_buffer_id::RuntimeBufferId;
pub use runtime_buffer_store::{RuntimeBufferError, RuntimeBufferStore};
