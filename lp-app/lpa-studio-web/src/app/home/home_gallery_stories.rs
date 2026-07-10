//! Home gallery stories: first run, populated, opening, and no-store.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use lpa_studio_core::{
    UiDeviceCard, UiDeviceCardState, UiExampleCard, UiHomeView, UiIssue, UiPackageCard,
};

use crate::app::home::HomeGallery;

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
        },
        UiPackageCard {
            uid: "prj_9sLm2Xc44dQnUv7BgWkEyt".to_string(),
            kind: "Project".to_string(),
            slug: "2026-07-04-1102-basic".to_string(),
            last_saved_at: Some(STORY_NOW - 5.0 * 86_400.0),
            provenance: Some("Remixed from Basic".to_string()),
            on_device: None,
            open_elsewhere: false,
        },
        UiPackageCard {
            uid: "prj_1aBc3De56fGhIj8KlMnOpq".to_string(),
            kind: "Project".to_string(),
            slug: "2026-05-28-1740-porch-sign".to_string(),
            last_saved_at: Some(STORY_NOW - 40.0 * 86_400.0),
            provenance: Some("Forked from 2026-07-02-0930-porch-sign".to_string()),
            on_device: None,
            open_elsewhere: false,
        },
    ]
}

fn devices() -> Vec<UiDeviceCard> {
    vec![
        UiDeviceCard {
            uid: Some("dev_7pQr5St89uVwXy2CzDaFbg".to_string()),
            name: "Workbench ESP32".to_string(),
            transport: "USB".to_string(),
            state: UiDeviceCardState::ConnectedRunning {
                project: Some("Porch sign".to_string()),
            },
        },
        UiDeviceCard {
            uid: Some("dev_4hJk6Lm01nPqRs3TuVwXyz".to_string()),
            name: "Luna's porch sign".to_string(),
            transport: "USB".to_string(),
            state: UiDeviceCardState::RememberedOffline {
                last_seen_at: STORY_NOW - 3.0 * 86_400.0,
                last_known: Some("2026-07-02-0930-porch-sign".to_string()),
            },
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
