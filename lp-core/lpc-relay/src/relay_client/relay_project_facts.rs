//! What the board knows about the project it plays, as the relay client is
//! told it.

use alloc::string::String;

/// What the board knows about its loaded project. Held in RAM only: the
/// uid and the package hash are read capabilities and never cross the
/// device leg; the client sends their tags (`lpc_relay::RelayProject`).
#[derive(Clone, PartialEq, Eq)]
pub struct RelayProjectFacts {
    /// The project's name for people (project.json's `name`, else its
    /// folder's). The client cuts it to
    /// [`MAX_PROJECT_NAME_BYTES`](crate::MAX_PROJECT_NAME_BYTES).
    pub name: String,
    /// The project's uid, when it has one.
    pub uid: Option<String>,
    /// The project's package hash, when the board computed one.
    pub content_hash: Option<[u8; 32]>,
}

/// The uid and the hash are capabilities: never in a log line. The name is
/// not (the leg carries it in the clear).
impl core::fmt::Debug for RelayProjectFacts {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RelayProjectFacts")
            .field("name", &self.name)
            .field("has_uid", &self.uid.is_some())
            .field("has_content_hash", &self.content_hash.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_uid_or_the_hash() {
        let facts = RelayProjectFacts {
            name: String::from("Rocaille"),
            uid: Some(String::from("prj7m3qk2x9z4w8v6t5r1n0p2a4c")),
            content_hash: Some([0xab; 32]),
        };
        let text = alloc::format!("{facts:?}");
        assert!(text.contains("Rocaille"), "{text}");
        assert!(!text.contains("prj7m3qk"), "{text}");
        assert!(!text.contains("171"), "{text}");
    }
}
