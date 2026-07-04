//! Kinds of recovery frames — the coarse "what was the system doing" axis.

/// What kind of work a recovery frame covers.
///
/// Discriminants are nonzero and stable: they are stored raw in the
/// persistent region and compared across reboots. `0` means "empty slot".
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FrameKind {
    /// Firmware boot, from recovery init until the boot-complete milestone.
    Boot = 1,
    /// Loading a project (auto-load at boot or client-requested).
    ProjectLoad = 2,
    /// Compiling (and linking) a shader.
    ShaderCompile = 3,
    /// Rendering/executing a node.
    NodeRender = 4,
}

impl FrameKind {
    /// Decode a raw discriminant read back from the persistent region.
    pub fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            1 => Some(Self::Boot),
            2 => Some(Self::ProjectLoad),
            3 => Some(Self::ShaderCompile),
            4 => Some(Self::NodeRender),
            _ => None,
        }
    }

    /// Short stable label, used when formatting paths for humans.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boot => "boot",
            Self::ProjectLoad => "project",
            Self::ShaderCompile => "shader-compile",
            Self::NodeRender => "node",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_u8_round_trips_all_kinds() {
        for kind in [
            FrameKind::Boot,
            FrameKind::ProjectLoad,
            FrameKind::ShaderCompile,
            FrameKind::NodeRender,
        ] {
            assert_eq!(FrameKind::from_u8(kind as u8), Some(kind));
        }
    }

    #[test]
    fn from_u8_rejects_empty_and_unknown() {
        assert_eq!(FrameKind::from_u8(0), None);
        assert_eq!(FrameKind::from_u8(200), None);
    }
}
