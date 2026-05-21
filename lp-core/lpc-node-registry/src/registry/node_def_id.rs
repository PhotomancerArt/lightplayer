//! Opaque handle to a node definition entry inside [`super::NodeDefRegistry`].

/// Runtime handle returned by [`super::NodeDefRegistry::load_root`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct NodeDefId(u32);

impl NodeDefId {
    pub(crate) const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub fn raw(self) -> u32 {
        self.0
    }
}
