//! The roadmap's fleet/family walkthroughs as executable specs.
//!
//! These scenarios are the product promises of the studio project-management
//! roadmap (2026-07-06), exercised end-to-end through the public API over
//! in-memory filesystems. All timestamps are hand-supplied — nothing here
//! reads a clock.

use lpc_history::{
    ContentHash, EventKind, EventLog, HistoryEvent, PrefixedUid, ProjectHistory, SnapshotStore,
    SyncRelation, UidPrefix,
};
use lpfs::{LpFs, LpFsMemory, LpPath};

fn write(fs: &LpFsMemory, path: &str, data: &[u8]) {
    fs.write_file(LpPath::new(path), data).unwrap();
}

fn read(fs: &LpFsMemory, path: &str) -> Vec<u8> {
    fs.read_file(LpPath::new(path)).unwrap()
}

fn uid(prefix: UidPrefix, seed: u8) -> PrefixedUid {
    PrefixedUid::mint(prefix, &[seed; 16])
}

/// Save the package's current content: snapshot + record + log.
fn save(
    store: &SnapshotStore<'_>,
    history: &mut ProjectHistory,
    log: &EventLog<'_>,
    package: &LpFsMemory,
    at: f64,
) -> ContentHash {
    let (hash, _) = store.put_package(package).unwrap();
    let event = history.record_save(hash, at);
    log.append(&event).unwrap();
    hash
}

/// Fleet: one project, several boards, every reconnect is a fast-forward.
#[test]
fn fleet_one_project_many_boards() {
    let package = LpFsMemory::new();
    let history_fs = LpFsMemory::new();
    let store = SnapshotStore::new(&history_fs);
    let log = EventLog::new(&history_fs);

    let origin = HistoryEvent {
        at: 1.0,
        kind: EventKind::Created,
    };
    log.append(&origin).unwrap();
    let mut history = ProjectHistory::new(origin).unwrap();

    let dev1 = uid(UidPrefix::Device, 1);
    let dev2 = uid(UidPrefix::Device, 2);

    // save v1..v3, push v3 to dev-1
    write(&package, "/project.json", b"{\"name\":\"x\"}");
    write(&package, "/shader.glsl", b"v1");
    save(&store, &mut history, &log, &package, 2.0);
    write(&package, "/shader.glsl", b"v2");
    save(&store, &mut history, &log, &package, 3.0);
    write(&package, "/shader.glsl", b"v3");
    let v3 = save(&store, &mut history, &log, &package, 4.0);
    let push = history.record_push(v3, dev1, 5.0, None).unwrap();
    log.append(&push).unwrap();

    // keep working: v4..v7, push v7 to dev-2
    write(&package, "/shader.glsl", b"v7");
    let v7 = save(&store, &mut history, &log, &package, 6.0);
    log.append(&history.record_push(v7, dev2, 7.0, None).unwrap())
        .unwrap();

    // reconnect dev-1: carrying v3 -> Behind; push v7; reconnect -> AtHead
    log.append(&history.record_connect(dev1, v3, 8.0)).unwrap();
    assert_eq!(history.classify(v3), SyncRelation::Behind);
    log.append(&history.record_push(v7, dev1, 9.0, None).unwrap())
        .unwrap();
    assert_eq!(history.classify(v7), SyncRelation::AtHead);

    // the whole story replays from the persistent log
    let replayed = ProjectHistory::load(&log).unwrap();
    assert_eq!(replayed, history);
    assert_eq!(replayed.version_number(v3), Some(3));
    assert_eq!(replayed.version_number(v7), Some(4));
}

