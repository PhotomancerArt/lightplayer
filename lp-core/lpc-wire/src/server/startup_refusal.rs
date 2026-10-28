//! The board's word that its engine refused its startup project for the
//! project's format — a hello fact (wire 46).
//!
//! A host finishing a board's projects after an update (`lpa-update`'s
//! project stage) needs it for one corner (the mapping rebuild's DD27/DD30):
//! a link that drops after the startup project was rewritten to the engine's
//! format and before it was loaded leaves a board whose startup project is
//! current and not running. A stage run again finds nothing to rewrite;
//! this report is what tells it to load that project, and its absence is
//! what keeps a board someone stopped on purpose stopped.
//!
//! The server sets it when the boot's startup load fails and the project's
//! `project.json` says another format than the engine reads, and clears it
//! the moment anything else is asked of the engine's projects (a load, an
//! unload, a stop). Hosts read it as a fact, never from log words.

use alloc::string::String;
use serde::{Deserialize, Serialize};

/// The startup project this engine refused at boot for its project format,
/// while nothing has been loaded, unloaded or stopped since.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupRefusal {
    /// The project's path as the boot loaded it (`/projects/eaves`).
    pub project: String,
    /// The format its `project.json` says; `None` when it says none.
    pub found: Option<u32>,
}
