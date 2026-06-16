use lpc_model::{ArtifactLocation, AssetLocation, NodeDefLocation};

pub fn artifact(path: &str) -> ArtifactLocation {
    ArtifactLocation::file(path)
}

pub fn artifact_asset(path: &str) -> AssetLocation {
    AssetLocation::artifact(artifact(path))
}

pub fn root_def(path: &str) -> NodeDefLocation {
    NodeDefLocation::artifact_root(artifact(path))
}
