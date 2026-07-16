//! LightPlayer application configuration.
//!
//! Stored in `lightplayer.json` at the filesystem root. May be extended or split later.

use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Application/server configuration (lightplayer.json)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Project to load at startup; if None, use lexical-first available project
    #[serde(default)]
    pub startup_project: Option<String>,
}

impl ServerConfig {
    /// Config file location at the filesystem root — shared by device boot
    /// (read) and the server's load handler (write).
    pub const PATH: &'static str = "/lightplayer.json";
}
