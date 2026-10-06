//! Keep the engine a USB install just wrote (plan D19): fetch the package it
//! flashed — its manifest and merged image, at the same URLs the flasher
//! used — slice the engine out ([`InstalledPackage`]) and put it in the
//! [`EngineCache`] as `installed`.
//!
//! The caller runs this AFTER the install's success is reported and only
//! logs its answer: keeping the engine must never fail or delay an install.

use crate::engine_cache::EngineCache;
use crate::firmware_fetch::FirmwareFetch;
use crate::installed_engine::InstalledPackage;

/// What [`keep_installed_engine`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeptEngine {
    /// The engine is in the cache, by this SHA-256, from this build.
    Kept { sha256: String, build_id: String },
    /// The package has no `split` block: no engine to keep.
    NotSplit,
}

/// Fetch the package at `manifest_url` (its merged image beside it), slice
/// its engine out and put it in `cache` as `installed` at `now` (epoch
/// seconds). Every error is a one-line reason for a log.
pub async fn keep_installed_engine<F: FirmwareFetch + ?Sized, C: EngineCache + ?Sized>(
    fetch: &F,
    cache: &C,
    manifest_url: &str,
    now: f64,
) -> Result<KeptEngine, String> {
    let manifest = fetch_required(fetch, manifest_url).await?;
    let Some(package) = InstalledPackage::parse(&manifest)? else {
        return Ok(KeptEngine::NotSplit);
    };
    let dir = manifest_url
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    let image_url = match dir.is_empty() {
        true => package.image_file.clone(),
        false => format!("{dir}/{}", package.image_file),
    };
    let image = fetch_required(fetch, &image_url).await?;
    let (entry, bytes) = package.engine(&image, now)?;
    let sha256 = entry.sha256.clone();
    cache
        .put(entry, bytes)
        .await
        .map_err(|e| format!("the engine cache refused it: {e}"))?;
    Ok(KeptEngine::Kept {
        sha256,
        build_id: package.build_id,
    })
}

async fn fetch_required<F: FirmwareFetch + ?Sized>(
    fetch: &F,
    url: &str,
) -> Result<Vec<u8>, String> {
    match fetch.get(url).await {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => Err(format!("{url} is not there")),
        Err(e) => Err(format!("{url}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_cache_entry::EngineSource;
    use crate::memory_engine_cache::MemoryEngineCache;
    use crate::test_block_on::block_on;
    use crate::test_fetch::FakeFetch;
    use lpc_firmware_release::sha256_hex;

    const MANIFEST_URL: &str = "./firmware/esp32c6-4mb/manifest.json";
    const IMAGE_URL: &str = "./firmware/esp32c6-4mb/fw-esp32c6-merged.bin";

    #[test]
    fn a_split_install_keeps_its_engine() {
        let (fetch, engine) = split_package_fetch();
        let cache = MemoryEngineCache::new();
        let kept = block_on(keep_installed_engine(&fetch, &cache, MANIFEST_URL, 5.0)).unwrap();
        let sha = sha256_hex(&engine);
        assert_eq!(
            kept,
            KeptEngine::Kept {
                sha256: sha.clone(),
                build_id: "2026.10.05-3+103285d5d05e".into()
            }
        );
        assert_eq!(block_on(cache.get(&sha, 6.0)).unwrap(), engine);
        let index = block_on(cache.index());
        let entry = &index.entries[0];
        assert_eq!(entry.source, EngineSource::Installed);
        assert_eq!(entry.target.as_deref(), Some("esp32c6-4mb"));
        assert_eq!(fetch.calls(), [MANIFEST_URL, IMAGE_URL]);
    }

    #[test]
    fn a_package_without_a_split_block_fetches_no_image() {
        let fetch = FakeFetch::default();
        fetch.put(
            MANIFEST_URL,
            br#"{"firmwareId":"esp32s3-8mb","images":[{"path":"x.bin","sizeBytes":1,"sha256":"aa"}]}"#
                .to_vec(),
        );
        let cache = MemoryEngineCache::new();
        let kept = block_on(keep_installed_engine(&fetch, &cache, MANIFEST_URL, 5.0)).unwrap();
        assert_eq!(kept, KeptEngine::NotSplit);
        assert_eq!(fetch.calls(), [MANIFEST_URL]);
        assert!(block_on(cache.index()).entries.is_empty());
    }

    #[test]
    fn a_failed_fetch_is_a_reason_and_keeps_nothing() {
        let (fetch, _) = split_package_fetch();
        fetch.remove(IMAGE_URL);
        let cache = MemoryEngineCache::new();
        let error = block_on(keep_installed_engine(&fetch, &cache, MANIFEST_URL, 5.0)).unwrap_err();
        assert!(error.contains("is not there"), "{error}");
        assert!(block_on(cache.index()).entries.is_empty());

        fetch.go_offline();
        let error = block_on(keep_installed_engine(&fetch, &cache, MANIFEST_URL, 5.0)).unwrap_err();
        assert!(error.contains("unreachable"), "{error}");
    }

    /// A fetch serving a synthetic split package at the bundle's URLs.
    fn split_package_fetch() -> (FakeFetch, Vec<u8>) {
        let mut image = vec![0xffu8; 0x3000];
        let engine: Vec<u8> = (0..0x800u32).map(|i| (i * 13 + 1) as u8).collect();
        image[0x2000..0x2800].copy_from_slice(&engine);
        let manifest = serde_json::json!({
            "schemaVersion": 2, "firmwareId": "esp32c6-4mb",
            "core": { "version": "2026.10.05-3" },
            "images": [{ "path": "fw-esp32c6-merged.bin", "sizeBytes": image.len(),
                         "sha256": sha256_hex(&image) }],
            "split": { "buildId": "2026.10.05-3+103285d5d05e",
                       "engine": { "offset": "0x2000", "sizeBytes": engine.len(),
                                   "sha256": sha256_hex(&engine) } }
        });
        let fetch = FakeFetch::default();
        fetch.put(MANIFEST_URL, serde_json::to_vec(&manifest).unwrap());
        fetch.put(IMAGE_URL, image);
        (fetch, engine)
    }
}
