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
        blurb: "A single strip, the smallest complete project".to_string(),
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
        remembered: Vec::new(),
        library_available: true,
        opening: None,
        issue: None,
    })
}

#[story(
    description = "Device support being rebuilt (M2 of the device-model rebuild): with no sim session running, the Devices page is the honest stub alone — no roster, no creation cards, no dead buttons."
)]
fn devices_page_stub() -> Element {
    gallery(UiHomeView {
        sim: None,
        projects: packages(),
        examples: examples(),
        // the registry survived: the stub names what Studio still holds
        remembered: vec![
            "Workbench ESP32".to_string(),
            "Luna's porch sign".to_string(),
        ],
        library_available: true,
        opening: None,
        issue: None,
    })
}

#[story]
fn store_unavailable_with_issue() -> Element {
    let home = UiHomeView {
        sim: None,
        projects: Vec::new(),
        examples: examples(),
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
