//! Resolve production def source paths and materialize versions (internal).

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::{
    ArtifactLocator, FixtureDef, NodeDef, Revision, ShaderSource, SourceFileSlot, SourcePath,
};
use lpfs::{LpFs, LpPath};

use crate::source::{SourceDiagnosticCtx, materialize_source, resolve_source_file};
use crate::{ArtifactStore, RegistryError};

use super::def_walker::resolve_node_locator;

/// Resolved file path backing a def's authored source (empty if inline / none).
pub fn source_paths_for_def(
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
    let locator = ArtifactLocator::path(path.as_path_buf());
    resolve_node_locator(containing_file, &locator)
}

pub fn materialize_version_for_path(
    store: &mut ArtifactStore,
    fs: &dyn LpFs,
    containing_file: &LpPath,
    resolved_path: &lpc_model::LpPathBuf,
    authored_path: &str,
    frame: lpc_model::Revision,
) -> Result<Revision, RegistryError> {
    let slot = SourceFileSlot::from_path(SourcePath::from(authored_path));
    let reference = resolve_source_file(store, containing_file, &slot, frame).map_err(|err| {
        RegistryError::LocatorResolution {
            message: alloc::format!("resolve `{resolved_path:?}`: {err:?}"),
        }
    })?;
    let ctx = SourceDiagnosticCtx {
        containing_file: String::from(containing_file.as_str()),
        slot_path: None,
    };
    let materialized =
        materialize_source(store, fs, &reference, &slot, &ctx, None).map_err(|err| {
            RegistryError::LocatorResolution {
                message: alloc::format!("materialize `{resolved_path:?}`: {err:?}"),
            }
        })?;
    Ok(materialized.version)
}

pub fn materialize_version_for_def_path(
    store: &mut ArtifactStore,
    fs: &dyn LpFs,
    containing_file: &LpPath,
    def: &NodeDef,
    resolved_path: &lpc_model::LpPathBuf,
    frame: lpc_model::Revision,
) -> Result<Revision, RegistryError> {
    let authored = authored_path_for_resolved(def, resolved_path.as_str())?;
    materialize_version_for_path(store, fs, containing_file, resolved_path, &authored, frame)
}

fn authored_path_for_resolved(def: &NodeDef, resolved: &str) -> Result<String, RegistryError> {
    match def {
        NodeDef::Shader(shader) => authored_shader_path(shader.shader_source(), resolved),
        NodeDef::ComputeShader(shader) => authored_shader_path(shader.shader_source(), resolved),
        NodeDef::Fixture(fixture) => {
            use lpc_model::nodes::fixture::MappingConfig;
            let MappingConfig::SvgPath { source, .. } = fixture.mapping.value() else {
                return Err(RegistryError::LocatorResolution {
                    message: String::from("fixture has no svg path source"),
                });
            };
            Ok(String::from(source.value().as_str()))
        }
        _ => Err(RegistryError::LocatorResolution {
            message: String::from("def has no file source"),
        }),
    }
}

fn authored_shader_path(source: &ShaderSource, _resolved: &str) -> Result<String, RegistryError> {
    let ShaderSource::Path(path) = source else {
        return Err(RegistryError::LocatorResolution {
            message: String::from("shader has inline source"),
        });
    };
    Ok(String::from(path.value().as_str()))
}
