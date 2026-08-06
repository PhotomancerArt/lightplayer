//! Gallery-page stories: first run, populated, opening, and no-store.
//! The P09 split divided the combined gallery into Devices / Projects /
//! Explore pages; these stories stack all three from one fixture so the
//! old coverage (and the cross-page states, like empty-device push
//! buttons) stays in frame.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use lpa_studio_core::app::library::PackageHealth;
use lpa_studio_core::{
    RosterCardState, UiDeviceCard, UiDeviceProjectChip, UiExampleCard, UiHomeView, UiIssue,
    UiPackageCard,
};

use lpa_studio_core::UiAction;

use crate::app::home::card_thumb::CardThumb;
use crate::app::home::gallery_preview::ThumbPreviewBadge;
use crate::app::home::{DevicesPage, ExplorePage, ProjectsPage};

/// A fixed "now" so relative times in baselines never drift.
const STORY_NOW: f64 = 1_800_000_000.0;

fn examples() -> Vec<UiExampleCard> {
    vec![UiExampleCard {
        id: "examples/basic".to_string(),
        name: "Basic".to_string(),
        kind: "Module".to_string(),
    }]
}

fn packages() -> Vec<UiPackageCard> {
    vec![
        UiPackageCard {
            uid: "prj_3fKq8Zr21bTxYw0AhVmDpe".to_string(),
            kind: "Module".to_string(),
            slug: "2026-07-02-0930-porch-sign".to_string(),
            last_saved_at: Some(STORY_NOW - 2.0 * 3600.0),
            provenance: None,
            on_device: Some("Luna's porch sign".to_string()),
            open_elsewhere: false,
            connected_device: None,
            running_in_sim: false,
            target: None,
            health: PackageHealth::Ready,
        },
        UiPackageCard {
            uid: "prj_9sLm2Xc44dQnUv7BgWkEyt".to_string(),
            kind: "Module".to_string(),
            slug: "2026-07-04-1102-basic".to_string(),
            last_saved_at: Some(STORY_NOW - 5.0 * 86_400.0),
            provenance: Some("Remixed from Basic".to_string()),
            on_device: None,
            open_elsewhere: false,
            connected_device: None,
            running_in_sim: false,
            target: None,
            health: PackageHealth::Ready,
        },
        UiPackageCard {
            uid: "prj_1aBc3De56fGhIj8KlMnOpq".to_string(),
            kind: "Module".to_string(),
            slug: "2026-05-28-1740-porch-sign".to_string(),
            last_saved_at: Some(STORY_NOW - 40.0 * 86_400.0),
            provenance: Some("Forked from 2026-07-02-0930-porch-sign".to_string()),
            on_device: None,
            open_elsewhere: false,
            connected_device: None,
            running_in_sim: false,
            target: None,
            health: PackageHealth::Ready,
        },
    ]
}

fn devices() -> Vec<UiDeviceCard> {
    // the D27 roster: live first (naturally), then last-seen order
    vec![
        UiDeviceCard {
            port_label: None,
            session_key: None,
            uid: Some("dev_7pQr5St89uVwXy2CzDaFbg".to_string()),
            name: "Workbench ESP32".to_string(),
            transport: "USB".to_string(),
            state: RosterCardState::RunningUpToDate,
            project: Some(UiDeviceProjectChip {
                uid: "prj_3fKq8Zr21bTxYw0AhVmDpe".to_string(),
                name: "2026-07-02-0930-porch-sign".to_string(),
            }),
            fw: None,
            hardware: None,
            safe_clamp: None,
            sim: false,
            console_tail: Vec::new(),
            ui: Default::default(),
            detected_chip: None,
            board_id: None,
        },
        UiDeviceCard {
            port_label: None,
            session_key: None,
            uid: Some("dev_4hJk6Lm01nPqRs3TuVwXyz".to_string()),
            name: "Luna's porch sign".to_string(),
            transport: "USB".to_string(),
            state: RosterCardState::Offline {
                last_seen_at: Some(STORY_NOW - 3.0 * 86_400.0),
            },
            project: Some(UiDeviceProjectChip {
                uid: "prj_3fKq8Zr21bTxYw0AhVmDpe".to_string(),
                name: "2026-07-02-0930-porch-sign".to_string(),
            }),
            fw: None,
            hardware: None,
            safe_clamp: None,
            sim: false,
            console_tail: Vec::new(),
            ui: Default::default(),
            detected_chip: None,
            board_id: None,
        },
    ]
}

