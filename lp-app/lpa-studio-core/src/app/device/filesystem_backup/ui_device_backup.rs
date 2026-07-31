//! The finished backup, as the view carries it to the shell.

use std::rc::Rc;

/// One completed filesystem backup, waiting to be saved.
///
/// `seq` is session-monotonic and the shell downloads exactly when it
/// observes it advance — the same paint-key discipline the agent's debug
/// dump uses. Without it, every re-render of a stale DTO would re-download a
/// megabyte, which on a slow board is the difference between one file in the
/// user's Downloads folder and twenty.
///
/// The bytes ride an `Rc<[u8]>` because the view is cloned on every render
/// and this is the largest thing in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiDeviceBackup {
    pub seq: u64,
    pub file_name: String,
    pub bytes: Rc<[u8]>,
    /// Files captured, for the notice that announces the download.
    pub file_count: u32,
}
