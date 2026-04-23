//! TOML artifact loader.
//!
//! Reads a `.toml` file via [`LpFs`], deserializes it into a typed [`Artifact`]
//! struct, validates `schema_version`, and walks the loaded artifact to
//! materialize its [`ValueSpec`](crate::value_spec::ValueSpec) defaults at load
//! time (per `docs/design/lightplayer/quantity.md` §7 and non-negotiable §6).
//! Materialization uses [`LoadCtx`]; resulting
//! [`LpsValue`](crate::LpsValue)s are not cached in M3 — this module only checks
//! that [`Slot::default_value`](crate::shape::Slot::default_value) completes.
//!
//! # Errors
//!
//! - [`LoadError::Io`] — `LpFs::read_file` failed.
//! - [`LoadError::Utf8`] — file bytes are not UTF-8.
//! - [`LoadError::Parse`] — TOML does not match `T`’s serde shape.
//! - [`LoadError::SchemaVersion`] — on-disk `schema_version` ≠ `T::CURRENT_VERSION`.
//! - [`LoadError::Domain`] — reserved for domain validation during load (unused in this stub).
//!
//! Cross-artifact resolution (e.g. stack references) is out of scope; one file
//! per call.

use crate::error::DomainError;
use crate::schema::Artifact;
use crate::value_spec::LoadCtx;
use lp_model::path::LpPath;
use lpfs::{LpFs, error::FsError};

/// Errors the loader can return.
#[derive(Debug)]
pub enum LoadError {
    /// Underlying [`LpFs::read_file`] failure.
    Io(FsError),
    /// File content was not valid UTF-8.
    Utf8(core::str::Utf8Error),
    /// TOML parse failure.
    Parse(toml::de::Error),
    /// `schema_version` did not match the artifact's `CURRENT_VERSION`.
    SchemaVersion {
        artifact_kind: &'static str,
        expected: u32,
        found: u32,
    },
    /// Domain-layer error during materialization or validation.
    Domain(DomainError),
}

impl From<FsError> for LoadError {
    fn from(e: FsError) -> Self {
        LoadError::Io(e)
    }
}

impl From<core::str::Utf8Error> for LoadError {
    fn from(e: core::str::Utf8Error) -> Self {
        LoadError::Utf8(e)
    }
}

impl From<toml::de::Error> for LoadError {
    fn from(e: toml::de::Error) -> Self {
        LoadError::Parse(e)
    }
}

impl From<DomainError> for LoadError {
    fn from(e: DomainError) -> Self {
        LoadError::Domain(e)
    }
}

/// Load a TOML artifact through [`LpFs`] and validate its `schema_version`
/// against `T::CURRENT_VERSION`. Materializes embedded default values via a
/// throwaway [`LoadCtx`].
pub fn load_artifact<T, F>(fs: &F, path: &LpPath) -> Result<T, LoadError>
where
    T: Artifact + serde::de::DeserializeOwned,
    F: LpFs,
{
    let bytes = fs.read_file(path)?;
    let text = core::str::from_utf8(&bytes)?;
    let loaded: T = toml::from_str(text)?;

    let found = loaded.schema_version();
    if found != T::CURRENT_VERSION {
        return Err(LoadError::SchemaVersion {
            artifact_kind: T::KIND,
            expected: T::CURRENT_VERSION,
            found,
        });
    }

    let mut ctx = LoadCtx::default();
    walk_and_materialize(&loaded, &mut ctx);

    Ok(loaded)
}

fn walk_and_materialize<T: Artifact>(artifact: &T, ctx: &mut LoadCtx) {
    artifact.walk_slots(|slot| {
        let _ = slot.default_value(ctx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual::Pattern;
    use lp_model::path::LpPathBuf;
    use lpfs::LpFsMemory;

    fn fs_with(file: &str, body: &str) -> LpFsMemory {
        let fs = LpFsMemory::new();
        fs.write_file(LpPathBuf::from(file).as_path(), body.as_bytes())
            .unwrap();
        fs
    }

    #[test]
    fn loads_minimal_pattern() {
        let fs = fs_with(
            "/test.pattern.toml",
            r#"
            schema_version = 1
            title = "Tiny"
            [shader]
            glsl = "void main() {}"
        "#,
        );
        let p: Pattern =
            load_artifact(&fs, LpPathBuf::from("/test.pattern.toml").as_path()).unwrap();
        assert_eq!(p.title, "Tiny");
    }

    #[test]
    fn missing_file_returns_io_error() {
        let fs = LpFsMemory::new();
        let res: Result<Pattern, _> =
            load_artifact(&fs, LpPathBuf::from("/missing.toml").as_path());
        assert!(matches!(res, Err(LoadError::Io(_))));
    }

    #[test]
    fn invalid_toml_returns_parse_error() {
        let fs = fs_with("/bad.pattern.toml", "not = valid\nrandom = ");
        let res: Result<Pattern, _> =
            load_artifact(&fs, LpPathBuf::from("/bad.pattern.toml").as_path());
        assert!(matches!(res, Err(LoadError::Parse(_))));
    }

    #[test]
    fn schema_version_mismatch_is_caught() {
        let fs = fs_with(
            "/test.pattern.toml",
            r#"
            schema_version = 999
            title = "Wrong version"
            [shader]
            glsl = "void main() {}"
        "#,
        );
        let res: Result<Pattern, _> =
            load_artifact(&fs, LpPathBuf::from("/test.pattern.toml").as_path());
        match res {
            Err(LoadError::SchemaVersion {
                expected: 1,
                found: 999,
                ..
            }) => {}
            other => panic!("expected SchemaVersion mismatch, got {other:?}"),
        }
    }

    #[test]
    fn materializes_default_values_without_panic() {
        let fs = fs_with(
            "/full.pattern.toml",
            r#"
            schema_version = 1
            title = "Full"
            [shader]
            glsl = "void main() {}"
            [params.speed]
            kind    = "amplitude"
            default = 0.5
            [params.tint]
            kind    = "color"
            default = { space = "oklch", coords = [0.7, 0.15, 90] }
        "#,
        );
        let _: Pattern =
            load_artifact(&fs, LpPathBuf::from("/full.pattern.toml").as_path()).unwrap();
    }
}
