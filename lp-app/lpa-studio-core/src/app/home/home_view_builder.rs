//! Building [`UiHomeView`] from hydrated library inputs and the runtime
//! pool's evidence.
//!
//! ⚠️ The DEVICE roster (the D27 last-seen-sorted list, the live/remembered
//! merge, `derive_roster_card_state`) was deleted in M2 of the device-model
//! rebuild; the rebuilt model owns that projection. What remains is the
//! library half plus the live SIM card (D36) and the D28 sim pairing —
//! a project the sim runs wears "Running in simulator" on its own card.

use std::cell::RefCell;
use std::rc::Rc;

use lpc_history::EventKind;
use lpfs::LpFs;

use crate::UiIssue;
use crate::app::library::{LibraryStore, PackageMeta, PackageProvenance};
use crate::app::places::{DeviceRegistry, RegisteredDevice};
use crate::app::roster::SimCardState;

use super::card_ui_state::CardUiState;
use super::embedded_example::embedded_examples;
use super::ui_example_card::UiExampleCard;
use super::ui_home_view::UiHomeView;
use super::ui_package_card::UiPackageCard;
use super::ui_sim_card::{UiSimCard, UiSimProjectChip};

/// The gallery's hydrated library data: built asynchronously from a host
/// catalog snapshot (`StudioController::refresh_library`) and cached —
/// `view()` never reads a live store.
#[derive(Debug, Clone, Default)]
pub struct HomeInputs {
    pub projects: Vec<UiPackageCard>,
    /// The raw registry rows — remembered boards, read straight off the
    /// on-disk registry.
    ///
    /// Nothing renders them today (the device roster is being rebuilt), but
    /// they are not presentation: the output face's board diagram needs the
    /// lens device's `board_id`, and the deviceless stub names what Studio
    /// still remembers. Read through `StudioController::lens_board_id`.
    pub registered: Vec<RegisteredDevice>,
    /// Listing failed — the gallery surfaces this instead of an empty
    /// library.
    pub issue: Option<UiIssue>,
}

/// Everything the runtime pool contributes to the gallery: the SIM
/// session's evidence while that session lives.
#[derive(Clone, Debug, Default)]
pub struct HomePoolEvidence {
    /// The live SIM session's evidence — present exactly while the
    /// session lives (D36: the sim card exists only while the session
    /// does; stop-sim removes both together).
    pub sim: Option<HomeSimEvidence>,
}

