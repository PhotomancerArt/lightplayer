//! Materialize [`SourceFileRef`] to transient UTF-8 text.

use alloc::format;
use alloc::string::{String, ToString};

use lpc_model::{LpPathBuf, Revision, SlotPath, SourceFileSlot, SourcePath};
use lpfs::LpFs;

use crate::edit::{ArtifactOverlay, PendingAsset};
use crate::{ArtifactError, ArtifactReadFailure, ArtifactStore};

use super::{MaterializedSource, ResolveError, SourceFileRef};

/// Context for stable compile/diagnostic labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDiagnosticCtx {
    pub containing_file: String,
    pub slot_path: Option<SlotPath>,
}

/// Errors from [`materialize_source`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializeError {
    Unsupported,
    MissingInlineBody,
    Utf8 { message: String },
    Resolve(ResolveError),
    Artifact(ArtifactError),
}

impl From<ResolveError> for MaterializeError {
    fn from(err: ResolveError) -> Self {
        Self::Resolve(err)
    }
}

impl From<ArtifactError> for MaterializeError {
    fn from(err: ArtifactError) -> Self {
        Self::Artifact(err)
    }
}

/// Read source bytes/text transiently and compute the effective revision.
///
/// When `overlay` is present, pending bytes for `resolved_path` take precedence
/// over the committed store/fs read.
pub fn materialize_source(
    store: &mut ArtifactStore,
    fs: &dyn LpFs,
    reference: &SourceFileRef,
    slot: &SourceFileSlot,
    ctx: &SourceDiagnosticCtx,
    overlay: Option<&ArtifactOverlay>,
) -> Result<MaterializedSource, MaterializeError> {
    match reference {
        SourceFileRef::File {
            location,
            authored_path,
            resolved_path,
            ..
        } => {
            if let Some(overlay) = overlay {
                if let Some(materialized) =
                    materialize_file_artifact_overlay(overlay, resolved_path, authored_path, slot)?
                {
                    return Ok(materialized);
                }
            }
            let bytes = store.read_bytes(location, fs)?;
            let text = core::str::from_utf8(&bytes).map_err(|err| MaterializeError::Utf8 {
                message: format!("{err}"),
            })?;
            let artifact_revision = store.revision(location).unwrap_or_else(Revision::default);
            Ok(MaterializedSource {
                version: slot.revision().max(artifact_revision),
                text: String::from(text),
                diagnostic_name: authored_path.as_str().to_string(),
            })
        }
        SourceFileRef::Inline { extension, .. } => {
            let (_, text) = slot
                .inline_value()
                .ok_or(MaterializeError::MissingInlineBody)?;
            Ok(MaterializedSource {
                version: slot.revision(),
                text: String::from(text),
                diagnostic_name: inline_diagnostic_name(ctx, extension),
            })
        }
        SourceFileRef::Url { .. } => Err(MaterializeError::Unsupported),
    }
}

fn materialize_file_artifact_overlay(
    overlay: &ArtifactOverlay,
    resolved_path: &LpPathBuf,
    authored_path: &SourcePath,
    slot: &SourceFileSlot,
) -> Result<Option<MaterializedSource>, MaterializeError> {
    let location = crate::ArtifactLoc::location_for_path(resolved_path.as_path());
    let Some(pending) = overlay.pending_at(&location) else {
        return Ok(None);
    };
    match &pending.asset_edit {
        PendingAsset::ReplaceBody(bytes) => {
            let text = core::str::from_utf8(bytes).map_err(|err| MaterializeError::Utf8 {
                message: format!("{err}"),
            })?;
            Ok(Some(MaterializedSource {
                version: slot.revision(),
                text: String::from(text),
                diagnostic_name: authored_path.as_str().to_string(),
            }))
        }
        PendingAsset::Delete => Err(MaterializeError::Artifact(ArtifactError::Read(
            ArtifactReadFailure::Deleted,
        ))),
        PendingAsset::None => Ok(None),
    }
}

