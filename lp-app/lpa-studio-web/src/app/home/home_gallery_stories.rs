//! Home gallery stories: first run, populated, opening, and no-store.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use lpa_studio_core::{
    RosterCardState, UiDeviceCard, UiDeviceProjectChip, UiExampleCard, UiHomeView, UiIssue,
    UiPackageCard,
};

use crate::app::home::HomeGallery;
use crate::app::home::card_thumb::CardThumb;
use crate::app::home::gallery_preview::ThumbPreviewBadge;

/// A fixed "now" so relative times in baselines never drift.
const STORY_NOW: f64 = 1_800_000_000.0;

fn examples() -> Vec<UiExampleCard> {
    vec![UiExampleCard {
        id: "examples/basic".to_string(),
        name: "Basic".to_string(),
        kind: "Project".to_string(),
    }]
}

fn packages() -> Vec<UiPackageCard> {
    vec![
        UiPackageCard {
            uid: "prj_3fKq8Zr21bTxYw0AhVmDpe".to_string(),
            kind: "Project".to_string(),
            slug: "2026-07-02-0930-porch-sign".to_string(),
            last_saved_at: Some(STORY_NOW - 2.0 * 3600.0),
            provenance: None,
            on_device: Some("Luna's porch sign".to_string()),
            open_elsewhere: false,
            connected_device: None,
            running_in_sim: false,
        },
        UiPackageCard {
            uid: "prj_9sLm2Xc44dQnUv7BgWkEyt".to_string(),
            kind: "Project".to_string(),
            slug: "2026-07-04-1102-basic".to_string(),
            last_saved_at: Some(STORY_NOW - 5.0 * 86_400.0),
            provenance: Some("Remixed from Basic".to_string()),
            on_device: None,
            open_elsewhere: false,
            connected_device: None,
            running_in_sim: false,
        },
        UiPackageCard {
            uid: "prj_1aBc3De56fGhIj8KlMnOpq".to_string(),
            kind: "Project".to_string(),
            slug: "2026-05-28-1740-porch-sign".to_string(),
            last_saved_at: Some(STORY_NOW - 40.0 * 86_400.0),
            provenance: Some("Forked from 2026-07-02-0930-porch-sign".to_string()),
            on_device: None,
            open_elsewhere: false,
            connected_device: None,
            running_in_sim: false,
        },
    ]
}

fn devices() -> Vec<UiDeviceCard> {
    // the D27 roster: live first (naturally), then last-seen order
    vec![
        UiDeviceCard {
            uid: Some("dev_7pQr5St89uVwXy2CzDaFbg".to_string()),
            name: "Workbench ESP32".to_string(),
            transport: "USB".to_string(),
            state: RosterCardState::RunningUpToDate,
            project: Some(UiDeviceProjectChip {
                uid: "prj_3fKq8Zr21bTxYw0AhVmDpe".to_string(),
                name: "2026-07-02-0930-porch-sign".to_string(),
            }),
            fw: None,
            sim: false,
        },
        UiDeviceCard {
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
            sim: false,
        },
    ]
}

#[story]
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
    };
    rsx! {
        section { class: "tw:p-4",
            HomeGallery {
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
    };
    rsx! {
        section { class: "tw:p-4",
            HomeGallery {
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
        device_name: "Workbench ESP32".to_string(),
        relation: lpa_studio_core::SyncRelation::Behind,
    });
    let mut devices = devices();
    devices[0].state = RosterCardState::RunningBehind {
        observed_version: Some(3),
        head_version: Some(5),
    };
    devices.push(UiDeviceCard {
        uid: Some("dev_4hJk6Lm01nPqRs3T".to_string()),
        name: "Fresh board".to_string(),
        transport: "USB".to_string(),
        state: RosterCardState::ReadyToSetUp,
        project: None,
        fw: None,
        sim: false,
    });
    let home = UiHomeView {
        devices,
        projects,
        examples: examples(),
        library_available: true,
        opening: None,
        issue: None,
    };
    rsx! {
        section { class: "tw:p-4",
            HomeGallery {
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
    };
    rsx! {
        section { class: "tw:p-4",
            HomeGallery {
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
    };
    home.opening = Some(home.projects[0].uid.clone());
    rsx! {
        section { class: "tw:p-4",
            HomeGallery {
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
        sim: true,
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
    }
}

fn gallery(home: UiHomeView, roster_label: Option<String>) -> Element {
    rsx! {
        section { class: "tw:p-4",
            HomeGallery {
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
    description = "The D28 aggregate (M5): ONE project live on both the sim and a device presents 'Live in 2 places' on its card — one line, not two; amber because the device runs behind (the tooltip spells the places out). Chips stay inert pointers — the runtime cards are one glance up."
)]
fn project_live_in_two_places() -> Element {
    // the SAME project on both runtimes: the sim runs it AND the
    // (behind) device holds it — the two D28 facts aggregate
    let mut projects = packages();
    projects[0].running_in_sim = true;
    projects[0].connected_device = Some(lpa_studio_core::UiCardConnection {
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
    };
    rsx! {
        section { class: "tw:p-4",
            HomeGallery {
                home,
                now_secs: Some(STORY_NOW),
                has_ever_granted: Some(true),
                on_action: |_| {},
            }
        }
    }
}
