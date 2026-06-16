use alloc::vec::Vec;

/// Added, changed, and removed identities for a comparable project collection.
///
/// `Id` is the stable identity used for additions and removals. `Changed` is
/// the payload used for changed entries; it defaults to `Id` for collections
/// that only need to report which identities changed.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChangeSummary<Id, Changed = Id> {
    /// Newly present identities.
    pub added: Vec<Id>,
    /// Previously present identities whose effective contents or state changed.
    pub changed: Vec<Changed>,
    /// Identities that are no longer present.
    pub removed: Vec<Id>,
}

impl<Id, Changed> Default for ChangeSummary<Id, Changed> {
    fn default() -> Self {
        Self {
            added: Vec::new(),
            changed: Vec::new(),
            removed: Vec::new(),
        }
    }
}

impl<Id, Changed> ChangeSummary<Id, Changed> {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}
