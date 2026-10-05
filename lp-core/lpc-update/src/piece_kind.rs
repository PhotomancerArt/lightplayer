//! The two pieces of a split image an update moves: the core and the engine.

use serde::{Deserialize, Serialize};

/// Which piece a message, a request or a record is about. On the wire it is
/// one byte, the letter: `C` core, `E` engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PieceKind {
    Core,
    Engine,
}

impl PieceKind {
    /// The wire byte.
    #[must_use]
    pub const fn byte(self) -> u8 {
        match self {
            Self::Core => b'C',
            Self::Engine => b'E',
        }
    }

    /// The piece a wire byte names, if any.
    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            b'C' => Some(Self::Core),
            b'E' => Some(Self::Engine),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_bytes_are_the_letters_and_round_trip() {
        assert_eq!(PieceKind::Core.byte(), b'C');
        assert_eq!(PieceKind::Engine.byte(), b'E');
        for k in [PieceKind::Core, PieceKind::Engine] {
            assert_eq!(PieceKind::from_byte(k.byte()), Some(k));
        }
        assert_eq!(PieceKind::from_byte(b'X'), None);
    }

    #[test]
    fn json_is_the_lowercase_word() {
        assert_eq!(
            serde_json::to_string(&PieceKind::Engine).unwrap(),
            "\"engine\""
        );
    }
}
