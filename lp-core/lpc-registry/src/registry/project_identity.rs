//! Who a loaded project says it is: `project.json`'s `name` and `uid`, kept
//! from the manifest the load gate already parses
//! ([`ProjectRegistry`](super::project_registry::ProjectRegistry) reads it
//! at every load and again when a refresh carries `/project.json`), so
//! nothing reads the file a second time to answer "what is playing?".
//!
//! The board's relay reports it (the `Project` frame: the name in the
//! clear, the uid only as a keyed tag). **The uid is a read capability**:
//! a link-viewable project is fetched by it, so it is never logged
//! (`Debug` prints only whether there is one).

use alloc::string::String;

use lpc_model::ProjectManifest;

/// See the module doc.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ProjectIdentity {
    /// `project.json`'s `name`, when it has one.
    pub name: Option<String>,
    /// `project.json`'s `uid`, when it has one. A read capability.
    pub uid: Option<String>,
}

impl ProjectIdentity {
    /// Take the identity out of a parsed manifest (moved, not copied).
    #[must_use]
    pub fn from_manifest(manifest: &mut ProjectManifest) -> Self {
        Self {
            name: manifest.name.take(),
            uid: manifest.uid.take(),
        }
    }
}

/// The uid is a capability: never in a log line.
impl core::fmt::Debug for ProjectIdentity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ProjectIdentity")
            .field("name", &self.name)
            .field("has_uid", &self.uid.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_moves_out_of_the_manifest_and_debug_hides_the_uid() {
        let mut manifest = ProjectManifest::read_json(
            r#"{"format":6,"uid":"prj7m3qk2x9z4w8v6t5r1n0p2a4c","name":"Rocaille"}"#,
        )
        .expect("a manifest");
        let identity = ProjectIdentity::from_manifest(&mut manifest);
        assert_eq!(identity.name.as_deref(), Some("Rocaille"));
        assert_eq!(
            identity.uid.as_deref(),
            Some("prj7m3qk2x9z4w8v6t5r1n0p2a4c")
        );
        let text = alloc::format!("{identity:?}");
        assert!(text.contains("Rocaille"), "{text}");
        assert!(!text.contains("prj7m3qk"), "{text}");
    }
}
