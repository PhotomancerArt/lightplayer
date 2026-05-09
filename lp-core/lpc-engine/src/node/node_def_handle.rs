//! Runtime handle to a node's authored definition.

use lpc_model::SlotPath;

use crate::artifact::ArtifactId;

/// Address of an authored node definition inside the artifact store.
///
/// The current loader stores every node definition at the root of its artifact,
/// so handles are `artifact + SlotPath::root()`. Non-root paths are reserved for
/// future inline node definitions nested inside another artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeDefHandle {
    artifact: ArtifactId,
    path: SlotPath,
}

impl NodeDefHandle {
    /// Handle for a node definition that is the artifact root.
    pub fn artifact_root(artifact: ArtifactId) -> Self {
        Self {
            artifact,
            path: SlotPath::root(),
        }
    }

    pub fn new(artifact: ArtifactId, path: SlotPath) -> Self {
        Self { artifact, path }
    }

    pub fn artifact(&self) -> ArtifactId {
        self.artifact
    }

    pub fn path(&self) -> &SlotPath {
        &self.path
    }

    pub fn is_artifact_root(&self) -> bool {
        self.path.is_root()
    }
}
