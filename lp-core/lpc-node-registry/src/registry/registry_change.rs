//! Incoming change batches for [`super::NodeDefRegistry::sync`].

use lpfs::FsChange;

/// Registry change op. M4: filesystem only. M5: ChangeSet variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryChange {
    Fs(FsChange),
}
