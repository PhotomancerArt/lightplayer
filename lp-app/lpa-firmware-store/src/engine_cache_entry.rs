//! One cached engine: its key, what it is, where it came from, and its use.

use serde::{Deserialize, Serialize};

/// How an engine reached the cache.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineSource {
    /// Studio installed it over USB (sliced from the package it flashed).
    Installed,
    /// Fetched from the firmware store.
    Fetched,
    /// Read back from a board.
    ReadBack,
}

/// One engine in the cache.
///
/// Keyed by `sha256`: SHA-256 of `engine.bin` exactly as flashed (header
/// committed) — the same value as the core's digest slot, the board's
/// `engineSha256` and `ota-manifest.json`'s `engine.sha256`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCacheEntry {
    /// 64 lowercase hex; the key.
    pub sha256: String,
    /// The engine's length in bytes.
    pub length: u64,
    /// The target (a line of builds, e.g. `esp32c6-4mb`), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// `<version>+<commit[..12]>`, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    /// The app version (never the index's shape version), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// How it got here.
    pub source: EngineSource,
    /// When it was first put, f64 epoch seconds (caller-supplied).
    pub added_at_epoch_seconds: f64,
    /// When it was last put or read, f64 epoch seconds (caller-supplied).
    pub last_used_at_epoch_seconds: f64,
    /// Never evicted while set (M7 pins the backup of an in-flight update).
    #[serde(default)]
    pub held: bool,
}

impl EngineCacheEntry {
    /// A new entry for an engine put at `now`, not held.
    pub fn new(sha256: impl Into<String>, length: u64, source: EngineSource, now: f64) -> Self {
        Self {
            sha256: sha256.into(),
            length,
            target: None,
            build_id: None,
            version: None,
            source,
            added_at_epoch_seconds: now,
            last_used_at_epoch_seconds: now,
            held: false,
        }
    }
}