#[story(
    description = "First run, create-first since the D17 deviation (2026-07-27): the empty Projects section header carries the New chip beside Import — a pure-blank create-and-open — and the empty-library copy leads with creating a project before pointing at the examples."
)]
fn first_run() -> Element {
    // no devices ever granted: the Connected section collapses to a slim
    // affordance; the library holds nothing yet
    let home = UiHomeView {
        devices: Vec::new(),
        projects: Vec::new(),
        examples: examples(),
        library_available: true,
        opening: None,
        issue: None,
        backup: None,
        setup: None,
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages {
                home,
                now_secs: Some(STORY_NOW),
                has_ever_granted: Some(false),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "The gallery AS the project chooser (state-flow model §1-A, settled 2026-07-26): a connected-EMPTY device grows an explicit \"Put on <name>\" button on every project card, beside Open-in-sim — the target is always named, never guessed. The card-resident Project-tab picker stays as the second door."
)]
fn gallery_chooser_buttons() -> Element {
    let mut roster = devices();
    roster[0].state = RosterCardState::ConnectedEmpty;
    roster[0].project = None;
    let home = UiHomeView {
        devices: roster,
        projects: packages(),
        examples: examples(),
        library_available: true,
        opening: None,
        issue: None,
        backup: None,
        setup: None,
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages {
                home,
                now_secs: Some(STORY_NOW),
                has_ever_granted: Some(true),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "Project format states (P3): a package NEVER vanishes for being unreadable. A format-4 project carries a quiet \"upgrades when you open it\" line and is otherwise a normal card; below-floor, future-format and unreadable packages wear the amber edge, say what was found and what to do, and drop their open affordance for the two remedies that work on raw files — Export zip on the card, delete in the menu."
)]
fn project_format_states() -> Element {
    let mut projects = packages();
    projects[0].health = PackageHealth::UpgradesOnOpen { found: 4 };
    projects[1].health = PackageHealth::Blocked {
        headline: "Format 3 — too old for this Studio".to_string(),
        remedy: "Project format 3, expected 5; formats below 4 are too old to upgrade \
                 automatically. Open it in a LightPlayer that still reads format 3 and \
                 re-save it, or rebuild the project."
            .to_string(),
    };
    projects[2].health = PackageHealth::Blocked {
        headline: "Format 7 — made by a newer LightPlayer".to_string(),
        remedy: "Project format 7, expected 5; it was written by a newer LightPlayer. \
                 Update LightPlayer to open it."
            .to_string(),
    };
    projects.push(UiPackageCard {
        uid: "prj_5tYu7Vw90xZaBc4DeFgHi".to_string(),
        kind: "Module".to_string(),
        slug: "2026-06-11-0815-half-written".to_string(),
        last_saved_at: None,
        provenance: None,
        on_device: None,
        open_elsewhere: false,
        connected_device: None,
        running_in_sim: false,
        target: None,
        health: PackageHealth::Blocked {
            headline: "project.json could not be read".to_string(),
            remedy: "project.json could not be read as a project manifest (expected value at \
                     line 1 column 1); expected a JSON object stating format 5. Fix or restore \
                     the file before opening the project."
                .to_string(),
        },
    });
    let home = UiHomeView {
        devices: Vec::new(),
        projects,
        examples: examples(),
        library_available: true,
        opening: None,
        issue: None,
        backup: None,
        setup: None,
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages {
                home,
                now_secs: Some(STORY_NOW),
                has_ever_granted: Some(false),
                on_action: |_| {},
            }
        }
    }
}

#[story]
fn populated() -> Element {
    let home = UiHomeView {
        devices: devices(),
        projects: packages(),
        examples: examples(),
        library_available: true,
        opening: None,
        issue: None,
        backup: None,
        setup: None,
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages {
                home,
                now_secs: Some(STORY_NOW),
                has_ever_granted: Some(true),
                on_action: |_| {},
            }
        }
    }
}

