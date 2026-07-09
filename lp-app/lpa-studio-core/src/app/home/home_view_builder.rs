//! Building [`UiHomeView`] from the library, registry, and embedded examples.

use lpc_history::EventKind;

use crate::UiIssue;
use crate::app::library::{LibraryStore, PackageMeta, PackageProvenance};
use crate::app::places::{DeviceRegistry, RegisteredDevice};

use super::embedded_example::embedded_examples;
use super::ui_device_card::{UiDeviceCard, UiDeviceCardState};
use super::ui_example_card::UiExampleCard;
use super::ui_home_view::UiHomeView;
use super::ui_package_card::UiPackageCard;

/// Build the gallery view model. `library` is `None` when no local store
/// mounted (the gallery still shows examples and the connect card).
pub fn build_home_view(
    library: Option<&LibraryStore>,
    opening: Option<String>,
    issue: Option<UiIssue>,
) -> UiHomeView {
    let examples = embedded_examples()
        .iter()
        .map(|example| UiExampleCard {
            id: example.id.to_string(),
            name: example.name.to_string(),
            kind: example.kind.to_string(),
        })
        .collect();

    let Some(store) = library else {
        return UiHomeView {
            devices: Vec::new(),
            projects: Vec::new(),
            examples,
            library_available: false,
            opening,
            issue,
        };
    };

    let registered = DeviceRegistry::new(store.fs_handle())
        .list()
        .unwrap_or_else(|error| {
            log::warn!("home: device registry unreadable: {error}");
            Vec::new()
        });

    let mut issue = issue;
    let projects: Vec<UiPackageCard> = match store.list() {
        Ok(summaries) => summaries
            .into_iter()
            .filter_map(|summary| {
                package_card(store, &registered, summary)
                    .map_err(|error| log::warn!("home: skipping package card: {error}"))
                    .ok()
            })
            .collect(),
        Err(error) => {
            issue.get_or_insert_with(|| {
                UiIssue::new(format!("Your projects could not be listed: {error}"))
            });
            Vec::new()
        }
    };

    let devices = registered
        .iter()
        .map(|device| device_card(device, &projects))
        .collect();

    UiHomeView {
        devices,
        projects,
        examples,
        library_available: true,
        opening,
        issue,
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
        transport: "USB".to_string(),
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

    #[test]
    fn no_library_still_lists_examples() {
        let view = build_home_view(None, None, None);
        assert!(!view.library_available);
        assert!(view.projects.is_empty());
        assert_eq!(view.examples.len(), embedded_examples().len());
        assert!(view.examples.iter().any(|example| example.name == "Basic"));
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

        let view = build_home_view(Some(&store), None, None);
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

        let view = build_home_view(Some(&store), None, None);
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
                last_seen_at: 5.0,
                association: Some(DeviceAssociation {
                    device: device_uid,
                    project: summary.uid,
                    version: head,
                    at: 5.0,
                }),
            })
            .unwrap();

        let view = build_home_view(Some(&store), None, None);
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
    fn opening_and_issue_pass_through() {
        let view = build_home_view(None, Some("prj_x".to_string()), Some(UiIssue::new("boom")));
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
