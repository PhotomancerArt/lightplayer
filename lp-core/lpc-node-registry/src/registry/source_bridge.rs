//! Resolve file-backed asset paths referenced from loaded defs.

use alloc::vec;
use alloc::vec::Vec;

use lpc_model::{ArtifactSpec, FixtureDef, NodeDef, ShaderSource, SourcePath};
use lpfs::LpPath;

use crate::RegistryError;

use super::def_walker::resolve_node_specifier;

/// Resolved file paths for assets referenced by `def` (empty if inline / none).
pub fn asset_paths_for_def(
    def: &NodeDef,
    containing_file: &LpPath,
) -> Result<Vec<lpc_model::LpPathBuf>, RegistryError> {
    match def {
        NodeDef::Shader(shader) => paths_for_shader(shader.shader_source(), containing_file),
        NodeDef::ComputeShader(shader) => paths_for_shader(shader.shader_source(), containing_file),
        NodeDef::Fixture(fixture) => paths_for_fixture(fixture, containing_file),
        _ => Ok(Vec::new()),
    }
}

fn paths_for_shader(
    source: &ShaderSource,
    containing_file: &LpPath,
) -> Result<Vec<lpc_model::LpPathBuf>, RegistryError> {
    let ShaderSource::Path(path) = source else {
        return Ok(Vec::new());
    };
    Ok(vec![resolve_source_path(containing_file, path.value())?])
}

fn paths_for_fixture(
    fixture: &FixtureDef,
    containing_file: &LpPath,
) -> Result<Vec<lpc_model::LpPathBuf>, RegistryError> {
    use lpc_model::nodes::fixture::MappingConfig;
    let MappingConfig::SvgPath { source, .. } = fixture.mapping.value() else {
        return Ok(Vec::new());
    };
    Ok(vec![resolve_source_path(containing_file, source.value())?])
}

fn resolve_source_path(
    containing_file: &LpPath,
    path: &SourcePath,
) -> Result<lpc_model::LpPathBuf, RegistryError> {
    let specifier = ArtifactSpec::path(path.as_path_buf());
    resolve_node_specifier(containing_file, &specifier)
}
