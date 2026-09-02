//! Gallery-page stories: first run, populated, opening, and no-store.
//! The P09 split divided the combined gallery into Devices / Projects /
//! Explore pages; these stories stack all three from one fixture so the
//! old coverage stays in frame.
//!
//! ⚠️ The DEVICE-roster rows (the connected/offline/blank/safe-mode cards,
//! the empty-device push buttons, the section-label candidates over a
//! device roster) went with M2 of the device-model rebuild. What is left
//! covers the library pages and the live sim card.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use lpa_studio_core::app::library::PackageHealth;
use lpa_studio_core::{
    ColorOrder, ControlDisplayLayout, ControlExtent, ControlLamp2d, ControlLayout2d,
    ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan, Revision, SimCardState,
    UiControlProductPreview, UiControlSampleFormat, UiExampleCard, UiHomeView, UiIssue,
    UiPackageCard, UiSimCard, UiSimProjectChip,
};

use lpa_studio_core::UiAction;
use lpa_studio_core::{
    DeviceActivityKind, DeviceActivityView, DeviceEscape, DeviceId, DeviceLinkId,
    DeviceLoadedProject, DeviceRosterView, DeviceStatus, DeviceView, OutcomeView, PendingLinkView,
    RosterView,
};

use crate::app::home::card_thumb::CardThumb;
use crate::app::home::device_roster_card::DeviceRosterCard;
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
            uid: "prj3fKq8Zr21bTxYw0AhVmDpe".to_string(),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: "2026-07-02-0930-porch-sign".to_string(),
            last_saved_at: Some(STORY_NOW - 2.0 * 3600.0),
            provenance: None,
            on_device: Some("Luna's porch sign".to_string()),
            open_elsewhere: false,
            running_in_sim: false,
            target: None,
            health: PackageHealth::Ready,
        },
        UiPackageCard {
            uid: "prj9sLm2Xc44dQnUv7BgWkEyt".to_string(),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: "2026-07-04-1102-basic".to_string(),
            last_saved_at: Some(STORY_NOW - 5.0 * 86_400.0),
            provenance: Some("Remixed from Basic".to_string()),
            on_device: None,
            open_elsewhere: false,
            running_in_sim: false,
            target: None,
            health: PackageHealth::Ready,
        },
        UiPackageCard {
            uid: "prj1aBc3De56fGhIj8KlMnOpq".to_string(),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: "2026-05-28-1740-porch-sign".to_string(),
            last_saved_at: Some(STORY_NOW - 40.0 * 86_400.0),
            provenance: Some("Forked from 2026-07-02-0930-porch-sign".to_string()),
            on_device: None,
            open_elsewhere: false,
            running_in_sim: false,
            target: None,
            health: PackageHealth::Ready,
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
        sim: None,
        projects: Vec::new(),
        examples: examples(),
        devices: Default::default(),
        remembered: Vec::new(),
        library_available: true,
        opening: None,
        issue: None,
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages { home, now_secs: Some(STORY_NOW), on_action: |_| {} }
        }
    }
}

#[story(
    description = "Project format states (P3): a package NEVER vanishes for being unreadable. A format-4 project carries a quiet \"upgrades when you open it\" line and is otherwise a normal card; below-floor, future-format and unreadable packages wear the amber edge, say what was found and what to do, and drop their open affordance for the two remedies that work on raw files — Download zip on the card, delete in the menu."
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
        uid: "prj5tYu7Vw90xZaBc4DeFgHi".to_string(),
        kind: "Module".to_string(),
        project_kind: "General".to_string(),
        exports: Vec::new(),
        slug: "2026-06-11-0815-half-written".to_string(),
        last_saved_at: None,
        provenance: None,
        on_device: None,
        open_elsewhere: false,
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
        sim: None,
        projects,
        examples: examples(),
        devices: Default::default(),
        remembered: Vec::new(),
        library_available: true,
        opening: None,
        issue: None,
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages { home, now_secs: Some(STORY_NOW), on_action: |_| {} }
        }
    }
}

