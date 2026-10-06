//! The device store and a core on trial (OTA plan, doors #14): since
//! over-the-air updates an older core can read `/.lp/access.json` after a
//! rollback, so no boot rewrites it on its own while its core is an
//! unconfirmed trial. Reading an older shape converts it in memory and
//! writes nothing; the gate a migration must ask says no on a trial and yes
//! on a confirmed boot.

use lpa_server::access_store::{
    BootStanding, device_store_at_boot, may_migrate_device_store, read_device_store,
};
use lpc_access::OpenTo;
use lpc_model::AsLpPath;
use lpfs::{LpFs, LpFsMemory};

/// A version-2 store (its `open` a bool), as an older engine wrote it.
const V2_STORE: &str = r#"{"version":2,"secrets":[],"bleEnabled":true,"open":true}"#;

#[test]
fn reading_an_older_store_writes_nothing() {
    let fs = LpFsMemory::new();
    let path = "/.lp/access.json".as_path();
    fs.write_file(path, V2_STORE.as_bytes()).unwrap();
    let store = read_device_store(&fs);
    assert_eq!(store.open, OpenTo::Play, "a v2 `open: true` keeps play");
    let at_boot = device_store_at_boot(&fs, lpc_wire::FsBootState::Mounted);
    assert_eq!(at_boot.open, OpenTo::Play);
    assert_eq!(
        fs.read_file(path).unwrap(),
        V2_STORE.as_bytes(),
        "the file is still the v2 bytes an older core wrote"
    );
}

#[test]
fn only_a_confirmed_boot_may_migrate_the_store() {
    assert!(!may_migrate_device_store(BootStanding::UnconfirmedTrial));
    assert!(may_migrate_device_store(BootStanding::Confirmed));
}
