//! Moving a version's content between the local store and the service.

use alloc::vec::Vec;

use lpc_history::ContentHash;
use lpfs::{LpFs, LpFsMemory};

use crate::cloud_port::CloudPort;
use crate::local_project::LocalProject;
use crate::sync::service_calls::have_blobs;
use crate::sync_error::SyncError;

/// Upload everything the service is missing for one version, and nothing
/// else.
///
/// `HaveBlobs` first, always: the point of the pre-flight is that a version
/// sharing most of its files with one the service already has costs one
/// round trip and a couple of uploads, not a re-upload of the package. The
/// version's own tree hash rides along in the same question, because the
/// service checks for it on push like any other blob.
///
/// Returns how many objects were uploaded.
pub(crate) async fn upload_version<P: CloudPort + ?Sized>(
    port: &P,
    project: &LocalProject<'_>,
    version: ContentHash,
    also: &[ContentHash],
) -> Result<usize, SyncError> {
    let snapshots = project.snapshots();
    let manifest = snapshots.get_tree(&version)?;

    let mut candidates: Vec<ContentHash> = manifest.entries().iter().map(|e| e.hash).collect();
    candidates.push(version);
    for hash in also {
        if !candidates.contains(hash) {
            candidates.push(*hash);
        }
    }

    let missing = have_blobs(port, candidates).await?;
    let blobs = project.blobs();
    let mut uploaded = 0;
    for hash in missing {
        if hash == version {
            port.put_tree(&manifest).await?;
        } else {
            let bytes = blobs.get(&hash)?;
            let stored = port.put_blob(&bytes).await?;
            if stored != hash {
                return Err(SyncError::ContentMismatch {
                    expected: hash,
                    actual: stored,
                });
            }
        }
        uploaded += 1;
    }
    Ok(uploaded)
}

/// Fetch a version's content and bank it in the local snapshot store.
///
/// Nothing is written to the working copy — banking and adopting are
/// separate acts, and only the second one can lose anything.
///
/// The fetched files are assembled in a scratch filesystem and handed to
/// [`SnapshotStore::put_package`](lpc_history::SnapshotStore::put_package),
/// so the local store is written through the same path a local save takes
/// and the version's hash is **recomputed from the bytes** rather than taken
/// from the service. A service that hands back content for the wrong hash
/// gets a [`SyncError::ContentMismatch`], not a corrupted store.
///
/// Returns whether anything was fetched (`false` = already banked).
pub(crate) async fn fetch_version<P: CloudPort + ?Sized>(
    port: &P,
    project: &LocalProject<'_>,
    version: ContentHash,
) -> Result<bool, SyncError> {
    let snapshots = project.snapshots();
    if snapshots.has(&version)? {
        return Ok(false);
    }

    let manifest = port.get_tree(version).await?;
    if manifest.package_hash() != version {
        return Err(SyncError::ContentMismatch {
            expected: version,
            actual: manifest.package_hash(),
        });
    }

    let blobs = project.blobs();
    let scratch = LpFsMemory::new();
    for entry in manifest.entries() {
        let bytes = if blobs.has(&entry.hash)? {
            blobs.get(&entry.hash)?
        } else {
            let fetched = port.get_blob(entry.hash).await?;
            let actual = ContentHash::of(&fetched);
            if actual != entry.hash {
                return Err(SyncError::ContentMismatch {
                    expected: entry.hash,
                    actual,
                });
            }
            fetched
        };
        scratch.write_file(&entry.path, &bytes)?;
    }

    let (banked, _) = snapshots.put_package(&scratch)?;
    if banked != version {
        return Err(SyncError::ContentMismatch {
            expected: version,
            actual: banked,
        });
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on::block_on;
    use crate::test_support::TestWorld;

    #[test]
    fn upload_then_fetch_round_trips_a_version() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        dome.write("/shader.glsl", b"void main() {}");
        let v1 = local.save(2.0).unwrap();

        let uploaded = block_on(upload_version(&yona, &local, v1, &[])).unwrap();
        // two files plus the tree
        assert_eq!(uploaded, 3);
        assert_eq!(world.server().blob_count(), 2);
        assert_eq!(world.server().tree_count(), 1);

        // a second version sharing a file uploads only what is new
        dome.write("/shader.glsl", b"void main() { }");
        let v2 = local.save(3.0).unwrap();
        assert_eq!(block_on(upload_version(&yona, &local, v2, &[])).unwrap(), 2);

        // and a fresh copy can pull both back out
        let copy = world.project(2);
        let copy_local = copy.local();
        assert!(block_on(fetch_version(&yona, &copy_local, v1)).unwrap());
        copy_local.checkout(v1).unwrap();
        assert_eq!(copy.read("/shader.glsl"), Some(b"void main() {}".to_vec()));
    }

    #[test]
    fn fetching_an_already_banked_version_does_nothing() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        let v1 = local.save(2.0).unwrap();
        block_on(upload_version(&yona, &local, v1, &[])).unwrap();

        assert!(!block_on(fetch_version(&yona, &local, v1)).unwrap());
    }

    /// Offline is a transport failure, not a corrupt store: nothing is
    /// half-banked.
    #[test]
    fn an_offline_fetch_banks_nothing() {
        let world = TestWorld::new();
        let yona = world.user("yona");
        let dome = world.project(1);
        let local = dome.init();
        dome.write("/project.json", b"{}");
        let v1 = local.save(2.0).unwrap();
        block_on(upload_version(&yona, &local, v1, &[])).unwrap();

        let copy = world.project(2);
        let copy_local = copy.local();
        let viewer = world.anonymous();
        viewer.go_offline();
        assert!(matches!(
            block_on(fetch_version(&viewer, &copy_local, v1)),
            Err(SyncError::Transport(_))
        ));
        assert!(!copy_local.snapshots().has(&v1).unwrap());
    }
}