#[story]
fn populated() -> Element {
    let home = UiHomeView {
        sim: Some(sim_card_fixture(true)),
        projects: packages(),
        examples: examples(),
        devices: Default::default(),
        remembered: Vec::new(),
        library_available: true,
        opening: None,
        issue: None,
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages { home, now_secs: Some(STORY_NOW), on_action: |_| {} }
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
        sim: None,
        projects,
        examples: examples(),
        devices: Default::default(),
        remembered: Vec::new(),
        library_available: true,
        opening: None,
        issue: None,
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages { home, now_secs: Some(STORY_NOW), on_action: |_| {} }
        }
    }
}

#[story]
fn opening_a_project() -> Element {
    let mut home = UiHomeView {
        sim: None,
        projects: packages(),
        examples: examples(),
        devices: Default::default(),
        remembered: Vec::new(),
        library_available: true,
        opening: None,
        issue: None,
    };
    home.opening = Some(home.projects[0].uid.clone());
    rsx! {
        section { class: "tw:p-4",
            GalleryPages { home, now_secs: Some(STORY_NOW), on_action: |_| {} }
        }
    }
}

#[story]
fn live_thumb_states() -> Element {
    // The live-thumb overlay states, injected statically (story mode has
    // no PreviewHost and mounts no canvas): placeholder gradient, GPU
    // tier, CPU fallback with a surfaced reason, and a failed preview.
    // Badge policy is issue-only (fidelity-tiers ADR, decision-4 note), so
    // the GPU and CPU cards here PROVE the absence: only the failure wears
    // a badge. Tier state stays log/wire-visible for diagnosis.
    rsx! {
        section { class: "tw:grid tw:w-[720px] tw:grid-cols-4 tw:gap-3.5 tw:p-4",
            article { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                CardThumb { seed: "prj3fKq8Zr21bTxYw0AhVmDpe".to_string(), label: "placeholder".to_string() }
                p { class: thumb_state_caption_class(), "Placeholder" }
            }
            article { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                CardThumb {
                    seed: "prj9sLm2Xc44dQnUv7BgWkEyt".to_string(),
                    label: "gpu".to_string(),
                    static_badge: Some(ThumbPreviewBadge::Gpu),
                }
                p { class: thumb_state_caption_class(), "GPU tier — no badge" }
            }
            article { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                CardThumb {
                    seed: "prj1aBc3De56fGhIj8KlMnOpq".to_string(),
                    label: "cpu".to_string(),
                    static_badge: Some(ThumbPreviewBadge::Cpu {
                        reason: Some("WebGPU unavailable".to_string()),
                    }),
                }
                p { class: thumb_state_caption_class(), "CPU fallback — no badge" }
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

#[story]
fn thumb_product_faces() -> Element {
    // The two faces a card thumb can wear (root-module-product-display Q2):
    // a CONTROL-FIRST project — its root scope resolves `control.out` — shows
    // the fixture's lamps, and everything else keeps the raster. Story mode
    // leases no slot, so the lamp field is injected; the live thumb draws the
    // identical `LampView` from the slot's output frames, and the shader-only
    // card's raster is its live canvas (here: the placeholder it reveals
    // over).
    rsx! {
        section { class: "tw:grid tw:w-[480px] tw:grid-cols-2 tw:gap-3.5 tw:p-4",
            article { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                CardThumb {
                    seed: "prj3fkq8zr21btxyw0a".to_string(),
                    label: "porch-sign".to_string(),
                    static_lamps: Some(thumb_lamp_frame()),
                }
                p { class: thumb_state_caption_class(), "Control-first — lamps" }
            }
            article { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                CardThumb {
                    seed: "prj9sm2xc44dqnv7bgw".to_string(),
                    label: "plasma".to_string(),
                }
                p { class: thumb_state_caption_class(), "Shader-only — raster" }
            }
        }
    }
}

/// A deterministic 2×2 violet PNG data URL, hand-built (not a rendered
/// capture) so the poster-state baselines are reproducible bytes rather
/// than anything a live slot or worker produced — see the poster-first
/// gallery previews ADR (`docs/adr/`).
const POSTER_TEST_IMAGE: &str = "data:image/png;base64,\
iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR42mOosXoLRAwQCgAsHgaNmEOi\
1gAAAABJRU5ErkJggg==";

#[story]
fn poster_states() -> Element {
    // The poster-first policy's at-rest state (poster-first-gallery-
    // previews ADR): a captured frame shown with no live slot held. Story
    // mode leases no slot and captures nothing, so the poster is injected
    // statically via `static_poster` — a fixed inline PNG, never a
    // rendered capture, keeping the baseline byte-stable. Motion states
    // (hover-to-play, the live canvas reveal) are NOT posable this way:
    // they need a running canvas, which stories must never mount — so
    // only the poster and its badge composition are posed here.
    rsx! {
        section { class: "tw:grid tw:w-[480px] tw:grid-cols-2 tw:gap-3.5 tw:p-4",
            article { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                CardThumb {
                    seed: "prj3fkq8zr21btxyw0a".to_string(),
                    label: "poster".to_string(),
                    static_poster: Some(POSTER_TEST_IMAGE.to_string()),
                }
                p { class: thumb_state_caption_class(), "Poster (at rest)" }
            }
            article { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                CardThumb {
                    seed: "prj9sm2xc44dqnv7bgw".to_string(),
                    label: "poster-failed".to_string(),
                    static_poster: Some(POSTER_TEST_IMAGE.to_string()),
                    static_badge: Some(ThumbPreviewBadge::Error {
                        reason: "deploy: shader compile failed".to_string(),
                    }),
                }
                p { class: thumb_state_caption_class(), "Poster + failure badge" }
            }
        }
    }
}

/// The canned lamp field the control-first thumb story draws: a three-row
/// sign of 72 lamps under a fixed rainbow sweep.
///
/// Deterministic by construction — no clock, no worker, no re-simulation —
/// because these baselines are CI-canonical. The bytes are LINEAR unorm16,
/// which is what the wire carries and what `LampView` decodes; feeding it
/// display-sRGB here would make the story disagree with the real card.
fn thumb_lamp_frame() -> UiControlProductPreview {
    const COLS: u32 = 24;
    const ROWS: u32 = 3;
    const LAMPS: u32 = COLS * ROWS;
    let mut lamps = Vec::with_capacity(LAMPS as usize);
    let mut bytes = Vec::with_capacity(LAMPS as usize * 6);
    for index in 0..LAMPS {
        let (column, row) = (index % COLS, index / COLS);
        lamps.push(ControlLamp2d {
            lamp_index: index,
            sample_start: index * 3,
            center: [
                (column as f32 + 0.5) / COLS as f32,
                (row as f32 + 0.5) / ROWS as f32,
            ],
            radius: 0.02,
        });
        let phase = column as f32 / COLS as f32 + row as f32 * 0.08;
        for channel in 0..3_u32 {
            let turn = (phase + channel as f32 / 3.0) * core::f32::consts::TAU;
            let level = (turn.sin() * 0.5 + 0.5).powi(2);
            bytes.extend_from_slice(&((level * f32::from(u16::MAX)) as u16).to_le_bytes());
        }
    }
    UiControlProductPreview {
        revision: 7,
        extent: ControlExtent::new(1, LAMPS * 3),
        sample_format: UiControlSampleFormat::U16,
        sample_layout: ControlSampleLayout {
            spans: vec![ControlSampleSpan {
                row: 0,
                start: 0,
                len: LAMPS * 3,
                encoding: ControlSampleEncoding::RgbPixels {
                    count: LAMPS,
                    color_order: ColorOrder::Rgb,
                },
            }],
        },
        display_layout: Some(std::rc::Rc::new(ControlDisplayLayout::Layout2d(
            ControlLayout2d::new(Revision::new(7), COLS, ROWS, lamps),
        ))),
        bytes: bytes.into(),
    }
}

/// The live sim card (D36) as the pool evidence produces it: Running with
/// the loaded project's chip, or "nothing loaded".
fn sim_card_fixture(with_project: bool) -> UiSimCard {
    UiSimCard {
        state: if with_project {
            SimCardState::Running
        } else {
            SimCardState::Empty
        },
        project: with_project.then(|| UiSimProjectChip {
            uid: "prj3fKq8Zr21bTxYw0AhVmDpe".to_string(),
            name: "2026-07-02-0930-porch-sign".to_string(),
        }),
        board_id: None,
        console_tail: Vec::new(),
        frame_preview: None,
        frame_age_secs: None,
        frame_fps: None,
        ui: Default::default(),
    }
}

fn gallery(home: UiHomeView) -> Element {
    rsx! {
        section { class: "tw:p-4",
            GalleryPages { home, now_secs: Some(STORY_NOW), on_action: |_| {} }
        }
    }
}

#[story(
    description = "D36: only the sim session lives — the roster leads with the sim card (Running + project chip) and the loaded project's card wears 'Running in simulator'."
)]
fn sim_running_only() -> Element {
    let mut projects = packages();
    projects[0].running_in_sim = true;
    gallery(UiHomeView {
        sim: Some(sim_card_fixture(true)),
        projects,
        examples: examples(),
        devices: Default::default(),
        remembered: Vec::new(),
        library_available: true,
        opening: None,
        issue: None,
    })
}

#[story(
    description = "The sim session with nothing loaded: the card reads 'Connected — nothing loaded' and no project card claims a runtime."
)]
fn sim_with_nothing_loaded() -> Element {
    gallery(UiHomeView {
        sim: Some(sim_card_fixture(false)),
        projects: packages(),
        examples: examples(),
        devices: Default::default(),
        remembered: Vec::new(),
        library_available: true,
        opening: None,
        issue: None,
    })
}

