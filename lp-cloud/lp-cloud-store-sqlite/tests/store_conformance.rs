//! The conformance suite, run against every adapter there is.
//!
//! Four modules below, four instantiations of the same battery: the
//! in-memory `MetaStore` and `BlobStore` from `lp-cloud-store-mem`, and the
//! SQLite and filesystem stores from this crate. The checks themselves live
//! in [`conformance`] and are written once.
//!
//! If a check passes for one adapter and fails for another, one of them is
//! wrong — and since everything above the ports is tested against the
//! in-memory pair, "wrong" usually means production has been quietly
//! disagreeing with the test suite.
//!
//! The persistent stores get a `tempfile::TempDir` each: a database and a
//! blob directory that exist for exactly one check and are deleted with it.

mod conformance;

/// The in-memory `MetaStore` (`lp-cloud-store-mem`).
mod mem_meta_store {
    use crate::conformance::meta_store_suite::meta_store_conformance_tests;
    use lp_cloud_domain::MetaStore;
    use lp_cloud_store_mem::MemMetaStore;

    fn with_store(check: impl FnOnce(&mut dyn MetaStore)) {
        let mut store = MemMetaStore::new();
        check(&mut store);
    }

    meta_store_conformance_tests!(with_store);
}

/// The SQLite `MetaStore`, on a real file in a temporary directory.
mod sqlite_meta_store {
    use crate::conformance::meta_store_suite::meta_store_conformance_tests;
    use lp_cloud_domain::MetaStore;
    use lp_cloud_store_sqlite::SqliteMetaStore;

    fn with_store(check: impl FnOnce(&mut dyn MetaStore)) {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let mut store = SqliteMetaStore::open(directory.path().join("cloud.sqlite3"));
        check(&mut store);
    }

    meta_store_conformance_tests!(with_store);
}

/// The in-memory `BlobStore` (`lp-cloud-store-mem`).
mod mem_blob_store {
    use crate::conformance::blob_store_suite::blob_store_conformance_tests;
    use lp_cloud_domain::BlobStore;
    use lp_cloud_store_mem::MemBlobStore;

    fn with_store(check: impl FnOnce(&mut dyn BlobStore)) {
        let mut store = MemBlobStore::new();
        check(&mut store);
    }

    blob_store_conformance_tests!(with_store);
}

/// The filesystem `BlobStore`, on a real directory.
mod fs_blob_store {
    use crate::conformance::blob_store_suite::blob_store_conformance_tests;
    use lp_cloud_domain::BlobStore;
    use lp_cloud_store_sqlite::FsBlobStore;

    fn with_store(check: impl FnOnce(&mut dyn BlobStore)) {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let mut store = FsBlobStore::open(directory.path());
        check(&mut store);
    }

    blob_store_conformance_tests!(with_store);
}
