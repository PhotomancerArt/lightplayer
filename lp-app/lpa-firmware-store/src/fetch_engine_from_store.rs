//! An engine for a board, from the store: the building block the update
//! protocol orders after the cache and before a read-back.

use lpc_firmware_release::{BuildId, OtaManifest, ReleaseSelector, TargetName};

use crate::firmware_fetch::FirmwareFetch;
use crate::firmware_store::{FirmwareStore, StoreError};

/// Fetch the engine of `target`'s build `build_id` and check it is the one a
/// board reported (`expected_engine_sha256`, its `engineSha256`).
///
/// - `Ok(None)`: this build is not in the store — a dev build id (answered
///   **without fetching**: dev builds are never in the store) or a release
///   the store does not have.
/// - `Err(StoreError::EngineMismatch)`: the store has that build, but not
///   those engine bytes. The caller surfaces it; it is never papered over.
/// - `Ok(Some((manifest, bytes)))`: the manifest's `build_id()` and
///   `target` are the ones asked for and the bytes are verified against it.
pub async fn fetch_engine_from_store<F: FirmwareFetch>(
    store: &FirmwareStore<F>,
    target: &str,
    build_id: &str,
    expected_engine_sha256: &str,
) -> Result<Option<(OtaManifest, Vec<u8>)>, StoreError> {
    let Some(build_id) = BuildId::parse(build_id) else {
        log::debug!("firmware store: {build_id} is not a release build; not looked up");
        return Ok(None);
    };
    let target = TargetName::parse(target).ok_or_else(|| StoreError::BadTarget(target.into()))?;
    let Some(manifest) = store
        .manifest(&target, &ReleaseSelector::BuildId(build_id))
        .await?
    else {
        return Ok(None);
    };
    if manifest.engine.sha256 != expected_engine_sha256.to_ascii_lowercase() {
        return Err(StoreError::EngineMismatch {
            expected: expected_engine_sha256.to_string(),
            actual: manifest.engine.sha256.clone(),
        });
    }
    let engine_file = manifest.engine.file.clone();
    let bytes = store.file(&manifest, &engine_file).await?;
    Ok(Some((manifest, bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_block_on::block_on;
    use crate::test_fetch::{FakeFetch, release_fixture};

    const BUILD_ID: &str = "2026.10.05-3+103285d5d05e";

    #[test]
    fn the_happy_path_returns_verified_bytes() {
        let release = release_fixture();
        let fetch = release.fetch("https://store.test");
        let store = FirmwareStore::new("https://store.test", fetch.clone());
        let engine_sha = release.manifest.engine.sha256.clone();

        let (manifest, bytes) = block_on(fetch_engine_from_store(
            &store,
            "esp32c6-4mb",
            BUILD_ID,
            &engine_sha,
        ))
        .unwrap()
        .expect("in the store");
        assert_eq!(manifest, release.manifest);
        assert_eq!(bytes, release.engine);
        assert_eq!(
            fetch.calls(),
            vec![
                "https://store.test/firmware/esp32c6-4mb/2026.10.05-3%2B103285d5d05e/ota-manifest.json"
                    .to_string(),
                "https://store.test/firmware/esp32c6-4mb/2026.10.05-3/engine.bin".to_string(),
            ]
        );
    }

    #[test]
    fn a_dev_build_is_not_looked_up() {
        let fetch = FakeFetch::default();
        let store = FirmwareStore::new("https://store.test", fetch.clone());
        for dev in [
            "abc1234+103285d5d05e",
            "abc1234-dirty-101500PT+103285d5d05e",
            "not a build id",
        ] {
            assert_eq!(
                block_on(fetch_engine_from_store(&store, "esp32c6-4mb", dev, "00")),
                Ok(None),
                "{dev}"
            );
        }
        assert!(fetch.calls().is_empty(), "no fetch for a dev build");
    }

    #[test]
    fn a_release_the_store_lacks_is_none() {
        let fetch = FakeFetch::default();
        let store = FirmwareStore::new("https://store.test", fetch.clone());
        assert_eq!(
            block_on(fetch_engine_from_store(
                &store,
                "esp32c6-4mb",
                BUILD_ID,
                "00"
            )),
            Ok(None)
        );
        assert_eq!(fetch.calls().len(), 1);
    }

    #[test]
    fn another_engine_is_a_mismatch_and_its_bytes_are_not_fetched() {
        let release = release_fixture();
        let fetch = release.fetch("https://store.test");
        let store = FirmwareStore::new("https://store.test", fetch.clone());
        let reported = "ab".repeat(32);
        assert_eq!(
            block_on(fetch_engine_from_store(
                &store,
                "esp32c6-4mb",
                BUILD_ID,
                &reported
            )),
            Err(StoreError::EngineMismatch {
                expected: reported.clone(),
                actual: release.manifest.engine.sha256.clone(),
            })
        );
        assert_eq!(fetch.calls().len(), 1, "only the manifest");
    }

    #[test]
    fn a_build_id_with_another_commit_is_refused() {
        let release = release_fixture();
        let fetch = release.fetch("https://store.test");
        // The store (wrongly) answers the manifest under another build id.
        fetch.put(
            "https://store.test/firmware/esp32c6-4mb/2026.10.05-3%2Baaaaaaaaaaaa/ota-manifest.json",
            release.manifest.to_json_bytes(),
        );
        let store = FirmwareStore::new("https://store.test", fetch);
        assert!(matches!(
            block_on(fetch_engine_from_store(
                &store,
                "esp32c6-4mb",
                "2026.10.05-3+aaaaaaaaaaaa",
                &release.manifest.engine.sha256
            )),
            Err(StoreError::WrongManifest(_))
        ));
    }
}
