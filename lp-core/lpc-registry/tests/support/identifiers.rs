use lpc_model::{ArtifactLocation, AssetSource, NodeDefLocation};

pub fn artifact(path: &str) -> ArtifactLocation {
    ArtifactLocation::file(path)
}

pub fn artifact_asset(path: &str) -> AssetSource {
    AssetSource::artifact(artifact(path))
}

pub fn root_def(path: &str) -> NodeDefLocation {
    NodeDefLocation::artifact_root(artifact(path))
}
