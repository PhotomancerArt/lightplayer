//! Engine-side orchestration from [`ArtifactLocation`](super::ArtifactLocation) to typed [`SrcArtifact`] loads via
//! [`lpc_source::load_artifact`], mapping [`lpc_source::LoadError`] into [`ArtifactError`].
//!
//! Use with [`super::ArtifactManager::load_with`], e.g.
//! `manager.load_with(&artifact_id, frame, |location| load_source_artifact(fs, location))`.

use alloc::format;

use lpc_source::{ArtifactReadRoot, LoadError, SrcArtifact, load_artifact};

use super::{ArtifactError, ArtifactLocation};

/// Load the TOML artifact referenced by `location` through `fs` and validate schema version.
pub fn load_source_artifact<A, R>(fs: &R, location: &ArtifactLocation) -> Result<A, ArtifactError>
where
    A: SrcArtifact + serde::de::DeserializeOwned,
    R: ArtifactReadRoot,
    R::Err: core::fmt::Debug,
{
    match location {
        ArtifactLocation::File(path) => load_artifact(fs, path.as_path()).map_err(map_load_error),
    }
}

fn map_load_error<E: core::fmt::Debug>(err: LoadError<E>) -> ArtifactError {
    match err {
        LoadError::Io(e) => ArtifactError::Load(format!("artifact io: {e:?}")),
        LoadError::Utf8(e) => ArtifactError::Load(format!("artifact utf8: {e}")),
        LoadError::Parse(e) => ArtifactError::Load(format!("artifact parse: {e}")),
        LoadError::SchemaVersion {
            artifact_kind,
            expected,
            found,
        } => ArtifactError::Load(format!(
            "schema version mismatch for {artifact_kind}: expected {expected}, found {found}"
        )),
        LoadError::Domain(d) => ArtifactError::Load(format!("{d}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::string::ToString;
    use alloc::vec::Vec;

    use lpc_model::{FrameId, LpPath, LpPathBuf};
    use lpc_source::SrcArtifactSpec;
    use lpc_source::SrcSlot;

    use crate::artifact::{ArtifactManager, ArtifactState};

    #[derive(Debug, serde::Deserialize)]
    struct DummySrcArtifact {
        schema_version: u32,
        title: String,
    }

    impl SrcArtifact for DummySrcArtifact {
        const KIND: &'static str = "pattern";
        const CURRENT_VERSION: u32 = 1;

        fn schema_version(&self) -> u32 {
            self.schema_version
        }

        fn walk_slots<F: FnMut(&SrcSlot)>(&self, _f: F) {}
    }

    struct MockFs {
        files: Vec<(String, Vec<u8>)>,
    }

    impl MockFs {
        fn with_file(path: &str, body: &str) -> Self {
            let key = LpPathBuf::from(path).as_str().to_string();
            MockFs {
                files: alloc::vec![(key, body.as_bytes().to_vec())],
            }
        }
    }

    impl ArtifactReadRoot for MockFs {
        type Err = MockFsErr;

        fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, MockFsErr> {
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
    fn load_source_artifact_loads_dummy_toml() {
        let fs = MockFs::with_file(
            "/src/pattern.lp.toml",
            r#"
schema_version = 1
title = "hi"
"#,
        );
        let location = ArtifactLocation::file_from_spec(&SrcArtifactSpec(String::from(
            "/src/pattern.lp.toml",
        )));
        let a: DummySrcArtifact = load_source_artifact(&fs, &location).unwrap();
        assert_eq!(a.title, "hi");
    }

    #[test]
    fn schema_version_mismatch_maps_to_load_error() {
        let fs = MockFs::with_file(
            "/bad.toml",
            r#"
schema_version = 99
title = "x"
"#,
        );
        let location =
            ArtifactLocation::file_from_spec(&SrcArtifactSpec(String::from("/bad.toml")));
        let err = load_source_artifact::<DummySrcArtifact, _>(&fs, &location).unwrap_err();
        match err {
            ArtifactError::Load(s) => {
                assert!(s.contains("schema version mismatch"));
                assert!(s.contains("pattern"));
                assert!(s.contains("expected 1"));
                assert!(s.contains("found 99"));
            }
            other => panic!("expected Load error, got {other:?}"),
        }
    }

    #[test]
    fn artifact_manager_load_with_source_loader() {
        let fs = MockFs::with_file(
            "/eff.toml",
            r#"
schema_version = 1
title = "from-manager"
"#,
        );
        let mut m: ArtifactManager<DummySrcArtifact> = ArtifactManager::new();
        let r = m.acquire_location(ArtifactLocation::file("/eff.toml"), FrameId::new(1));
        m.load_with(&r, FrameId::new(2), |location| {
            load_source_artifact(&fs, location)
        })
        .unwrap();
        match &m.entry(&r).unwrap().state {
            ArtifactState::Loaded(a) => assert_eq!(a.title, "from-manager"),
            s => panic!("expected Loaded, got {s:?}"),
        }
    }
}