/// Family: one base, personalized variants (the Luna/Raquel story).
#[test]
fn family_luna_raquel_variants() {
    let package = LpFsMemory::new();
    let history_fs = LpFsMemory::new();
    let store = SnapshotStore::new(&history_fs);

    let project_x = uid(UidPrefix::Project, 10);
    let luna_dev = uid(UidPrefix::Device, 1);
    let raquel_dev = uid(UidPrefix::Device, 2);

    let mut x = ProjectHistory::new(HistoryEvent {
        at: 1.0,
        kind: EventKind::Created,
    })
    .unwrap();

    // X to v3 (green/white for Luna), pushed to Luna's sign — on location
    write(&package, "/project.json", b"{\"name\":\"patterns\"}");
    write(&package, "/params.json", b"{\"palette\":\"green-white\"}");
    let (v3, _) = store.put_package(&package).unwrap();
    x.record_save(v3, 2.0);
    x.record_push(
        v3,
        luna_dev,
        3.0,
        Some(lpc_history::GeoPoint {
            lat: 45.559,
            lon: -122.645,
            label: Some("near Alberta St".into()),
        }),
    )
    .unwrap();

    // X evolves to v7 (bee colors for Raquel), pushed to Raquel's sign
    write(&package, "/params.json", b"{\"palette\":\"yellow-black\"}");
    let (v7, _) = store.put_package(&package).unwrap();
    x.record_save(v7, 4.0);
    x.record_push(v7, raquel_dev, 5.0, None).unwrap();

    // Luna comes back: her device carries v3 -> Behind. The crate reports
    // the relation; choosing fork-vs-update is the UI's job. Luna wants her
    // colors kept, so: fork at v3.
    x.record_connect(luna_dev, v3, 6.0);
    assert_eq!(x.classify(v3), SyncRelation::Behind);

    let mut luna = ProjectHistory::fork_from(&x, project_x, v3, 7.0).unwrap();
    assert_eq!(luna.head(), Some(v3));

    // materialize her version and verify the bytes are her palette
    let luna_package = LpFsMemory::new();
    store.materialize(&v3, &luna_package).unwrap();
    assert_eq!(
        read(&luna_package, "/params.json"),
        b"{\"palette\":\"green-white\"}"
    );

    // tweak Luna's fork and save; her line advances independently of X
    write(
        &luna_package,
        "/params.json",
        b"{\"palette\":\"green-white-mint\"}",
    );
    let (luna_v2, _) = store.put_package(&luna_package).unwrap();
    luna.record_save(luna_v2, 8.0);
    luna.record_push(luna_v2, luna_dev, 9.0, None).unwrap();

    // the device association moved to Luna's project: classify against HER line
    assert_eq!(luna.classify(luna_v2), SyncRelation::AtHead);
    assert_eq!(luna.classify(v3), SyncRelation::Behind);
    // and X is untouched
    assert_eq!(x.head(), Some(v7));
    assert!(!x.contains(luna_v2));
}

/// Divergence: a device carries a version this history never saved.
#[test]
fn diverged_device_copy_is_banked_and_forkable() {
    let history_fs = LpFsMemory::new();
    let store = SnapshotStore::new(&history_fs);
    let project_x = uid(UidPrefix::Project, 10);
    let dev = uid(UidPrefix::Device, 3);

    let mut x = ProjectHistory::new(HistoryEvent {
        at: 1.0,
        kind: EventKind::Created,
    })
    .unwrap();
    let package = LpFsMemory::new();
    write(&package, "/project.json", b"{}");
    let (v1, _) = store.put_package(&package).unwrap();
    x.record_save(v1, 2.0);

    // the device shows up with foreign content (edited from another machine)
    let device_package = LpFsMemory::new();
    write(&device_package, "/project.json", b"{}");
    write(&device_package, "/extra.glsl", b"who wrote this?");
    // connect-as-pull: snapshot first, then record the observation
    let (observed, _) = store.put_package(&device_package).unwrap();
    x.record_connect(dev, observed, 3.0);

    assert_eq!(x.classify(observed), SyncRelation::Diverged);
    // still diverged on reconnect — observations never join the line
    x.record_connect(dev, observed, 4.0);
    assert_eq!(x.classify(observed), SyncRelation::Diverged);

    // but the banked version is a valid fork parent (adopt / keep-both flows)
    let fork = ProjectHistory::fork_from(&x, project_x, observed, 5.0).unwrap();
    assert_eq!(fork.head(), Some(observed));

    let recovered = LpFsMemory::new();
    store.materialize(&observed, &recovered).unwrap();
    assert_eq!(read(&recovered, "/extra.glsl"), b"who wrote this?");
}