#[story]
fn connected_device_and_project_chip() -> Element {
    // D28 (D24's collapse is gone): a connected device holding a known
    // project keeps its DEVICE card and the project card carries the live
    // chip — one fact, two views. A blank second board rides alongside.
    use lpa_studio_core::UiCardConnection;

    let mut projects = packages();
    projects[0].connected_device = Some(UiCardConnection {
        device_key: "runtime-1".to_string(),
        device_name: "Workbench ESP32".to_string(),
        relation: lpa_studio_core::SyncRelation::Behind,
    });
    let mut devices = devices();
    devices[0].state = RosterCardState::RunningBehind {
        observed_version: Some(3),
        head_version: Some(5),
    };
    devices.push(UiDeviceCard {
        port_label: None,
        session_key: None,
        uid: Some("dev_4hJk6Lm01nPqRs3T".to_string()),
        name: "Fresh board".to_string(),
        transport: "USB".to_string(),
        state: RosterCardState::ReadyToSetUp,
        project: None,
        fw: None,
        hardware: None,
        safe_clamp: None,
        sim: false,
        console_tail: Vec::new(),
        ui: Default::default(),
        detected_chip: None,
        board_id: None,
    });
    let home = UiHomeView {
        devices,
        projects,
        examples: examples(),
        library_available: true,
        opening: None,
        issue: None,
        backup: None,
        setup: None,
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages {
                home,
                now_secs: Some(STORY_NOW),
                has_ever_granted: Some(true),
                on_action: |_| {},
            }
        }
    }
}

#[story]
fn project_open_in_another_tab() -> Element {
    // M4b: a project another tab holds open — neutral badge, card stays
    // fully rendered and clickable (the refusal notice explains)
    let mut projects = packages();
    projects[0].open_elsewhere = true;
    let home = UiHomeView {
        devices: Vec::new(),
        projects,
        examples: examples(),
        library_available: true,
        opening: None,
        issue: None,
        backup: None,
        setup: None,
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages {
                home,
                now_secs: Some(STORY_NOW),
                has_ever_granted: Some(false),
                on_action: |_| {},
            }
        }
    }
}

#[story]
fn opening_a_project() -> Element {
    let mut home = UiHomeView {
        devices: Vec::new(),
        projects: packages(),
        examples: examples(),
        library_available: true,
        opening: None,
        issue: None,
        backup: None,
        setup: None,
    };
    home.opening = Some(home.projects[0].uid.clone());
    rsx! {
        section { class: "tw:p-4",
            GalleryPages {
                home,
                now_secs: Some(STORY_NOW),
                has_ever_granted: Some(false),
                on_action: |_| {},
            }
        }
    }
}

#[story]
fn live_thumb_states() -> Element {
    // The live-thumb overlay states, injected statically (story mode has
    // no PreviewHost and mounts no canvas): placeholder gradient, GPU
    // tier, CPU fallback with a surfaced reason, and a failed preview.
    // Live cards derive the same badges from their slot status.
    rsx! {
        section { class: "tw:grid tw:w-[720px] tw:grid-cols-4 tw:gap-3.5 tw:p-4",
            article { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                CardThumb { seed: "prj_3fKq8Zr21bTxYw0AhVmDpe".to_string(), label: "placeholder".to_string() }
                p { class: thumb_state_caption_class(), "Placeholder" }
            }
            article { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                CardThumb {
                    seed: "prj_9sLm2Xc44dQnUv7BgWkEyt".to_string(),
                    label: "gpu".to_string(),
                    static_badge: Some(ThumbPreviewBadge::Gpu),
                }
                p { class: thumb_state_caption_class(), "GPU tier" }
            }
            article { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                CardThumb {
                    seed: "prj_1aBc3De56fGhIj8KlMnOpq".to_string(),
                    label: "cpu".to_string(),
                    static_badge: Some(ThumbPreviewBadge::Cpu {
                        reason: Some("WebGPU unavailable".to_string()),
                    }),
                }
                p { class: thumb_state_caption_class(), "CPU fallback" }
            }
            article { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                CardThumb {
                    seed: "examples/basic".to_string(),
                    label: "failed".to_string(),
                    static_badge: Some(ThumbPreviewBadge::Error {
                        reason: "deploy: shader compile failed".to_string(),
                    }),
                }
                p { class: thumb_state_caption_class(), "Failed" }
            }
        }
    }
}

