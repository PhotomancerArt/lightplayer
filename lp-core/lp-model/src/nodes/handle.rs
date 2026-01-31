/// Runtime node identifier - sequential generation
///
/// Handles can change on reload (not stable). Paths are for loading/resolving;
/// handles are for runtime references.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct NodeHandle(pub i32);

impl NodeHandle {
    pub fn new(id: i32) -> Self {
        Self(id)
    }

    pub fn as_i32(self) -> i32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_handle_creation() {
        let handle = NodeHandle::new(42);
        assert_eq!(handle.as_i32(), 42);
    }
}
