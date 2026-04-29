//! TOML artifact loader.
//!
//! Reads a `.toml` file via [`ArtifactReadRoot`], deserializes it into a typed [`Artifact`]
//! struct, validates `schema_version`, and walks the loaded artifact to
//! materialize its [`ValueSpec`](crate::value_spec::ValueSpec) defaults at load
//! time (per `docs/design/lightplayer/quantity.md` §7 and non-negotiable §6).
//! Materialization uses [`LoadCtx`]; resulting
//! [`LpsValue`](crate::LpsValue)s are not cached in M3 — this module only checks
//! that [`Slot::default_value`](crate::shape::Slot::default_value) completes.
//!
//! # Errors
//!
//! - [`LoadError::Io`] — `read_file` failed (`ArtifactReadRoot::Err`).
//! - [`LoadError::Utf8`] — file bytes are not UTF-8.
//! - [`LoadError::Parse`] — TOML does not match `T`’s serde shape.
//! - [`LoadError::SchemaVersion`] — on-disk `schema_version` ≠ `T::CURRENT_VERSION`.
//! - [`LoadError::Domain`] — reserved for domain validation during load (unused in this stub).
//!
//! Cross-artifact resolution (e.g. stack references) is out of scope; one file
//! per call.

use crate::error::DomainError;
use crate::path::LpPath;
use crate::schema::Artifact;
use crate::value_spec::LoadCtx;

/// Narrow filesystem surface for [`load_artifact`].
///
/// Implemented for [`lpfs::LpFs`] implementations in the `lpfs` crate so `lpc-model`
/// does not depend on `lpfs` (avoids `lpc-model` ↔ `lpfs` cycles).
pub trait ArtifactReadRoot {
    /// Low-level error returned when reading bytes fails.
    type Err;

    /// Read full file contents at `path`.
    fn read_file(&self, path: &LpPath) -> Result<alloc::vec::Vec<u8>, Self::Err>;
}

/// Errors the loader can return.
#[derive(Debug)]
pub enum LoadError<E> {
    /// Underlying [`ArtifactReadRoot::read_file`] failure.
    Io(E),
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

impl<E> From<core::str::Utf8Error> for LoadError<E> {
    fn from(e: core::str::Utf8Error) -> Self {
        LoadError::Utf8(e)
    }
}

impl<E> From<toml::de::Error> for LoadError<E> {
    fn from(e: toml::de::Error) -> Self {
        LoadError::Parse(e)
    }
}

impl<E> From<DomainError> for LoadError<E> {
    fn from(e: DomainError) -> Self {
        LoadError::Domain(e)
    }
}

/// Load a TOML artifact through [`ArtifactReadRoot`] and validate its `schema_version`
/// against `T::CURRENT_VERSION`. Materializes embedded default values via a
/// throwaway [`LoadCtx`].
pub fn load_artifact<T, R>(fs: &R, path: &LpPath) -> Result<T, LoadError<R::Err>>
where
    T: Artifact + serde::de::DeserializeOwned,
    R: ArtifactReadRoot,
{
    let bytes = fs.read_file(path).map_err(LoadError::Io)?;
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
    use crate::path::LpPathBuf;

    /// Minimal deserialize target for loader tests (visual `Pattern` lives in `lpv-model`).
    #[derive(Debug, serde::Deserialize)]
    struct LoadTestArtifact {
        schema_version: u32,
        title: String,
    }

    impl Artifact for LoadTestArtifact {
        const KIND: &'static str = "pattern";
        const CURRENT_VERSION: u32 = 1;

        fn schema_version(&self) -> u32 {
            self.schema_version
        }
    }

    struct MockFs {
        files: alloc::vec::Vec<(alloc::string::String, alloc::vec::Vec<u8>)>,
    }

    impl MockFs {
        fn with_file(path: &str, body: &str) -> Self {
            let key = LpPathBuf::from(path).as_str().to_string();
            MockFs {
                files: alloc::vec![(key, body.as_bytes().to_vec())],
            }
        }

        fn empty() -> Self {
            MockFs {
                files: alloc::vec::Vec::new(),
            }
        }
    }

    impl ArtifactReadRoot for MockFs {
        type Err = MockFsErr;

        fn read_file(&self, path: &LpPath) -> Result<alloc::vec::Vec<u8>, MockFsErr> {
            let s = path.as_str();
            self.files
                .iter()
                .find(|(p, _)| p == s)
                .map(|(_, b)| b.clone())
                .ok_or(MockFsErr)
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct MockFsErr;

    #[test]
    fn loads_minimal_pattern_shaped_toml() {
        let fs = MockFs::with_file(
            "/test.pattern.toml",
            r#"
            schema_version = 1
            title = "Tiny"
            [shader]
            glsl = "void main() {}"
        "#,
        );
        let p: LoadTestArtifact =
            load_artifact(&fs, LpPathBuf::from("/test.pattern.toml").as_path()).unwrap();
        assert_eq!(p.title, "Tiny");
    }

    #[test]
    fn missing_file_returns_io_error() {
        let fs = MockFs::empty();
        let res: Result<LoadTestArtifact, _> =
            load_artifact(&fs, LpPathBuf::from("/missing.toml").as_path());
        assert!(matches!(res, Err(LoadError::Io(MockFsErr))));
    }

    #[test]
    fn invalid_toml_returns_parse_error() {
        let fs = MockFs::with_file("/bad.pattern.toml", "not = valid\nrandom = ");
        let res: Result<LoadTestArtifact, _> =
            load_artifact(&fs, LpPathBuf::from("/bad.pattern.toml").as_path());
        assert!(matches!(res, Err(LoadError::Parse(_))));
    }

    #[test]
    fn schema_version_mismatch_is_caught() {
        let fs = MockFs::with_file(
            "/test.pattern.toml",
            r#"
            schema_version = 999
            title = "Wrong version"
            [shader]
            glsl = "void main() {}"
        "#,
        );
        let res: Result<LoadTestArtifact, _> =
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
}