/// Recovery: the device is gone, but the push event + snapshot bring it back.
#[test]
fn recovery_from_history_when_device_is_lost() {
    let history_fs = LpFsMemory::new();
    let store = SnapshotStore::new(&history_fs);
    let log = EventLog::new(&history_fs);
    let project_x = uid(UidPrefix::Project, 10);
    let luna_dev = uid(UidPrefix::Device, 1);

    let origin = HistoryEvent {
        at: 1.0,
        kind: EventKind::Created,
    };
    log.append(&origin).unwrap();
    let mut x = ProjectHistory::new(origin).unwrap();

    let package = LpFsMemory::new();
    write(&package, "/params.json", b"{\"palette\":\"green-white\"}");
    let luna_version = save(&store, &mut x, &log, &package, 2.0);
    log.append(&x.record_push(luna_version, luna_dev, 3.0, None).unwrap())
        .unwrap();

    write(&package, "/params.json", b"{\"palette\":\"yellow-black\"}");
    save(&store, &mut x, &log, &package, 4.0);

    // a year later, laptop is new: reload history from the log alone,
    // find what was pushed to Luna's device, fork it, materialize it
    let reloaded = ProjectHistory::load(&log).unwrap();
    let pushed_to_luna = reloaded
        .events()
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::Pushed {
                version, device, ..
            } if *device == luna_dev => Some(*version),
            _ => None,
        })
        .expect("push event survives in history");
    assert_eq!(pushed_to_luna, luna_version);

    let fork = ProjectHistory::fork_from(&reloaded, project_x, pushed_to_luna, 5.0).unwrap();
    assert_eq!(fork.head(), Some(luna_version));

    let restored = LpFsMemory::new();
    store.materialize(&pushed_to_luna, &restored).unwrap();
    assert_eq!(
        read(&restored, "/params.json"),
        b"{\"palette\":\"green-white\"}"
    );
}

/// Revert: re-saving old content dedups in the store and stays honest in history.
#[test]
fn revert_dedups_and_history_stays_honest() {
    let history_fs = LpFsMemory::new();
    let store = SnapshotStore::new(&history_fs);
    let dev = uid(UidPrefix::Device, 1);

    let mut x = ProjectHistory::new(HistoryEvent {
        at: 1.0,
        kind: EventKind::Created,
    })
    .unwrap();

    let package = LpFsMemory::new();
    write(&package, "/shader.glsl", b"v1 content");
    let (v1, _) = store.put_package(&package).unwrap();
    x.record_save(v1, 2.0);
    x.record_push(v1, dev, 3.0, None).unwrap();

    write(&package, "/shader.glsl", b"v2 content");
    let (v2, _) = store.put_package(&package).unwrap();
    x.record_save(v2, 4.0);

    let blobs_before = history_fs
        .list_dir(LpPath::new("/blobs"), true)
        .unwrap()
        .len();

    // revert: write v1 content back and save — same hash, no new blobs
    write(&package, "/shader.glsl", b"v1 content");
    let (reverted, _) = store.put_package(&package).unwrap();
    assert_eq!(reverted, v1);
    x.record_save(reverted, 5.0);

    let blobs_after = history_fs
        .list_dir(LpPath::new("/blobs"), true)
        .unwrap()
        .len();
    assert_eq!(blobs_before, blobs_after);

    // the device that got v1 long ago now matches the head again — honestly
    assert_eq!(x.classify(v1), SyncRelation::AtHead);
    assert_eq!(x.classify(v2), SyncRelation::Behind);
    assert_eq!(x.version_number(v1), Some(1));
}
