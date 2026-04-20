//! Loaded effect: manifest plus raw GLSL (not compiled in M0).

use alloc::string::String;

use crate::error::FxError;
use crate::manifest::FxManifest;
use crate::parse::parse_manifest;

/// Parsed manifest and GLSL source for one `.fx` module.
#[derive(Debug, Clone, PartialEq)]
pub struct FxModule {
    pub manifest: FxManifest,
    pub glsl_source: String,
}

impl FxModule {
    /// Parse `fx.toml` content and retain `main.glsl` source. Caller reads files.
    pub fn from_sources(toml_src: &str, glsl_src: &str) -> Result<Self, FxError> {
        let manifest = parse_manifest(toml_src)?;
        Ok(Self {
            manifest,
            glsl_source: String::from(glsl_src),
        })
    }
}
