//! LightPlayer application configuration.
//!
//! Stored in `lightplayer.json` at the filesystem root. May be extended or split later.

use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Application configuration (lightplayer.json)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LightplayerConfig {
    /// Project to load at startup; if None, use lexical-first available project
    #[serde(default)]
    pub startup_project: Option<String>,
}
