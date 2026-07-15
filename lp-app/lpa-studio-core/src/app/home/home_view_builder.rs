//! Building [`UiHomeView`] from hydrated library inputs, the registry,
//! and embedded examples.

use std::cell::RefCell;
use std::rc::Rc;

use lpc_history::EventKind;
use lpfs::LpFs;

use crate::UiIssue;
use crate::app::library::{LibraryStore, PackageMeta, PackageProvenance};
use crate::app::places::{DeviceContent, DeviceRegistry, DeviceSyncState, RegisteredDevice};

use super::embedded_example::embedded_examples;
use super::ui_device_card::{UiDeviceCard, UiDeviceCardState};
use super::ui_example_card::UiExampleCard;
use super::ui_home_view::UiHomeView;
use super::ui_package_card::UiPackageCard;

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
/// connect card). `device_sync` is the LIVE device's connect-as-pull
/// result; D24 unification happens here: a connected device holding a
/// locally-known project becomes an indication on that project's card,
/// not a second card.
/// `live_transport` is the LIVE device's transport label, read from its
/// connector class metadata by the controller ("USB" for serial classes);
/// registry cards carry the label recorded at last sight instead.
pub fn build_home_view(
    inputs: Option<&HomeInputs>,
    opening: Option<String>,
    issue: Option<UiIssue>,
    device_sync: Option<&DeviceSyncState>,
    live_transport: Option<&str>,
) -> UiHomeView {
    let examples = embedded_examples()
        .iter()
        .map(|example| UiExampleCard {
            id: example.id.to_string(),
            name: example.name.to_string(),
            kind: example.kind.to_string(),
        })
        .collect();

    let Some(inputs) = inputs else {
        return UiHomeView {
            devices: unify_devices(&[], device_sync, live_transport).1,
            projects: Vec::new(),
            examples,
            library_available: false,
            opening,
            issue,
        };
    };

    let mut projects = inputs.projects.clone();
    let (unified_onto_project, mut devices) =
        unify_devices(&inputs.devices, device_sync, live_transport);
    if let Some((project_uid, connection)) = unified_onto_project {
        match projects.iter_mut().find(|card| card.uid == project_uid) {
            Some(card) => card.connected_device = Some(connection),
            None => {
                // the project vanished between pull and hydration —
                // degrade to a plain device card rather than losing the
                // device from view
                devices.push(UiDeviceCard {
                    uid: device_sync.and_then(|sync| {
                        sync.identity.as_ref().map(|identity| identity.uid.clone())
                    }),
                    name: connection.device_name,
                    transport: live_transport.unwrap_or_default().to_string(),
                    state: UiDeviceCardState::ConnectedRunning { project: None },
                });
            }
        }
    }

    UiHomeView {
        devices,
        projects,
        examples,
        library_available: true,
        opening,
        issue: issue.or_else(|| inputs.issue.clone()),
    }
}