/// What the live SIM session contributes to its card (D36). The session's
/// existence IS the live status — there is no link state, no connect
/// ceremony, no registry entry (the sim is not a device, D22).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HomeSimEvidence {
    /// The project loaded on the sim (uid + display name), when one is —
    /// the card's chip and the project card's "Running in simulator"
    /// pairing key.
    pub project: Option<UiSimProjectChip>,
    /// The ▶ tab's live frame — the SIM ENGINE'S published output, read
    /// through the session's card feed (G1 ruling 3: the sim card
    /// never re-simulates; it shows what the simulated board actually
    /// output).
    pub frame: Option<crate::UiControlProductPreview>,
    /// Seconds since [`Self::frame`] arrived, stamped at build time.
    pub frame_age_secs: Option<f64>,
    /// The sim engine's reported fps, when known.
    pub fps: Option<f32>,
    /// The board the sim claims to be (vision D4), inherited from the
    /// project it runs — `vendor/product`, the registry's vocabulary.
    /// `None` = no board known, the ordinary default.
    pub board_id: Option<String>,
    /// The session's console tail (D42), oldest first.
    pub console_tail: Vec<crate::UiLogEntry>,
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
        // Every listed package gets a card. Dropping the ones whose history
        // or provenance would not load is how a project vanished from the
        // gallery with only a `log::warn!` behind it; a card missing its
        // "edited 3 days ago" line is a far smaller lie than no card at all.
        Ok(summaries) => summaries
            .into_iter()
            .map(|summary| {
                package_card(&store, &registered, summary.clone()).unwrap_or_else(|error| {
                    log::warn!("home: {} listed without its history: {error}", summary.slug);
                    degraded_package_card(summary)
                })
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

    HomeInputs {
        projects,
        registered,
        issue,
    }
}

/// Assemble the gallery view model from cached inputs. `inputs` is `None`
/// when no local store mounted (the gallery still shows examples). `pool`
/// is the runtime pool's evidence; the D28 sim pairing happens here — the
/// sim session's loaded project stamps its card's "Running in simulator"
/// indication.
pub fn build_home_view(
    inputs: Option<&HomeInputs>,
    opening: Option<String>,
    issue: Option<UiIssue>,
    pool: &HomePoolEvidence,
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
    let sim = pool.sim.as_ref().map(sim_card);

    let Some(inputs) = inputs else {
        return UiHomeView {
            sim,
            projects: Vec::new(),
            examples,
            devices: crate::DeviceRosterView::default(),
            remembered: Vec::new(),
            library_available: false,
            opening,
            issue,
        };
    };

    let mut projects = inputs.projects.clone();
    // The D28 pairing's sim arm: the loaded project's card wears the
    // "Running in simulator" indication.
    if let Some(chip) = pool.sim.as_ref().and_then(|sim| sim.project.as_ref())
        && let Some(card) = projects.iter_mut().find(|card| card.uid == chip.uid)
    {
        card.running_in_sim = true;
    }

    UiHomeView {
        sim,
        projects: dedupe_by_key(projects, |card| card.uid.clone(), "project"),
        examples,
        // Filled by `StudioController::home_view` from the roster: the
        // builder reads the LIBRARY, and the device model is not in it.
        devices: crate::DeviceRosterView::default(),
        // The registry survived the device teardown; the stub names what it
        // holds so the records are visibly intact.
        remembered: inputs
            .registered
            .iter()
            .map(|device| device.name.clone())
            .collect(),
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

/// Collapse the sim card's state to the header control's dots (D16).
pub(crate) fn chip_status(state: SimCardState) -> crate::UiChromeSessionStatus {
    match state {
        SimCardState::Running => crate::UiChromeSessionStatus::Run,
        SimCardState::Empty => crate::UiChromeSessionStatus::Empty,
    }
}

/// The live sim card (D36). The session's existence is the status —
/// Running when a project is loaded, "Connected — nothing loaded"
/// otherwise; no uid, no transport, no firmware provenance (the sim is not
/// a device, D22).
pub(crate) fn sim_card(sim: &HomeSimEvidence) -> UiSimCard {
    UiSimCard {
        state: if sim.project.is_some() {
            SimCardState::Running
        } else {
            SimCardState::Empty
        },
        project: sim.project.clone(),
        // D4: the sim's inherited board — the one card that carries this
        board_id: sim.board_id.clone(),
        console_tail: sim.console_tail.clone(),
        frame_preview: sim.frame.clone(),
        frame_age_secs: sim.frame_age_secs,
        frame_fps: sim.fps,
        ui: CardUiState::default(),
    }
}

/// The display label a pattern project's kind reads as
/// ([`crate::app::library::package_manifest::kind_label`]).
const PATTERN_KIND_LABEL: &str = "Pattern";

/// Every pattern export the library offers, for the add-node picker's
/// import source (module authoring unit, P5).
///
/// Derived from the ALREADY-hydrated gallery cards rather than from a
/// second library read: the snapshot has been walked once at settle, and
/// the picker's needs are exactly two fields that walk already produced.
/// Blocked packages are skipped — a project this build cannot open is not
/// a project it can vendor bytes out of.
pub fn importable_patterns(inputs: &HomeInputs) -> Vec<crate::UiImportablePattern> {
    inputs
        .projects
        .iter()
        .filter(|card| card.project_kind == PATTERN_KIND_LABEL && card.health.is_openable())
        .flat_map(|card| {
            let family = card.exports.len() > 1;
            card.exports
                .iter()
                .map(move |export| crate::UiImportablePattern {
                    package_uid: card.uid.clone(),
                    package_label: card.slug.clone(),
                    export: export.clone(),
                    family,
                })
        })
        .collect()
}

fn package_card(
    store: &LibraryStore,
    registered: &[RegisteredDevice],
    summary: crate::app::library::PackageSummary,
) -> Result<UiPackageCard, crate::app::library::LibraryError> {
    let handle = store.open(summary.uid)?;
    let meta = crate::app::library::package_meta::read_meta(&*handle.package_fs.borrow())?;
    // Advisory board target (vision D3) + authored project kind (module
    // authoring unit, P1): straight passthrough from the container
    // manifest, same seam `provenance`/`on_device` use — no catalog lookup
    // here, that's the web renderer's job.
    let manifest_fields =
        crate::app::library::package_manifest::read_manifest(&*handle.package_fs.borrow())?;
    let target = manifest_fields.target;
    let project_kind =
        crate::app::library::package_manifest::kind_label(&manifest_fields.kind).to_string();
    let exports = manifest_fields.exports;

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
        project_kind,
        exports,
        slug: summary.slug,
        last_saved_at,
        provenance: meta.and_then(|meta| provenance_line(store, &meta)),
        on_device,
        open_elsewhere: false, // stamped by the hydration pass
        running_in_sim: false, // stamped by the D28 sim arm at view build
        target,
        health: summary.health,
    })
}

/// The card for a package whose history or provenance would not open. The
/// summary is all we have — and it is enough to name the package, show its
/// health, and offer export and delete.
fn degraded_package_card(summary: crate::app::library::PackageSummary) -> UiPackageCard {
    UiPackageCard {
        uid: summary.uid.to_string(),
        kind: summary.kind,
        // A degraded package's manifest may be unreadable; fall back to
        // the default kind rather than claim one we could not read.
        project_kind: crate::app::library::package_manifest::kind_label(
            &lpc_model::ProjectKind::General,
        )
        .to_string(),
        // Same reason as the kind above: nothing was readable, so nothing
        // is claimed.
        exports: Vec::new(),
        slug: summary.slug,
        last_saved_at: None,
        provenance: None,
        on_device: None,
        open_elsewhere: false,
        running_in_sim: false,
        // A degraded package's manifest may be unreadable; no target claim.
        target: None,
        health: summary.health,
    }
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
        PackageProvenance::ImportedJson { .. } => Some("Pasted from JSON".to_string()),
        PackageProvenance::PulledFromDevice { device_name, .. } => {
            Some(format!("Pulled from {device_name}"))
        }
        // Project-name-centric on purpose: the service exposes no owner
        // profile, and guessing would be worse than saying what it is.
        PackageProvenance::OpenedFromLink => Some("Shared with you".to_string()),
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

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
        build_home_view(Some(&inputs), None, None, &HomePoolEvidence::default())
    }

    /// A pool carrying only the live sim session's evidence.
    fn sim_pool(project: Option<UiSimProjectChip>) -> HomePoolEvidence {
        HomePoolEvidence {
            sim: Some(HomeSimEvidence {
                project,
                ..HomeSimEvidence::default()
            }),
        }
    }

    #[test]
    fn an_unopenable_package_still_gets_a_card() {
        // Swallow point 2: the card builder's `filter_map` dropped anything
        // whose history or provenance would not load, on top of the store
        // already dropping anything whose manifest would not parse. A card
        // that says what is wrong is the only honest outcome.
        let store = store();
        store.create("Healthy", 1.0).unwrap();
        {
            let fs = store.fs_handle();
            let view = fs.borrow();
            view.write_file(
                lpc_model::LpPath::new("/packages/z-hand-copied/project.json"),
                br#"{"kind":"Project","format":2,"nodes":{}}"#,
            )
            .unwrap();
        }

        let view = view_of(&store);
        assert_eq!(view.projects.len(), 2, "neither package vanished");
        let stale = view
            .projects
            .iter()
            .find(|card| card.slug == "z-hand-copied")
            .expect("the unreadable package has a card");
        let (headline, remedy) = stale.health.blocked().expect("classified as blocked");
        assert_eq!(headline, "Format 2 — too old for this Studio");
        assert!(
            remedy.contains("too old to upgrade automatically"),
            "{remedy}"
        );
        // its identity is addressable, so the card's delete/export work
        assert!(stale.uid.starts_with("prj"));

        let healthy = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-healthy")
            .expect("the healthy package is unaffected");
        assert_eq!(healthy.health, crate::app::library::PackageHealth::Ready);
    }

    #[test]
    fn no_library_still_lists_examples() {
        let view = build_home_view(None, None, None, &HomePoolEvidence::default());
        assert!(!view.library_available);
        assert!(view.projects.is_empty());
        assert!(view.sim.is_none());
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
                    br#"{"format":10,"name":"Basic"}"#.to_vec(),
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
        assert_eq!(scratch.kind, "Module");
    }

    /// P02: the advisory `target` passes through from the container
    /// manifest to the card, and stays `None` for an untargeted project
    /// (the common case — `store.create` writes no `target`).
    #[test]
    fn package_cards_carry_advisory_target() {
        let store = store();
        store.create("Untargeted", 1.0).unwrap();
        store
            .install_package(
                "Targeted",
                &[(
                    "project.json".to_string(),
                    br#"{"format":4,"name":"Targeted","target":"espressif/esp32-c6-devkitc-1"}"#
                        .to_vec(),
                )],
                PackageProvenance::Created,
                2.0,
            )
            .unwrap();

        let view = view_of(&store);
        let targeted = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-targeted")
            .unwrap();
        assert_eq!(
            targeted.target.as_deref(),
            Some("espressif/esp32-c6-devkitc-1")
        );

        let untargeted = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-untargeted")
            .unwrap();
        assert_eq!(untargeted.target, None);
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
    fn opening_and_issue_pass_through() {
        let view = build_home_view(
            None,
            Some("prjx".to_string()),
            Some(UiIssue::new("boom")),
            &HomePoolEvidence::default(),
        );
        assert_eq!(view.opening.as_deref(), Some("prjx"));
        assert_eq!(view.issue.as_ref().unwrap().message, "boom");
        assert_eq!(
            view.render_text_lines(),
            vec![
                format!(
                    "Home: 0 runtimes, 0 projects, {} examples, 0 remembered",
                    embedded_examples().len()
                ),
                "  opening prjx".to_string(),
                "  issue: boom".to_string(),
            ]
        );
    }

    #[test]
    fn sim_session_yields_the_live_sim_card_and_stamps_the_project() {
        // D36 + the D28 sim arm: a live sim session running a known
        // project = a Running sim card wearing the project chip, AND the
        // project card wearing "Running in simulator".
        let store = store();
        let summary = store.create("Porch", 1.0).unwrap();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let pool = sim_pool(Some(UiSimProjectChip {
            uid: summary.uid.to_string(),
            name: summary.slug.clone(),
        }));
        let view = build_home_view(Some(&inputs), None, None, &pool);

        let card = view.sim.as_ref().expect("the live sim card");
        assert_eq!(card.render_key(), "runtime-sim");
        assert_eq!(card.state, SimCardState::Running);
        let chip = card.project.as_ref().expect("loaded project chip");
        assert_eq!(chip.name, summary.slug);

        let project = view
            .projects
            .iter()
            .find(|card| card.uid == summary.uid.to_string())
            .unwrap();
        assert!(project.running_in_sim, "the sim arm stamps the project");
    }

    #[test]
    fn sim_with_nothing_loaded_reads_connected_empty() {
        let store = store();
        store.create("Porch", 1.0).unwrap();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let view = build_home_view(Some(&inputs), None, None, &sim_pool(None));
        let card = view.sim.as_ref().expect("the live sim card");
        assert_eq!(card.state, SimCardState::Empty);
        assert!(card.project.is_none());
        assert!(
            view.projects.iter().all(|card| !card.running_in_sim),
            "no loaded project, no sim stamp"
        );
    }

    /// The header control's dots collapse the sim's two rows honestly:
    /// running reads Run, empty reads Empty — nothing ever reads Attention,
    /// because neither sim state is a problem.
    #[test]
    fn the_header_dot_follows_the_sim_state() {
        assert_eq!(
            chip_status(SimCardState::Running),
            crate::UiChromeSessionStatus::Run
        );
        assert_eq!(
            chip_status(SimCardState::Empty),
            crate::UiChromeSessionStatus::Empty
        );
    }
}
