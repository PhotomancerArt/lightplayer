//! Building [`UiHomeView`] from hydrated library inputs, the registry,
//! and embedded examples.
//!
//! The device section is the D27 roster: ONE last-seen-sorted list where
//! live and remembered devices mix (live first naturally). Every card's
//! health derives through the M2 evidence function
//! ([`derive_roster_card_state`]); the D24 collapse is gone — a connected
//! device holding a known project keeps its device card AND the project
//! card carries the live chip (D28: one fact, two views).

use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::DeviceState;
use lpc_history::EventKind;
use lpfs::LpFs;

use crate::UiIssue;
use crate::app::library::{LibraryStore, PackageMeta, PackageProvenance};
use crate::app::places::{DeviceContent, DeviceRegistry, DeviceSyncState, RegisteredDevice};
use crate::app::roster::{
    ConnectEvidence, RosterCardState, RosterEvidence, derive_roster_card_state,
};

use super::embedded_example::embedded_examples;
use super::ui_device_card::{UiDeviceCard, UiDeviceProjectChip};
use super::ui_example_card::UiExampleCard;
use super::ui_home_view::UiHomeView;
use super::ui_package_card::{UiCardConnection, UiPackageCard};

/// The gallery's hydrated library data: built asynchronously from a host
/// catalog snapshot (`StudioController::refresh_library`) and cached —
/// `view()` never reads a live store.
#[derive(Debug, Clone, Default)]
pub struct HomeInputs {
    pub projects: Vec<UiPackageCard>,
    pub devices: Vec<UiDeviceCard>,
    /// Listing failed — the gallery surfaces this instead of an empty
    /// library.
    pub issue: Option<UiIssue>,
}

/// Everything the single live session contributes to the roster — the
/// evidence feeding [`derive_roster_card_state`] for the live card. M4's
/// runtime pool replaces this with per-runtime evidence without changing
/// the derivation. Absence of every field is honest evidence of absence
/// (no live card).
#[derive(Clone, Debug, Default)]
pub struct HomeDeviceEvidence {
    /// Connect-as-pull result, once the pull landed.
    pub sync: Option<DeviceSyncState>,
    /// The hardware link's observable state, when a session exists.
    pub link: Option<DeviceState>,
    /// What the connect flow is doing right now.
    pub connect: ConnectEvidence,
    /// Transport label from the live connector class ("USB" for serial);
    /// `None` while the provider is still resolving.
    pub transport: Option<String>,
    /// The device copy's version number on its project line, looked up at
    /// absorb time via `ProjectHistory::version_number`.
    pub observed_version: Option<usize>,
    /// The local head's version number, for the "Push vN" affordance.
    pub head_version: Option<usize>,
}

/// Hydrate [`HomeInputs`] from a library snapshot fs. `open_elsewhere`
/// marks the projects other tabs hold open (their cards get the badge
/// treatment and refuse structural actions kindly).
pub fn hydrate_home_inputs(fs: Rc<RefCell<dyn LpFs>>, open_elsewhere: &[String]) -> HomeInputs {
    let store = LibraryStore::read_only(fs);

    let registered = DeviceRegistry::new(store.fs_handle())
        .list()
        .unwrap_or_else(|error| {
            log::warn!("home: device registry unreadable: {error}");
            Vec::new()
        });

    let mut issue = None;
    let projects: Vec<UiPackageCard> = match store.list() {
        Ok(summaries) => summaries
            .into_iter()
            .filter_map(|summary| {
                package_card(&store, &registered, summary)
                    .map_err(|error| log::warn!("home: skipping package card: {error}"))
                    .ok()
            })
            .map(|mut card| {
                card.open_elsewhere = open_elsewhere.iter().any(|uid| *uid == card.uid);
                card
            })
            .collect(),
        Err(error) => {
            issue = Some(UiIssue::new(format!(
                "Your projects could not be listed: {error}"
            )));
            Vec::new()
        }
    };

    let devices = registered
        .iter()
        .map(|device| device_card(device, &projects))
        .collect();

    HomeInputs {
        projects,
        devices,
        issue,
    }
}

