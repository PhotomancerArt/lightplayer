//! Common engine resource vocabulary.
//!
//! Resources are registry-owned runtime payloads addressed by lightweight ids.
//! Concrete resource kinds live under [`crate::resources`].

pub use crate::resources::buffer::{
    RuntimeBuffer, RuntimeBufferError, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
    RuntimeBufferStore, RuntimeChannelSampleFormat, RuntimeColorLayout, RuntimeTextureFormat,
};