fn inline_diagnostic_name(ctx: &SourceDiagnosticCtx, extension: &str) -> String {
    match &ctx.slot_path {
        Some(path) => format!("{}:{}.{}", ctx.containing_file, path, extension),
        None => format!("{}:source.{}", ctx.containing_file, extension),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ArtifactReadFailure;
    use crate::edit::{ArtifactOverlay, PendingAsset};
    use crate::source::resolve_source_file;
    use lpc_model::Revision;
    use lpfs::{FsEvent, FsEventKind, LpFsMemory, LpPath, LpPathBuf};

    fn write_file(fs: &mut LpFsMemory, path: &str, content: &[u8]) {
        fs.write_file_mut(LpPathBuf::from(path).as_path(), content)
            .unwrap();
    }

    fn fs_change(path: &str) -> FsEvent {
        FsEvent {
            path: LpPathBuf::from(path),
            kind: FsEventKind::Modify,
        }
    }

    fn diag_ctx() -> SourceDiagnosticCtx {
        SourceDiagnosticCtx {
            containing_file: String::from("/shader.toml"),
            slot_path: None,
        }
    }

    #[test]
    fn materialize_file_reads_utf8() {
        let mut fs = LpFsMemory::new();
        write_file(&mut fs, "/shader.glsl", b"void main() {}");

        let slot = SourceFileSlot::from_path("./shader.glsl");
        let slot_revision = slot.revision();
        let mut store = ArtifactStore::new();
        let containing = LpPath::new("/shader.toml");
        let frame = Revision::new(1);
        let reference = resolve_source_file(&mut store, containing, &slot, frame).expect("resolve");

        let materialized =
            materialize_source(&mut store, &fs, &reference, &slot, &diag_ctx(), None)
                .expect("read");

        assert!(materialized.text.contains("main"));
        assert_eq!(materialized.diagnostic_name, "./shader.glsl");
        assert_eq!(materialized.version, slot_revision.max(frame));
    }

    #[test]
    fn materialize_inline_uses_slot_text_and_diagnostic_name() {
        let slot = SourceFileSlot::from_inline("glsl", "void main() {}");
        let reference = SourceFileRef::Inline {
            extension: String::from("glsl"),
            slot_revision: slot.revision(),
        };

        let materialized = materialize_source(
            &mut ArtifactStore::new(),
            &LpFsMemory::new(),
            &reference,
            &slot,
            &diag_ctx(),
            None,
        )
        .expect("read");

        assert!(materialized.text.contains("main"));
        assert_eq!(materialized.diagnostic_name, "/shader.toml:source.glsl");
        assert_eq!(materialized.version, slot.revision());
    }

    #[test]
    fn file_bump_increases_version_without_slot_edit() {
        let mut fs = LpFsMemory::new();
        write_file(&mut fs, "/shader.glsl", b"v1");

        let slot = SourceFileSlot::from_path("./shader.glsl");
        let slot_revision = slot.revision();
        let mut store = ArtifactStore::new();
        let containing = LpPath::new("/shader.toml");
        let reference =
            resolve_source_file(&mut store, containing, &slot, Revision::new(1)).expect("resolve");

        let first = materialize_source(&mut store, &fs, &reference, &slot, &diag_ctx(), None)
            .expect("read");
        assert_eq!(first.text, "v1");
        assert_eq!(first.version, slot_revision.max(Revision::new(1)));

        write_file(&mut fs, "/shader.glsl", b"v2");
        store.apply_fs_changes(&[fs_change("/shader.glsl")], Revision::new(5));

        let second = materialize_source(&mut store, &fs, &reference, &slot, &diag_ctx(), None)
            .expect("read");
        assert_eq!(second.text, "v2");
        assert_eq!(second.version, slot_revision.max(Revision::new(5)));
        assert!(second.version >= first.version);
    }

    #[test]
    fn overlay_setbytes_replaces_committed_file_text() {
        let mut fs = LpFsMemory::new();
        write_file(&mut fs, "/shader.glsl", b"v1");

        let slot = SourceFileSlot::from_path("./shader.glsl");
        let mut store = ArtifactStore::new();
        let containing = LpPath::new("/shader.toml");
        let reference =
            resolve_source_file(&mut store, containing, &slot, Revision::new(1)).expect("resolve");

        let mut overlay = ArtifactOverlay::new();
        overlay
            .ensure_pending(crate::ArtifactLoc::file("/shader.glsl"))
            .set_asset(PendingAsset::ReplaceBody(b"v2-overlay".to_vec()));

        let committed =
            materialize_source(&mut store, &fs, &reference, &slot, &diag_ctx(), None).unwrap();
        assert_eq!(committed.text, "v1");

        let effective = materialize_source(
            &mut store,
            &fs,
            &reference,
            &slot,
            &diag_ctx(),
            Some(&overlay),
        )
        .unwrap();
        assert_eq!(effective.text, "v2-overlay");
    }

    #[test]
    fn overlay_delete_yields_deleted_error() {
        let mut fs = LpFsMemory::new();
        write_file(&mut fs, "/shader.glsl", b"v1");

        let slot = SourceFileSlot::from_path("./shader.glsl");
        let mut store = ArtifactStore::new();
        let containing = LpPath::new("/shader.toml");
        let reference =
            resolve_source_file(&mut store, containing, &slot, Revision::new(1)).expect("resolve");

        let mut overlay = ArtifactOverlay::new();
        overlay
            .ensure_pending(crate::ArtifactLoc::file("/shader.glsl"))
            .set_asset(PendingAsset::Delete);

        let err = materialize_source(
            &mut store,
            &fs,
            &reference,
            &slot,
            &diag_ctx(),
            Some(&overlay),
        )
        .unwrap_err();
        assert_eq!(
            err,
            MaterializeError::Artifact(ArtifactError::Read(ArtifactReadFailure::Deleted))
        );
    }

    #[test]
    fn url_ref_is_unsupported() {
        let reference = SourceFileRef::Url {
            url: String::from("https://example.com/shader.glsl"),
        };
        let err = materialize_source(
            &mut ArtifactStore::new(),
            &LpFsMemory::new(),
            &reference,
            &SourceFileSlot::default(),
            &diag_ctx(),
            None,
        )
        .unwrap_err();
        assert_eq!(err, MaterializeError::Unsupported);
    }
}