#[story(
    description = "No transport (a browser without Web Serial, or a build without the provider): the Devices page says so rather than showing an empty roster, which would read as \"you have no devices\". The registry rows Studio still remembers are listed underneath."
)]
fn devices_page_without_a_transport() -> Element {
    gallery(UiHomeView {
        sim: None,
        projects: packages(),
        examples: examples(),
        devices: DeviceRosterView::default(),
        remembered: vec![
            "Workbench ESP32".to_string(),
            "Luna's porch sign".to_string(),
        ],
        library_available: true,
        opening: None,
        issue: None,
    })
}

#[story(
    description = "M3 of the device-model rebuild — the roster rendered from the model\'s own projection. A fresh plug is a pending \"identifying…\" entry FIRST (no verdict yet); an identified LightPlayer reads Ready with its freshness line and Disconnect/Forget; a board mid-identify shows its activity with a working Cancel beside Forget; a blank chip shows its honest classification with Setup disabled and a note saying it is coming back."
)]
fn devices_page_roster() -> Element {
    gallery(UiHomeView {
        sim: None,
        projects: packages(),
        examples: examples(),
        devices: roster_fixture(),
        remembered: Vec::new(),
        library_available: true,
        opening: None,
        issue: None,
    })
}

#[story(
    description = "The armed destructive chip, idle beside armed (2K+, devices-treatments spike gate 2026-08-31): first click turns Forget into 'Confirm Forget' — the prefix column opens, red ramps in, and the card previews its own removal (body dimmed and desaturated behind a red inset ring via :has(); the footer keeps full contrast). Blur or the 4s window stands down. Captured with the story-only armed_preview hook; the knock and the quiet drain track are motion and do not capture."
)]
fn devices_page_armed_confirm() -> Element {
    let idle = roster_fixture().roster.devices.remove(0);
    let armed = idle.clone();
    rsx! {
        div { class: "tw:grid tw:max-w-xl tw:grid-cols-2 tw:gap-3 tw:p-4",
            DeviceRosterCard {
                card: idle,
                projects: vec![],
                examples: vec![],
                on_action: |_| {},
            }
            DeviceRosterCard {
                card: armed,
                projects: vec![],
                examples: vec![],
                armed_preview: true,
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "Running vs Degraded, side by side (a fault is never black, 2026-09-02). Left: the healthy running card. Right: the SAME board reporting a faulted node — the chip drops from Ready to Degraded in the attention tone, and one line under \"Running …\" names the node and the runtime's own reason. The running face is deliberately kept: a degraded board is still running, and dropping the project name would answer \"what is on it?\" with a complaint. This is the card that lied for two days while a quarantined shader rendered black (2026-09-01 bench). The degraded card also carries one extra verb in the actions row — Clear faults, beside Reset — which forgets the board\'s crash ledger and re-arms the faulted nodes; the healthy card does not offer it, because there would be nothing for it to do."
)]
fn devices_page_degraded_card() -> Element {
    let healthy = roster_fixture().roster.devices.remove(0);
    let degraded = degraded_card_fixture();
    rsx! {
        div { class: "tw:grid tw:max-w-xl tw:grid-cols-2 tw:gap-3 tw:p-4",
            DeviceRosterCard {
                card: healthy,
                projects: vec![],
                examples: vec![],
                on_action: |_| {},
            }
            DeviceRosterCard {
                card: degraded,
                projects: vec![],
                examples: vec![],
                on_action: |_| {},
            }
        }
    }
}

/// The running card of `roster_fixture`, as it reads once the board reports
/// a faulted node — the bench case, with the ledger's own denial as the
/// runtime's reason.
fn degraded_card_fixture() -> DeviceView {
    let mut card = roster_fixture().roster.devices.remove(0);
    card.status = DeviceStatus::Degraded;
    card.state_label = "Degraded".to_string();
    card.degraded = Some(
        "Degraded: node /studio.show/s faulted — recovery: node 'nodes/meteor' \
         (disabled after 3 crashes)"
            .to_string(),
    );
    card.terminal_lines
        .push("[WARN] recovery: node 'nodes/meteor' disabled after 3 crashes".to_string());
    card
}

/// A roster covering the four states this milestone can reach: a fresh plug
/// still identifying, a settled LightPlayer, one mid-activity, and a blank
/// chip whose only honest verb is round 2\'s.
fn roster_fixture() -> DeviceRosterView {
    DeviceRosterView {
        transport_available: true,
        roster: RosterView {
            pending: vec![
                PendingLinkView {
                    link: DeviceLinkId(3),
                    device: DeviceId(103),
                    title: "Fake ESP32 (usb-3)".to_string(),
                    state_label: "New device found — identifying…".to_string(),
                    detail: Some("chip: esp32c6".to_string()),
                    can_adopt: true,
                    // Mid-identification: no settled verdict, no flash face.
                    needs_firmware: false,
                    detected_chip: Some("esp32c6".to_string()),
                    escapes: vec![DeviceEscape::Forget],
                },
                PendingLinkView {
                    link: DeviceLinkId(4),
                    device: DeviceId(104),
                    title: "Fake ESP32 (usb-4)".to_string(),
                    state_label: "New device found — Blank flash — needs firmware".to_string(),
                    detail: Some("invalid header: 0xffffffff".to_string()),
                    can_adopt: true,
                    // Settled blank: the needs-firmware face (board pick +
                    // Flash) rides this pending card.
                    needs_firmware: true,
                    detected_chip: Some("esp32c6".to_string()),
                    escapes: vec![DeviceEscape::Forget],
                },
            ],
            devices: vec![
                DeviceView {
                    id: DeviceId(1),
                    title: "Luna\'s porch sign".to_string(),
                    status: DeviceStatus::Ready,
                    state_label: "Ready".to_string(),
                    detail: Some("LightPlayer · dig-uno".to_string()),
                    freshness_label: Some("last heard 3 s ago".to_string()),
                    identity_label: Some("dev000000daqf6dvvqz".to_string()),
                    detected_chip: None,
                    board_id: Some("dig-uno".to_string()),
                    needs_firmware: false,
                    degraded: None,
                    // The RUNNING face (M3): what the board itself reports,
                    // named by the storage dir it runs from.
                    loaded_project: DeviceLoadedProject::Running {
                        label: "2026-07-09-1421-porch-sign".to_string(),
                    },
                    can_receive_project: true,
                    // Running, open, idle: the always-actions row offers to
                    // take the project off.
                    can_remove_project: true,
                    activity: None,
                    last_outcome: None,
                    terminal_lines: vec![
                        "ESP-ROM:esp32c6-20220919".to_string(),
                        "[INIT] fw-esp32 initialized, starting server loop".to_string(),
                        "[INIT] loaded /projects/2026-07-09-1421-porch-sign".to_string(),
                    ],
                    escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
                },
                DeviceView {
                    id: DeviceId(2),
                    title: "Workbench ESP32".to_string(),
                    status: DeviceStatus::Busy,
                    state_label: "Identifying…".to_string(),
                    detail: Some("chip: esp32c6".to_string()),
                    freshness_label: Some("last heard just now".to_string()),
                    identity_label: Some("60:55:f9:0a:0b:0c".to_string()),
                    detected_chip: Some("esp32c6".to_string()),
                    board_id: None,
                    needs_firmware: false,
                    degraded: None,
                    loaded_project: DeviceLoadedProject::Unknown,
                    // Busy: one activity per device, so no second verb.
                    can_receive_project: false,
                    can_remove_project: false,
                    activity: Some(DeviceActivityView {
                        kind: DeviceActivityKind::Identify,
                        label: "Identifying…".to_string(),
                        percent: Some(40),
                        cancellable: true,
                        cancel_requested: false,
                    }),
                    last_outcome: None,
                    // Mid-activity: the bar is in the state zone above and
                    // the narration is here, which is the whole point of the
                    // terminal panel.
                    terminal_lines: vec![
                        "— Identifying —".to_string(),
                        "ESP-ROM:esp32c6-20220919".to_string(),
                        "SPIWP:0xee".to_string(),
                        "mode:DIO, clock div:2".to_string(),
                    ],
                    // Cancel FIRST: a running activity\'s way out leads.
                    escapes: vec![
                        DeviceEscape::Cancel,
                        DeviceEscape::Disconnect,
                        DeviceEscape::Forget,
                    ],
                },
                DeviceView {
                    id: DeviceId(3),
                    title: "Shelf light".to_string(),
                    status: DeviceStatus::NeedsAttention,
                    state_label: "Blank flash — needs firmware".to_string(),
                    detail: Some("chip: esp32c6".to_string()),
                    freshness_label: None,
                    identity_label: Some("dev000000000shelf01".to_string()),
                    detected_chip: Some("esp32c6".to_string()),
                    board_id: None,
                    needs_firmware: true,
                    degraded: None,
                    loaded_project: DeviceLoadedProject::Unknown,
                    can_receive_project: false,
                    can_remove_project: false,
                    activity: None,
                    last_outcome: Some(OutcomeView {
                        summary: "identification timed out".to_string(),
                        ok: false,
                    }),
                    // The blank-flash boot loop, which is what "needs
                    // firmware" is actually made of.
                    terminal_lines: vec![
                        "ESP-ROM:esp32c6-20220919".to_string(),
                        "invalid header: 0xffffffff".to_string(),
                        "invalid header: 0xffffffff".to_string(),
                    ],
                    escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
                },
                // The EMPTY face (M3): a LightPlayer that has SAID it has
                // nothing on it, wearing the one inline picker.
                DeviceView {
                    id: DeviceId(4),
                    title: "Seeed XIAO ESP32-C6 · Aug 30".to_string(),
                    status: DeviceStatus::Ready,
                    state_label: "Ready".to_string(),
                    detail: Some("LightPlayer · seeed-xiao-esp32c6".to_string()),
                    freshness_label: Some("last heard just now".to_string()),
                    identity_label: Some("60:55:f9:0a:0b:0d".to_string()),
                    detected_chip: Some("esp32c6".to_string()),
                    board_id: Some("seeed-xiao-esp32c6".to_string()),
                    needs_firmware: false,
                    degraded: None,
                    loaded_project: DeviceLoadedProject::Empty,
                    can_receive_project: true,
                    // Nothing on it to remove — the empty face's picker is
                    // the verb here.
                    can_remove_project: false,
                    activity: None,
                    last_outcome: Some(OutcomeView {
                        summary: "firmware installed — seeed-xiao-esp32c6".to_string(),
                        ok: true,
                    }),
                    // A flash's narration, kept across the reconnect
                    // ladder's reopen — the log the bench had to read in the
                    // browser console.
                    terminal_lines: vec![
                        "— Flashing firmware —".to_string(),
                        "Connecting to the chip".to_string(),
                        "Writing firmware".to_string(),
                        "Waiting for the board to come back (1/5)".to_string(),
                        "ESP-ROM:esp32c6-20220919".to_string(),
                        "[INIT] fw-esp32 initialized, starting server loop".to_string(),
                        "firmware installed — seeed-xiao-esp32c6".to_string(),
                    ],
                    escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
                },
            ],
        },
    }
}

#[story]
fn store_unavailable_with_issue() -> Element {
    let home = UiHomeView {
        sim: None,
        projects: Vec::new(),
        examples: examples(),
        devices: Default::default(),
        remembered: Vec::new(),
        library_available: false,
        opening: None,
        issue: Some(UiIssue::new("Failed to open serial port.")),
    };
    rsx! {
        section { class: "tw:p-4",
            GalleryPages { home, now_secs: Some(STORY_NOW), on_action: |_| {} }
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
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-10",
            DevicesPage { home: home.clone(), on_action }
            ProjectsPage { home: home.clone(), now_secs, on_action }
            ExplorePage { home: Some(home), on_action }
        }
    }
}