/// Assemble the gallery view model from cached inputs. `inputs` is `None`
/// when no local store mounted (the gallery still shows examples and the
/// connect card). `live` is the single live session's evidence; the D28
/// pairing happens here: a live device holding a known project keeps its
/// device card AND that project's card gets the [`UiCardConnection`] chip.
pub fn build_home_view(
    inputs: Option<&HomeInputs>,
    opening: Option<String>,
    issue: Option<UiIssue>,
    live: &HomeDeviceEvidence,
) -> UiHomeView {
    let examples = dedupe_by_key(
        embedded_examples()
            .iter()
            .map(|example| UiExampleCard {
                id: example.id.to_string(),
                name: example.name.to_string(),
                kind: example.kind.to_string(),
            })
            .collect(),
        |card| card.id.clone(),
        "example",
    );

    let Some(inputs) = inputs else {
        let (_, devices) = assemble_roster(&[], live);
        return UiHomeView {
            devices: dedupe_by_key(devices, |card| card.render_key().to_string(), "device"),
            projects: Vec::new(),
            examples,
            library_available: false,
            opening,
            issue,
        };
    };

    let mut projects = inputs.projects.clone();
    let (live_connection, devices) = assemble_roster(&inputs.devices, live);
    if let Some((project_uid, connection)) = live_connection
        && let Some(card) = projects.iter_mut().find(|card| card.uid == project_uid)
    {
        card.connected_device = Some(connection);
    }

    UiHomeView {
        devices: dedupe_by_key(devices, |card| card.render_key().to_string(), "device"),
        projects: dedupe_by_key(projects, |card| card.uid.clone(), "project"),
        examples,
        library_available: true,
        opening,
        issue: issue.or_else(|| inputs.issue.clone()),
    }
}

/// Drop cards whose render key repeats (keeping the first), warning loudly.
/// Keyed lists with duplicate keys PANIC the renderer and kill the whole
/// app (2026-07-15 home-gallery crash) — a corrupt registry or store must
/// degrade to a missing card, never to a dead UI.
fn dedupe_by_key<T>(cards: Vec<T>, key: impl Fn(&T) -> String, what: &'static str) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    cards
        .into_iter()
        .filter(|card| {
            let card_key = key(card);
            let fresh = seen.insert(card_key.clone());
            if !fresh {
                log::warn!("home: dropping {what} card with duplicate key {card_key:?}");
            }
            fresh
        })
        .collect()
}

