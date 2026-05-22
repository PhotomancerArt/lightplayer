//! Resolve authored [`SourceFileSlot`] to [`SourceFileRef`].

use alloc::string::String;

use lpc_model::{ArtifactLocator, Revision, SourceFileBacking, SourceFileSlot, SourcePath};
use lpfs::LpPath;

use crate::artifact::ArtifactLocation;
use crate::registry::resolve_node_locator;
use crate::{ArtifactStore, RegistryError};

use super::SourceFileRef;

/// Errors from [`resolve_source_file`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    LocatorResolution { message: String },
}

impl From<RegistryError> for ResolveError {
    fn from(err: RegistryError) -> Self {
        match err {
            RegistryError::LocatorResolution { message } => Self::LocatorResolution { message },
            other => Self::LocatorResolution {
                message: alloc::format!("{other:?}"),
            },
        }
    }
}

/// Resolve an authored slot to a handle-only ref, acquiring file artifacts in `store`.
pub fn resolve_source_file(
    store: &mut ArtifactStore,
    containing_file: &LpPath,
    slot: &SourceFileSlot,
    frame: Revision,
) -> Result<SourceFileRef, ResolveError> {
    match slot.backing() {
        SourceFileBacking::Path(path) => resolve_path_backing(store, containing_file, path, frame),
        SourceFileBacking::Inline { extension, .. } => Ok(SourceFileRef::Inline {
            extension: extension.clone(),
            slot_revision: slot.revision(),
        }),
    }
}

fn resolve_path_backing(
    store: &mut ArtifactStore,
    containing_file: &LpPath,
    path: &SourcePath,
    frame: Revision,
) -> Result<SourceFileRef, ResolveError> {
    let locator = ArtifactLocator::path(path.as_path_buf());
    let resolved_path = resolve_node_locator(containing_file, &locator)?;
    let extension = resolved_path.extension().unwrap_or("").into();
    let location = ArtifactLocation::file(resolved_path.clone());
    let artifact_id = store.acquire_location(location, frame);
    Ok(SourceFileRef::File {
        artifact_id,
        authored_path: path.clone(),
        resolved_path,
        extension,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::Revision;

    #[test]
    fn resolve_path_acquires_artifact() {
        let slot = SourceFileSlot::from_path("./shader.glsl");
        let mut store = ArtifactStore::new();
        let containing = LpPath::new("/project/shader.toml");

        let reference =
            resolve_source_file(&mut store, containing, &slot, Revision::new(2)).expect("resolve");

        let SourceFileRef::File {
            artifact_id,
            authored_path,
            resolved_path,
            extension,
        } = reference
        else {
            panic!("expected file ref");
        };
        assert_eq!(authored_path.as_str(), "./shader.glsl");
        assert_eq!(resolved_path.as_str(), "/project/shader.glsl");
        assert_eq!(extension, "glsl");
        assert_eq!(store.refcount(&artifact_id), Some(1));
    }

    #[test]
    fn resolve_inline_carries_slot_revision() {
        let slot = SourceFileSlot::from_inline("glsl", "void main() {}");
        let mut store = ArtifactStore::new();

        let reference =
            resolve_source_file(&mut store, LpPath::new("/a.toml"), &slot, Revision::new(1))
                .expect("resolve");

        assert_eq!(
            reference,
            SourceFileRef::Inline {
                extension: String::from("glsl"),
                slot_revision: slot.revision(),
            }
        );
    }
}
