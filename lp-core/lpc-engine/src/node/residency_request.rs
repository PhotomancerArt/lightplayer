//! A parent node's request to load or unload one of its own children.
//!
//! Loading children is the parent's job (multi-pattern vision D2): a node
//! that owns entry-keyed children (today only the playlist, D3) asks through
//! [`crate::node::NodeRuntime::residency_request`], and the engine applies
//! the request at the pre-tick step ([`crate::Engine::apply_residency`]),
//! where no render borrow is live and no node is `Executing`.

/// Load and/or unload of the requesting node's own entry-keyed children.
///
/// Entry keys are the authored ones (`entries[k]`), never runtime ids. The
/// engine applies `unload` before `load`, so the two entries are never held
/// at once (abort-tier firmware: a switch must free before it allocates).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResidencyRequest {
    /// Entry to load, if any.
    pub load: Option<u32>,
    /// Entry to unload, if any. Applied first.
    pub unload: Option<u32>,
}

impl ResidencyRequest {
    /// Load `entry`, nothing else.
    pub const fn load(entry: u32) -> Self {
        Self {
            load: Some(entry),
            unload: None,
        }
    }

    /// Unload `entry`, nothing else.
    pub const fn unload(entry: u32) -> Self {
        Self {
            load: None,
            unload: Some(entry),
        }
    }

    /// Unload `from`, then load `to`: one switch.
    pub const fn switch(from: u32, to: u32) -> Self {
        Self {
            load: Some(to),
            unload: Some(from),
        }
    }

    /// Whether the request asks for nothing.
    pub const fn is_empty(&self) -> bool {
        self.load.is_none() && self.unload.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_carries_both_halves() {
        let request = ResidencyRequest::switch(1, 2);
        assert_eq!(request.unload, Some(1));
        assert_eq!(request.load, Some(2));
        assert!(!request.is_empty());
        assert!(ResidencyRequest::default().is_empty());
    }
}