fn thumb_state_caption_class() -> &'static str {
    "tw:m-0 tw:p-3 tw:text-xs tw:text-muted-foreground"
}

/// The live sim card (D36) as the pool evidence produces it: Running with
/// the loaded project's chip, or "nothing loaded".
fn sim_device_card(with_project: bool) -> UiDeviceCard {
    UiDeviceCard {
        port_label: None,
        session_key: None,
        uid: None,
        name: "Simulator".to_string(),
        transport: String::new(),
        state: if with_project {
            RosterCardState::RunningUpToDate
        } else {
            RosterCardState::ConnectedEmpty
        },
        project: with_project.then(|| UiDeviceProjectChip {
            uid: "prj_3fKq8Zr21bTxYw0AhVmDpe".to_string(),
            name: "2026-07-02-0930-porch-sign".to_string(),
        }),
        fw: None,
        hardware: None,
        safe_clamp: None,
        sim: true,
        console_tail: Vec::new(),
        ui: Default::default(),
        detected_chip: None,
        board_id: None,
    }
}

/// The sim + live device gallery (runtime-pool P4): the roster leads the
/// page with both live cards; the sim's project card wears "Running in
/// simulator" while the device's wears its connected line — the D28
/// pairings side by side.
fn sim_and_live_device_home() -> UiHomeView {
    let mut projects = packages();
    projects[0].running_in_sim = true;
    projects[1].connected_device = Some(lpa_studio_core::UiCardConnection {
        device_key: "runtime-1".to_string(),
        device_name: "Workbench ESP32".to_string(),
        relation: lpa_studio_core::SyncRelation::AtHead,
    });
    let mut device = devices().remove(0);
    device.project = Some(UiDeviceProjectChip {
        uid: "prj_9sLm2Xc44dQnUv7BgWkEyt".to_string(),
        name: "2026-07-04-1102-basic".to_string(),
    });
    UiHomeView {
        devices: vec![sim_device_card(true), device],
        projects,
        examples: examples(),
        library_available: true,
        opening: None,
        issue: None,
        backup: None,
        setup: None,
    }
}

fn gallery(home: UiHomeView, roster_label: Option<String>) -> Element {
    rsx! {
        section { class: "tw:p-4",
            GalleryPages {
                home,
                now_secs: Some(STORY_NOW),
                has_ever_granted: Some(true),
                roster_label,
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "D36: only the sim session lives — the roster leads with the sim card (Running + project chip) and the loaded project's card wears 'Running in simulator'."
)]
fn sim_running_only() -> Element {
    let mut projects = packages();
    projects[0].running_in_sim = true;
    gallery(
        UiHomeView {
            devices: vec![sim_device_card(true)],
            projects,
            examples: examples(),
            library_available: true,
            opening: None,
            issue: None,
            backup: None,
            setup: None,
        },
        None,
    )
}

#[story(
    description = "Coexistence on the roster (P4): the sim card first among live, a live device beside it, and both D28 project pairings — 'Running in simulator' and the connected line."
)]
fn sim_and_live_device() -> Element {
    gallery(sim_and_live_device_home(), None)
}