/// The D27 roster shape: the live card first, then remembered cards
/// sorted by last seen (newest first). The live device stops being
/// "remembered offline" while it's here; its project pairing (when the
/// contents are a known/adopted library project) returns alongside so the
/// project card can carry the D28 chip.
///
/// Returns `(live project connection, device cards)`.
fn assemble_roster(
    registry_cards: &[UiDeviceCard],
    live: &HomeDeviceEvidence,
) -> (Option<(String, UiCardConnection)>, Vec<UiDeviceCard>) {
    let live_card = live_device_card(live);

    let mut devices: Vec<UiDeviceCard> = registry_cards
        .iter()
        .filter(|card| match (&card.uid, &live_card) {
            // the live device is not "remembered offline" while it's here
            (Some(uid), Some(live_card)) => live_card.uid.as_deref() != Some(uid.as_str()),
            _ => true,
        })
        .cloned()
        .collect();
    // last-seen sort (stable: hydration order breaks ties); live leads
    devices.sort_by(|a, b| {
        last_seen_sort_key(b)
            .partial_cmp(&last_seen_sort_key(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let connection = live_card.as_ref().and_then(|card| {
        let chip = card.project.as_ref()?;
        let relation = match live.sync.as_ref().map(|sync| &sync.content) {
            Some(DeviceContent::Known { relation, .. }) => *relation,
            Some(DeviceContent::Adopted { .. }) => lpc_history::SyncRelation::AtHead,
            _ => return None,
        };
        Some((
            chip.uid.clone(),
            UiCardConnection {
                device_name: card.name.clone(),
                relation,
            },
        ))
    });
    if let Some(card) = live_card {
        devices.insert(0, card);
    }
    (connection, devices)
}

/// The live session's card, derived from evidence. `None` when there is
/// no live evidence at all, and also when the evidence derives *Offline*
/// (a `Gone` link or stale sync) — the registry card is the
/// better-informed offline view (it knows the last sighting).
fn live_device_card(live: &HomeDeviceEvidence) -> Option<UiDeviceCard> {
    if live.sync.is_none() && live.link.is_none() && live.connect == ConnectEvidence::Idle {
        return None;
    }
    let state = derive_roster_card_state(&RosterEvidence {
        link: live.link.as_ref(),
        content: live.sync.as_ref().map(|sync| &sync.content),
        observed_version: live.observed_version,
        head_version: live.head_version,
        registry: None,
        connect: live.connect.clone(),
    });
    if matches!(state, RosterCardState::Offline { .. }) {
        return None;
    }
    let identity = live.sync.as_ref().and_then(|sync| sync.identity.as_ref());
    let project = live.sync.as_ref().and_then(|sync| match &sync.content {
        DeviceContent::Known {
            project_uid, slug, ..
        }
        | DeviceContent::Adopted {
            project_uid, slug, ..
        } => Some(UiDeviceProjectChip {
            uid: project_uid.clone(),
            name: slug.clone(),
        }),
        _ => None,
    });
    // hello firmware provenance: Technical evidence for the card's
    // rich-object detail (Ready links only — a pre-hello link has none)
    let fw = match &live.link {
        Some(DeviceState::Ready { hello }) => Some(hello.fw.clone()),
        _ => None,
    };
    Some(UiDeviceCard {
        uid: identity.map(|identity| identity.uid.clone()),
        name: identity
            .map(|identity| identity.name.clone())
            .unwrap_or_else(|| "Connected device".to_string()),
        transport: live.transport.clone().unwrap_or_default(),
        state,
        project,
        fw,
    })
}

/// Last-seen ordering key: live cards lead, sighted cards follow newest
/// first, never-sighted cards trail.
fn last_seen_sort_key(card: &UiDeviceCard) -> f64 {
    match &card.state {
        RosterCardState::Offline { last_seen_at } => last_seen_at.unwrap_or(f64::NEG_INFINITY),
        _ => f64::INFINITY,
    }
}

fn package_card(
    store: &LibraryStore,
    registered: &[RegisteredDevice],
    summary: crate::app::library::PackageSummary,
) -> Result<UiPackageCard, crate::app::library::LibraryError> {
    let handle = store.open(summary.uid)?;
    let meta = crate::app::library::package_meta::read_meta(&*handle.package_fs.borrow())?;

    let last_saved_at = handle
        .history
        .events()
        .iter()
        .rev()
        .find_map(|event| match event.kind {
            EventKind::Saved { .. } => Some(event.at),
            _ => None,
        })
        .or(meta.as_ref().map(|meta| meta.created_at));

    let uid = summary.uid.to_string();
    let on_device = handle.history.head().and_then(|head| {
        registered.iter().find_map(|device| {
            let association = device.association.as_ref()?;
            (association.project.to_string() == uid && association.version == head)
                .then(|| device.name.clone())
        })
    });

    Ok(UiPackageCard {
        uid,
        kind: summary.kind,
        slug: summary.slug,
        last_saved_at,
        provenance: meta.and_then(|meta| provenance_line(store, &meta)),
        on_device,
        open_elsewhere: false,  // stamped by the hydration pass
        connected_device: None, // stamped by the D28 pairing at view build
    })
}

/// The card's human provenance line; `None` for created-from-scratch.
fn provenance_line(store: &LibraryStore, meta: &PackageMeta) -> Option<String> {
    match &meta.provenance {
        PackageProvenance::Created => None,
        PackageProvenance::SeededFrom { source } => {
            let name = super::embedded_example::embedded_example(source)
                .map(|example| example.name.to_string())
                .unwrap_or_else(|| source.clone());
            Some(format!("Remixed from {name}"))
        }
        PackageProvenance::ImportedZip { .. } => Some("Imported from zip".to_string()),
        PackageProvenance::PulledFromDevice { device_name, .. } => {
            Some(format!("Pulled from {device_name}"))
        }
        PackageProvenance::ForkedFrom { parent_project, .. } => {
            let parent = parent_project
                .parse()
                .ok()
                .and_then(|uid| {
                    store
                        .list()
                        .ok()?
                        .into_iter()
                        .find_map(|summary| (summary.uid == uid).then_some(summary.slug))
                })
                .unwrap_or_else(|| parent_project.clone());
            Some(format!("Forked from {parent}"))
        }
    }
}

/// A remembered device's card, derived through the same evidence function
/// as the live card (registry-only evidence = offline).
fn device_card(device: &RegisteredDevice, projects: &[UiPackageCard]) -> UiDeviceCard {
    // the last-known chip prefers the project's current name; a deleted
    // project's association falls back to its uid
    let project = device.association.as_ref().map(|association| {
        let uid = association.project.to_string();
        let name = projects
            .iter()
            .find_map(|card| (card.uid == uid).then(|| card.slug.clone()))
            .unwrap_or_else(|| uid.clone());
        UiDeviceProjectChip { uid, name }
    });
    let state = derive_roster_card_state(&RosterEvidence {
        link: None,
        content: None,
        observed_version: None,
        head_version: None,
        registry: Some(device),
        connect: ConnectEvidence::Idle,
    });
    UiDeviceCard {
        uid: Some(device.uid.clone()),
        name: device.name.clone(),
        // recorded at last sight from the live session's connector class
        transport: device.transport.clone(),
        state,
        project,
        // remembered only: no live hello, no firmware provenance
        fw: None,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lpc_history::{DeviceAssociation, PrefixedUid, SyncRelation, UidPrefix};
    use lpc_wire::{FwProvenance, ServerHello, WIRE_PROTO_VERSION};
    use lpfs::LpFsMemory;

    use crate::app::places::{DeviceIdentity, DeviceSyncState};

    use super::*;

    fn store() -> LibraryStore {
        let counter = Rc::new(RefCell::new(0u8));
        LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || {
                *counter.borrow_mut() += 1;
                [*counter.borrow(); 16]
            }),
            Rc::new(|| "2026-07-09-1421".to_string()),
        )
    }

    fn view_of(store: &LibraryStore) -> UiHomeView {
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);
        build_home_view(Some(&inputs), None, None, &HomeDeviceEvidence::default())
    }

    fn ready_link() -> DeviceState {
        DeviceState::Ready {
            hello: ServerHello {
                proto: WIRE_PROTO_VERSION,
                fw: FwProvenance {
                    package: "fw-esp32".to_string(),
                    commit: "abc123456789".to_string(),
                    dirty: false,
                    profile: "release-esp32".to_string(),
                },
                device_uid: Some("dev_aaaaaaaaaaaaaaaa".to_string()),
            },
        }
    }

    /// Evidence for a live, Ready device carrying `sync`.
    fn live(sync: DeviceSyncState) -> HomeDeviceEvidence {
        HomeDeviceEvidence {
            sync: Some(sync),
            link: Some(ready_link()),
            transport: Some("USB".to_string()),
            ..HomeDeviceEvidence::default()
        }
    }

    #[test]
    fn no_library_still_lists_examples() {
        let view = build_home_view(None, None, None, &HomeDeviceEvidence::default());
        assert!(!view.library_available);
        assert!(view.projects.is_empty());
        assert_eq!(view.examples.len(), embedded_examples().len());
        assert!(
            view.examples
                .iter()
                .any(|example| example.name == "Fyeah Sign")
        );
    }

    #[test]
    fn open_elsewhere_uids_stamp_their_cards() {
        let store = store();
        let held = store.create("Held", 1.0).unwrap();
        let free = store.create("Free", 2.0).unwrap();

        let inputs = hydrate_home_inputs(store.fs_handle(), &[held.uid.to_string()]);
        let by_uid = |uid: &str| inputs.projects.iter().find(|card| card.uid == uid).unwrap();
        assert!(by_uid(&held.uid.to_string()).open_elsewhere);
        assert!(!by_uid(&free.uid.to_string()).open_elsewhere);
    }

    #[test]
    fn package_cards_carry_meta_and_provenance() {
        let store = store();
        store.create("Scratch", 10.0).unwrap();
        store
            .install_package(
                "Basic",
                &[(
                    "project.json".to_string(),
                    br#"{"kind":"Project","name":"Basic"}"#.to_vec(),
                )],
                PackageProvenance::SeededFrom {
                    source: "examples/fyeah-sign".to_string(),
                },
                20.0,
            )
            .unwrap();

        let view = view_of(&store);
        assert!(view.library_available);
        assert_eq!(view.projects.len(), 2);

        let basic = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-basic")
            .unwrap();
        assert_eq!(basic.provenance.as_deref(), Some("Remixed from Fyeah Sign"));
        assert_eq!(basic.last_saved_at, Some(20.0));

        let scratch = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-scratch")
            .unwrap();
        assert_eq!(scratch.provenance, None);
        assert_eq!(scratch.kind, "Project");
    }

    #[test]
    fn fork_provenance_names_the_parent_slug() {
        let store = store();
        let original = store.create("Original", 1.0).unwrap();
        let copy_summary = store.duplicate(original.uid, 2.0).unwrap();
        // re-stamped label, uniqued against the (same-stamp) original
        assert_eq!(copy_summary.slug, "2026-07-09-1421-original-2");

        let view = view_of(&store);
        let copy = view
            .projects
            .iter()
            .find(|card| card.uid == copy_summary.uid.to_string())
            .unwrap();
        assert_eq!(
            copy.provenance.as_deref(),
            Some("Forked from 2026-07-09-1421-original")
        );
    }

    #[test]
    fn same_named_registered_devices_keep_unique_render_keys() {
        // Erasing and re-provisioning a board registers a NEW dev_… uid
        // under the SAME name. Duplicate render keys panic the keyed diff
        // and kill the whole app (2026-07-15 crash) — uid-based keys must
        // stay unique, and both cards must survive.
        let store = store();
        let registry = DeviceRegistry::new(store.fs_handle());
        for seed in [7u8, 8u8] {
            registry
                .upsert(crate::app::places::RegisteredDevice {
                    uid: PrefixedUid::mint(UidPrefix::Device, &[seed; 16]).to_string(),
                    name: "lp-c6".to_string(),
                    transport: "USB".to_string(),
                    last_seen_at: 5.0,
                    association: None,
                })
                .unwrap();
        }

        let view = view_of(&store);
        assert_eq!(view.devices.len(), 2);
        let keys: std::collections::HashSet<_> = view
            .devices
            .iter()
            .map(|card| card.render_key().to_string())
            .collect();
        assert_eq!(keys.len(), 2, "render keys must be unique: {keys:?}");
    }

    #[test]
    fn duplicate_card_keys_degrade_to_a_dropped_card_not_a_dead_ui() {
        // A corrupt registry/store that repeats an identity must lose the
        // duplicate card (with a warning), never poison the keyed list.
        let offline = RosterCardState::Offline {
            last_seen_at: Some(5.0),
        };
        let cards = vec![
            UiDeviceCard {
                uid: Some("dev_a".to_string()),
                name: "one".to_string(),
                transport: "USB".to_string(),
                state: offline.clone(),
                project: None,
                fw: None,
            },
            UiDeviceCard {
                uid: Some("dev_a".to_string()),
                name: "two".to_string(),
                transport: "USB".to_string(),
                state: offline,
                project: None,
                fw: None,
            },
        ];
        let deduped = dedupe_by_key(cards, |card| card.render_key().to_string(), "device");
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].name, "one");
    }

    #[test]
    fn registered_devices_become_offline_cards_with_project_chips() {
        let store = store();
        let summary = store.create("Porch", 1.0).unwrap();
        let head = store.open(summary.uid).unwrap().history.head().unwrap();

        let registry = DeviceRegistry::new(store.fs_handle());
        let device_uid = PrefixedUid::mint(UidPrefix::Device, &[9u8; 16]);
        registry
            .upsert(crate::app::places::RegisteredDevice {
                uid: device_uid.to_string(),
                name: "Luna's porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 5.0,
                association: Some(DeviceAssociation {
                    device: device_uid,
                    project: summary.uid,
                    version: head,
                    at: 5.0,
                }),
            })
            .unwrap();

        let view = view_of(&store);
        assert_eq!(view.devices.len(), 1);
        assert_eq!(view.devices[0].name, "Luna's porch sign");
        assert_eq!(
            view.devices[0].state,
            RosterCardState::Offline {
                last_seen_at: Some(5.0),
            }
        );
        let chip = view.devices[0].project.as_ref().expect("last-known chip");
        assert_eq!(chip.name, "2026-07-09-1421-porch");
        assert_eq!(chip.uid, summary.uid.to_string());

        let porch = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-porch")
            .unwrap();
        assert_eq!(porch.on_device.as_deref(), Some("Luna's porch sign"));
    }

    #[test]
    fn d28_connected_device_keeps_its_card_and_the_project_gets_the_chip() {
        let store = store();
        let summary = store.create("Porch", 1.0).unwrap();
        // the device is also remembered in the registry (it must not show
        // twice: not as remembered-offline while it is live)
        let registry = DeviceRegistry::new(store.fs_handle());
        registry
            .upsert(RegisteredDevice {
                uid: "dev_aaaaaaaaaaaaaaaa".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 5.0,
                association: None,
            })
            .unwrap();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let mut evidence = live(DeviceSyncState {
            identity: Some(DeviceIdentity {
                uid: "dev_aaaaaaaaaaaaaaaa".to_string(),
                name: "Porch sign".to_string(),
            }),
            content: DeviceContent::Known {
                project_uid: summary.uid.to_string(),
                slug: summary.slug.clone(),
                observed: lpc_history::ContentHash::of(b"v"),
                relation: SyncRelation::Behind,
            },
        });
        evidence.observed_version = Some(3);
        evidence.head_version = Some(5);
        let view = build_home_view(Some(&inputs), None, None, &evidence);

        // one device card (live, not remembered-offline), with the state
        // derived through the roster evidence function
        assert_eq!(view.devices.len(), 1, "one card, not two (D28)");
        let card = &view.devices[0];
        assert_eq!(card.name, "Porch sign");
        assert_eq!(
            card.state,
            RosterCardState::RunningBehind {
                observed_version: Some(3),
                head_version: Some(5),
            }
        );
        let chip = card.project.as_ref().expect("live project chip");
        assert_eq!(chip.name, summary.slug);

        // AND the project card carries the live connection chip
        let project = view
            .projects
            .iter()
            .find(|card| card.uid == summary.uid.to_string())
            .unwrap();
        let connection = project.connected_device.as_ref().expect("indication set");
        assert_eq!(connection.device_name, "Porch sign");
        assert_eq!(connection.relation, SyncRelation::Behind);
    }

    #[test]
    fn roster_sorts_by_last_seen_with_the_live_card_first() {
        let store = store();
        let registry = DeviceRegistry::new(store.fs_handle());
        for (seed, name, seen) in [
            (1u8, "old", 10.0),
            (2u8, "newest", 300.0),
            (3u8, "middle", 200.0),
        ] {
            registry
                .upsert(RegisteredDevice {
                    uid: PrefixedUid::mint(UidPrefix::Device, &[seed; 16]).to_string(),
                    name: name.to_string(),
                    transport: "USB".to_string(),
                    last_seen_at: seen,
                    association: None,
                })
                .unwrap();
        }
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let evidence = live(DeviceSyncState {
            identity: Some(DeviceIdentity {
                uid: "dev_bbbbbbbbbbbbbbbb".to_string(),
                name: "Live one".to_string(),
            }),
            content: DeviceContent::Empty,
        });
        let view = build_home_view(Some(&inputs), None, None, &evidence);

        let names: Vec<&str> = view.devices.iter().map(|card| card.name.as_str()).collect();
        assert_eq!(names, vec!["Live one", "newest", "middle", "old"]);
    }

    #[test]
    fn blank_and_unknown_devices_keep_their_own_cards() {
        let store = store();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let blank = live(DeviceSyncState {
            identity: Some(DeviceIdentity {
                uid: "dev_bbbbbbbbbbbbbbbb".to_string(),
                name: "Fresh board".to_string(),
            }),
            content: DeviceContent::Empty,
        });
        let view = build_home_view(Some(&inputs), None, None, &blank);
        assert_eq!(view.devices.len(), 1);
        assert_eq!(view.devices[0].state, RosterCardState::ConnectedEmpty);

        let anonymous = live(DeviceSyncState {
            identity: None,
            content: DeviceContent::PendingIdentity {
                observed: lpc_history::ContentHash::of(b"x"),
            },
        });
        let view = build_home_view(Some(&inputs), None, None, &anonymous);
        assert_eq!(view.devices.len(), 1);
        assert_eq!(view.devices[0].state, RosterCardState::NeedsAName);
        assert_eq!(view.devices[0].name, "Connected device");
    }

    #[test]
    fn a_gone_link_yields_no_live_card_only_the_registry_view() {
        // cable yanked: the link reads Gone; the remembered card (which
        // knows the last sighting) is the honest view, not a live card
        // that would say "Not seen yet"
        let store = store();
        let registry = DeviceRegistry::new(store.fs_handle());
        registry
            .upsert(RegisteredDevice {
                uid: "dev_aaaaaaaaaaaaaaaa".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 50.0,
                association: None,
            })
            .unwrap();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let evidence = HomeDeviceEvidence {
            link: Some(DeviceState::Gone),
            ..HomeDeviceEvidence::default()
        };
        let view = build_home_view(Some(&inputs), None, None, &evidence);
        assert_eq!(view.devices.len(), 1);
        assert_eq!(
            view.devices[0].state,
            RosterCardState::Offline {
                last_seen_at: Some(50.0),
            }
        );
    }

    #[test]
    fn opening_and_issue_pass_through() {
        let view = build_home_view(
            None,
            Some("prj_x".to_string()),
            Some(UiIssue::new("boom")),
            &HomeDeviceEvidence::default(),
        );
        assert_eq!(view.opening.as_deref(), Some("prj_x"));
        assert_eq!(view.issue.as_ref().unwrap().message, "boom");
        assert_eq!(
            view.render_text_lines(),
            vec![
                "Home: 0 devices, 0 projects, 1 examples".to_string(),
                "  opening prj_x".to_string(),
                "  issue: boom".to_string(),
            ]
        );
    }
}
