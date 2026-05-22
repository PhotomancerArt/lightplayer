//! Incoming change batches for [`super::NodeDefRegistry::sync`].

use lpfs::FsChange;

/// Incoming filesystem notification for [`super::NodeDefRegistry::sync`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryChange {
    Fs(FsChange),
}