#[story(
    description = "Safe mode: the device booted with the recovery output clamp (dim on purpose). The card wears the warning callout on every tab — what happened, and that a replug is the exit — because a clamped board otherwise just looks broken."
)]
fn device_in_safe_mode() -> Element {
    let mut device = devices().remove(0);
    device.safe_clamp = Some(26);
    gallery(
        UiHomeView {
            devices: vec![device],
            projects: packages(),
            examples: examples(),
            library_available: true,
            opening: None,
            issue: None,
            backup: None,
            setup: None,
        },
        None,
    )
}

#[story(
    description = "The D28 aggregate (M5): ONE project live on both the sim and a device presents 'Live in 2 places' on its card — one line, not two; amber because the device runs behind (the tooltip spells the places out). Chips stay inert pointers — the runtime cards are one glance up."
)]
fn project_live_in_two_places() -> Element {
    // the SAME project on both runtimes: the sim runs it AND the
    // (behind) device holds it — the two D28 facts aggregate
    let mut projects = packages();
    projects[0].running_in_sim = true;
    projects[0].connected_device = Some(lpa_studio_core::UiCardConnection {
        device_key: "runtime-1".to_string(),
        device_name: "Workbench ESP32".to_string(),
        relation: lpa_studio_core::SyncRelation::Behind,
    });
    let mut device = devices().remove(0);
    device.state = RosterCardState::RunningBehind {
        observed_version: Some(3),
        head_version: Some(5),
    };
    gallery(
        UiHomeView {
            devices: vec![sim_device_card(true), device],
            projects,
            examples: examples(),
            library_available: true,
            opening: None,
            issue: None,
            backup: None,
            setup: None,
        },
        None,
    )
}

#[story(
    description = "The sim card alongside a remembered (offline) device: live leads, the offline card keeps its last-seen fade."
)]
fn sim_and_offline_device() -> Element {
    let mut projects = packages();
    projects[0].running_in_sim = true;
    let offline = devices().remove(1);
    gallery(
        UiHomeView {
            devices: vec![sim_device_card(true), offline],
            projects,
            examples: examples(),
            library_available: true,
            opening: None,
            issue: None,
            backup: None,
            setup: None,
        },
        None,
    )
}

#[story(
    description = "Section-label candidate 'Devices' (the current label) over the top-of-page roster with sim + device cards — the P4 gate compares the three candidates on identical content."
)]
fn roster_label_devices() -> Element {
    gallery(sim_and_live_device_home(), Some("Devices".to_string()))
}

#[story(
    description = "Section-label candidate 'Running' over the same top-of-page roster (story-only override; the rendered product label stays 'Devices' until the gate decides)."
)]
fn roster_label_running() -> Element {
    gallery(sim_and_live_device_home(), Some("Running".to_string()))
}

#[story(
    description = "Section-label candidate 'Open' over the same top-of-page roster (story-only override; the rendered product label stays 'Devices' until the gate decides)."
)]
fn roster_label_open() -> Element {
    gallery(sim_and_live_device_home(), Some("Open".to_string()))
}

#[story]
fn store_unavailable_with_issue() -> Element {
    let home = UiHomeView {
        devices: Vec::new(),
        projects: Vec::new(),
        examples: examples(),
        library_available: false,
        opening: None,
        issue: Some(UiIssue::new("Failed to open serial port.")),
        backup: None,
        setup: None,
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages {
                home,
                now_secs: Some(STORY_NOW),
                has_ever_granted: Some(true),
                on_action: |_| {},
            }
        }
    }
}

/// The P09 pages stacked from one fixture — the story stand-in for the
/// old combined gallery page (the app renders them on separate routes).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn GalleryPages(
    home: UiHomeView,
    #[props(default)] now_secs: Option<f64>,
    #[props(default)] has_ever_granted: Option<bool>,
    #[props(default)] roster_label: Option<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-10",
            DevicesPage {
                home: home.clone(),
                now_secs,
                has_ever_granted,
                roster_label,
                on_action,
            }
            ProjectsPage { home: home.clone(), now_secs, on_action }
            ExplorePage { home: Some(home), on_action }
        }
    }
}
