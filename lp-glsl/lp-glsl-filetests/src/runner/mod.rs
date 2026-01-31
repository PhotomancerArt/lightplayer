//! Main test runner orchestration.

pub mod concurrent;

use crate::output_mode::OutputMode;
use anyhow::Result;
use std::path::Path;

/// Run tests - implementation will be added in later phases.
pub fn run_tests(_filetests_dir: &Path, _output_mode: OutputMode) -> Result<()> {
    // TODO: Implement using new structure
    todo!("Phase 1: Stub implementation")
}
