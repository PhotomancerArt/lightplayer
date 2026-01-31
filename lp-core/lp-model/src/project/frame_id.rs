/// Frame identifier - increments each render frame
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct FrameId(pub i64);

impl FrameId {
    pub fn new(id: i64) -> Self {
        Self(id)
    }

    pub fn as_i64(self) -> i64 {
        self.0
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Default for FrameId {
    fn default() -> Self {
        Self(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_id_creation() {
        let id = FrameId::new(42);
        assert_eq!(id.as_i64(), 42);
    }

    #[test]
    fn test_frame_id_next() {
        let id = FrameId::new(10);
        let next = id.next();
        assert_eq!(next.as_i64(), 11);
    }

    #[test]
    fn test_frame_id_default() {
        let id = FrameId::default();
        assert_eq!(id.as_i64(), 0);
    }
}