/// The D24 device-section shape: registry cards minus the live device
/// (it stops being "remembered" while it's here), plus either a live
/// device card OR a unification onto a project card.
///
/// Returns `(project unification, device cards)`.
fn unify_devices(
    registry_cards: &[UiDeviceCard],
    device_sync: Option<&DeviceSyncState>,
    live_transport: Option<&str>,
) -> (
    Option<(String, crate::app::home::UiCardConnection)>,
    Vec<UiDeviceCard>,
) {
    let Some(sync) = device_sync else {
        return (None, registry_cards.to_vec());
    };
    let transport = live_transport.unwrap_or_default().to_string();
    let live_name = sync
        .identity
        .as_ref()
        .map(|identity| identity.name.clone())
        .unwrap_or_else(|| "Connected device".to_string());
    // the live device is not "remembered offline" while it's here
    let mut devices: Vec<UiDeviceCard> = registry_cards
        .iter()
        .filter(|card| {
            !matches!((&card.uid, &sync.identity), (Some(uid), Some(identity)) if *uid == identity.uid)
        })
        .cloned()
        .collect();

    match &sync.content {
        DeviceContent::Known {
            project_uid,
            relation,
            ..
        } => {
            return (
                Some((
                    project_uid.clone(),
                    crate::app::home::UiCardConnection {
                        device_name: live_name,
                        relation: *relation,
                    },
                )),
                devices,
            );
        }
        DeviceContent::Adopted { project_uid, .. } => {
            return (
                Some((
                    project_uid.clone(),
                    crate::app::home::UiCardConnection {
                        device_name: live_name,
                        relation: lpc_history::SyncRelation::AtHead,
                    },
                )),
                devices,
            );
        }
        DeviceContent::Empty => devices.push(UiDeviceCard {
            uid: sync.identity.as_ref().map(|identity| identity.uid.clone()),
            name: live_name,
            transport,
            state: UiDeviceCardState::Blank,
        }),
        DeviceContent::PendingIdentity { .. } => devices.push(UiDeviceCard {
            uid: None,
            name: live_name,
            transport,
            state: UiDeviceCardState::ConnectedUnknown {
                detail: "Holds a project — name this device to keep it".to_string(),
            },
        }),
        DeviceContent::Unreadable { detail } => devices.push(UiDeviceCard {
            uid: sync.identity.as_ref().map(|identity| identity.uid.clone()),
            name: live_name,
            transport,
            state: UiDeviceCardState::ConnectedUnknown {
                detail: detail.clone(),
            },
        }),
    }
    (None, devices)
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
        connected_device: None, // stamped by D24 unification at view build
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

fn device_card(device: &RegisteredDevice, projects: &[UiPackageCard]) -> UiDeviceCard {
    // the last-known line prefers the project's current name; a deleted
    // project's association falls back to its uid
    let last_known = device.association.as_ref().map(|association| {
        let uid = association.project.to_string();
        projects
            .iter()
            .find_map(|card| (card.uid == uid).then(|| card.slug.clone()))
            .unwrap_or(uid)
    });
    UiDeviceCard {
        uid: Some(device.uid.clone()),
        name: device.name.clone(),
        // recorded at last sight from the live session's connector class
        transport: device.transport.clone(),
        state: UiDeviceCardState::RememberedOffline {
            last_seen_at: device.last_seen_at,
            last_known,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lpc_history::{DeviceAssociation, PrefixedUid, UidPrefix};
    use lpfs::LpFsMemory;

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
        build_home_view(Some(&inputs), None, None, None, None)
    }

    #[test]
    fn no_library_still_lists_examples() {
        let view = build_home_view(None, None, None, None, None);
        assert!(!view.library_available);
        assert!(view.projects.is_empty());
        assert_eq!(view.examples.len(), embedded_examples().len());
        assert!(view.examples.iter().any(|example| example.name == "Basic"));
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
                    source: "examples/basic".to_string(),
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
        assert_eq!(basic.provenance.as_deref(), Some("Remixed from Basic"));
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
    fn registered_devices_become_remembered_cards_and_parity_lines() {
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
        assert!(matches!(
            view.devices[0].state,
            UiDeviceCardState::RememberedOffline { last_seen_at, .. } if last_seen_at == 5.0
        ));

        let porch = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-porch")
            .unwrap();
        assert_eq!(porch.on_device.as_deref(), Some("Luna's porch sign"));
    }

    #[test]
    fn d24_connected_device_with_local_project_unifies_onto_one_card() {
        use crate::app::places::{DeviceContent, DeviceIdentity, DeviceSyncState};
        use lpc_history::SyncRelation;

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

        let sync = DeviceSyncState {
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
        };
        let view = build_home_view(Some(&inputs), None, None, Some(&sync), Some("USB"));

        assert!(view.devices.is_empty(), "one card, not two (D24)");
        let card = view
            .projects
            .iter()
            .find(|card| card.uid == summary.uid.to_string())
            .unwrap();
        let connection = card.connected_device.as_ref().expect("indication set");
        assert_eq!(connection.device_name, "Porch sign");
        assert_eq!(connection.relation, SyncRelation::Behind);
    }

    #[test]
    fn blank_and_unknown_devices_keep_their_own_cards() {
        use crate::app::places::{DeviceContent, DeviceIdentity, DeviceSyncState};

        let store = store();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let blank = DeviceSyncState {
            identity: Some(DeviceIdentity {
                uid: "dev_bbbbbbbbbbbbbbbb".to_string(),
                name: "Fresh board".to_string(),
            }),
            content: DeviceContent::Empty,
        };
        let view = build_home_view(Some(&inputs), None, None, Some(&blank), Some("USB"));
        assert_eq!(view.devices.len(), 1);
        assert_eq!(view.devices[0].state, UiDeviceCardState::Blank);

        let anonymous = DeviceSyncState {
            identity: None,
            content: DeviceContent::PendingIdentity {
                observed: lpc_history::ContentHash::of(b"x"),
            },
        };
        let view = build_home_view(Some(&inputs), None, None, Some(&anonymous), Some("USB"));
        assert_eq!(view.devices.len(), 1);
        assert!(matches!(
            view.devices[0].state,
            UiDeviceCardState::ConnectedUnknown { .. }
        ));
        assert_eq!(view.devices[0].name, "Connected device");
    }

    #[test]
    fn opening_and_issue_pass_through() {
        let view = build_home_view(
            None,
            Some("prj_x".to_string()),
            Some(UiIssue::new("boom")),
            None,
            None,
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
