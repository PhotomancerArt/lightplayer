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
    DeviceLoadedProject, DeviceRosterView, DeviceStatus, DeviceTerminalKind, DeviceTerminalLine,
    DeviceView, OutcomeView, PendingLinkView, RosterView,
};

use crate::app::home::card_thumb::CardThumb;
use crate::app::home::device_pick_popover::{
    BoardPickMode, BoardPickPopover, ChipSource, ProjectPickPopover,
};
use crate::app::home::device_roster_card::DeviceRosterCard;
use crate::app::home::device_terminal::DeviceTerminal;
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
        library_available: true,
        opening: None,
        issue: None,
    })
}

#[story(
    description = "No transport (a browser without Web Serial, or a build without the provider): the Devices page says so rather than showing an empty roster, which would read as \"you have no devices\"."
)]
fn devices_page_without_a_transport() -> Element {
    gallery(UiHomeView {
        sim: None,
        projects: packages(),
        examples: examples(),
        devices: DeviceRosterView::default(),
        library_available: true,
        opening: None,
        issue: None,
    })
}

#[story(
    description = "The Devices page under D7 (disconnect → disappear, AC9), with the cards in their four-zone reading (P9): each card is header · PROJECT (preview, the project name or \"Nothing loaded\", its verbs) · FIRMWARE (\"<firmware> · <board>\", Flash firmware … Factory reset, with the terminal flush edge to edge underneath as the same zone's second half) · DEVICE (freshness, Reset · Disconnect … Forget), with no labels anywhere — a zone is known by what it says and what it offers. The pending link wears the same grammar minus the project zone, which it has nothing to fill. The grid holds only boards that are actually THERE — a pending link still identifying, the two connected cards, and the add slot at the insertion point. The board Studio remembers but cannot see is not a card at all: it is the one quiet line under the grid, counted and collapsed, with 'show' as the way in. That line is where Forget lives for an absent board, which is why an unplugged board can still be removed without plugging it back in. Compare with devices_page_remembered_open."
)]
fn devices_page_roster() -> Element {
    devices_page_story(false)
}

#[story(
    description = "The same page with the remembered line expanded (D7, AC9). Each absent board is a dashed, dimmed tile at card width: its name, the 120px preview slot saying WHY there is no picture (not connected, and when it was last heard — never a stale frame passed off as current), the board id · last-seen meta, and the two verbs an absent board can honestly offer — Reconnect in the outline voice (some bridges' port grants do not survive a replug) and Forget as a reserve-width inline confirm. The tiles are deliberately not cards: an offline board has no project, firmware, terminal or device zone to fill, because it has none of those facts to hand."
)]
fn devices_page_remembered_open() -> Element {
    devices_page_story(true)
}

/// The page stories' one body: the roster with a pending link, two
/// connected boards and one remembered board, with the line open or shut.
fn devices_page_story(remembered_open: bool) -> Element {
    let home = UiHomeView {
        sim: None,
        projects: packages(),
        examples: examples(),
        devices: roster_page_fixture(),
        library_available: true,
        opening: None,
        issue: None,
    };
    rsx! {
        section { class: "tw:p-4",
            DevicesPage { home, remembered_open, on_action: |_| {} }
        }
    }
}

#[story(
    description = "The card's four zones and the height rule (AC2), as a measurement: the six device states in 400px columns — running, nothing loaded, needs firmware, flashing at 62%, sending (indeterminate), and degraded. Under the header (title · status chip · board · chip · MAC · firmware) the card is divided by SUBJECT, with no labels: PROJECT (preview slot 120px · info line 17px · bar 4px · verbs 30px) says what is on the board and offers Open · Clear faults … the pick + Put it on the board … Remove; FIRMWARE (info 17px · bar 4px · verbs 30px, then the terminal) reads \"<firmware> · <board>\" or \"Blank flash — needs firmware\", holds Flash firmware … Factory reset, and carries the terminal as its second half — one zone, no hairline between the verb row and the log, and the dark ground running flush to both card edges rather than sitting in a padded well (the pair is one section so a later milestone can put it behind one curtain); DEVICE (info 17px · verbs 30px) carries the freshness line and Reset · Retry · Disconnect … Forget. An activity narrates in the zone whose subject it changes, lights THAT zone's bar and puts its Cancel in THAT zone's verb row: compare Flashing (firmware bar lit, the firmware line counting percent) with Sending (project bar sweeping, the project line narrating). Every row exists in every state, so all six cards MUST measure the same height, and a board event — a heartbeat, a fault, a lost link, a new terminal line — can never move a card nor make the gallery jump while a flash runs. Laid out three rows of two rather than six across so every state fits the captured sheet."
)]
fn devices_card_states() -> Element {
    let states = card_state_fixtures();
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[repeat(2,400px)] tw:items-start tw:gap-3",
                for (label , card , open_uid) in states {
                    div { key: "{label}", class: "tw:grid tw:gap-2",
                        p { class: "tw:m-0 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground",
                            "{label}"
                        }
                        DeviceRosterCard {
                            card,
                            open_uid,
                            // The real gallery lists, so the empty face
                            // shows the pick trigger it actually wears
                            // rather than the "nothing to offer" note.
                            projects: packages(),
                            examples: examples(),
                            on_action: |_| {},
                        }
                    }
                }
            }
        }
    }
}

#[story(
    description = "The quiet state: a board whose port is open and which has stopped saying anything (NotResponding). It is deliberately undramatic — the chip reads Not responding in the neutral tone, and the DEVICE zone's info line carries the honest staleness (\"last heard 4 min ago\") rather than an invented failure, with the way out beside it: Reset · Retry (re-run identification, no replug needed) · Disconnect … Forget. Nothing is claimed about what is loaded: the board has not said, so the PROJECT zone's info line stays empty at its height and the preview slot says the feed has nothing to show. The FIRMWARE zone still names the firmware and board the record remembers — going quiet does not unlearn what the board already said."
)]
fn devices_card_not_responding() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:w-[400px]",
                DeviceRosterCard {
                    card: not_responding_card_fixture(),
                    projects: packages(),
                    examples: examples(),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The armed destructive chips, idle beside both armed states (2K+, devices-treatments spike gate 2026-08-31; RESERVE width from the device-card-v2 spike §2, 2026-09-02). The chip renders both 'Forget' and 'Confirm Forget' in one grid cell, so it is already as wide as its armed reading and the first click changes text and tone WITHOUT moving the chip or its neighbours — compare the footers, the chips sit at the same width and the card's height is unchanged. Middle: Forget armed in the DEVICE zone. Right: Remove armed in the PROJECT zone's verb row — D8, the OTHER destructive chip, which marks the whole card exactly as Forget does, and the two now sit in different zones, which is why a capture that proves the marking needs both. Arming dims what the card SAYS (the header and every zone's info line, the preview and the terminal) and never what it OFFERS: every verb row keeps full contrast, so the chip that is asking stays legible. Blur or the 4s window stands down. Captured with the story-only armed_preview hooks; the knock and the quiet drain track are motion and do not capture."
)]
fn devices_card_armed() -> Element {
    let card = armed_card_fixture();
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[repeat(3,340px)] tw:items-start tw:gap-3",
                DeviceRosterCard {
                    card: card.clone(),
                    projects: vec![],
                    examples: vec![],
                    on_action: |_| {},
                }
                DeviceRosterCard {
                    card: card.clone(),
                    projects: vec![],
                    examples: vec![],
                    armed_preview: true,
                    on_action: |_| {},
                }
                DeviceRosterCard {
                    card,
                    projects: vec![],
                    examples: vec![],
                    armed_remove_preview: true,
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "Running vs Degraded, side by side (a fault is never black, 2026-09-02). Left: the healthy running card. Right: the SAME board reporting a faulted node — the chip drops from Ready to Degraded in the attention tone, and the PROJECT zone's info line takes the attention tone to name the node and the runtime's own reason, in the row that otherwise holds the project name (one line, the full text on hover). The running face is deliberately kept: a degraded board is still running, which is why Open stays and the fault reads as a line rather than as a new state. This is the card that lied for two days while a quarantined shader rendered black (2026-09-01 bench). The degraded card also carries one extra verb, in the same zone as the fault it answers — Clear faults, beside Open — which forgets the board's crash ledger and re-arms the faulted nodes; the healthy card does not offer it, because there would be nothing for it to do."
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
    card.terminal.push(DeviceTerminalLine {
        kind: DeviceTerminalKind::Recovery,
        text: "[WARN] recovery: node 'nodes/meteor' disabled after 3 crashes".to_string(),
        repeats: 1,
    });
    card
}

/// A roster covering the four states this milestone can reach: a fresh plug
/// still identifying, a settled LightPlayer, one mid-activity, and a blank
/// chip whose only honest verb is round 2\'s.
fn roster_fixture() -> DeviceRosterView {
    DeviceRosterView {
        transport_available: true,
        // The running card has earned a registry row, so it has an editor
        // address and the running face wears Open (round-2 M5).
        open_addresses: [(1, "dev000000daqf6dvvqz".to_string())]
            .into_iter()
            .collect(),
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
                    firmware_face: lpa_studio_core::DeviceFirmwareFace::Unknown,
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
                    firmware_face: lpa_studio_core::DeviceFirmwareFace::Blank,
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
                    detail: Some("LightPlayer · quinled/dig-uno".to_string()),
                    freshness_label: Some("last heard 3 s ago".to_string()),
                    identity_label: Some("dev000000daqf6dvvqz".to_string()),
                    detected_chip: Some("esp32".to_string()),
                    board_id: Some("quinled/dig-uno".to_string()),
                    firmware_face: lpa_studio_core::DeviceFirmwareFace::LightPlayer {
                        firmware: Some("fw-esp32v3 abc1234".to_string()),
                        wire: lpa_studio_core::DeviceWireVersion::Match,
                    },
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
                    // What a running board actually says, typed by the fold
                    // (P1): the ROM banner it booted with, its own init
                    // lines, the decoded wire summaries — including the
                    // heartbeat run collapsed to ×4, which is what keeps a
                    // healthy board from scrolling its own terminal away.
                    terminal: vec![
                        story_line(DeviceTerminalKind::Rom, "ESP-ROM:esp32c6-20220919"),
                        story_line(
                            DeviceTerminalKind::Board,
                            "[INIT] fw-esp32 initialized, starting server loop",
                        ),
                        story_line(
                            DeviceTerminalKind::Board,
                            "[INIT] loaded /projects/2026-07-09-1421-porch-sign",
                        ),
                        story_line(
                            DeviceTerminalKind::Wire,
                            "hello · proto 1 · quinled/dig-uno · fw-esp32v3 abc1234",
                        ),
                        story_line(DeviceTerminalKind::Studio, "Opened the port"),
                        story_line(DeviceTerminalKind::Outcome, "Identified in 0.4 s"),
                        story_repeat(
                            DeviceTerminalKind::Wire,
                            "heartbeat · 43 fps · heap 108 KB · porch-sign",
                            4,
                        ),
                    ],
                    terminal_dropped: 0,
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
                    // A board Studio has met before: the record kept its
                    // board id and firmware label, so the identity line
                    // still names them while the re-identify runs.
                    board_id: Some("seeed/xiao-esp32-c6".to_string()),
                    firmware_face: lpa_studio_core::DeviceFirmwareFace::LightPlayer {
                        firmware: Some("fw-esp32c6 abc1234".to_string()),
                        wire: lpa_studio_core::DeviceWireVersion::Match,
                    },
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
                    terminal: vec![
                        story_line(DeviceTerminalKind::Studio, "Identifying the board"),
                        story_line(DeviceTerminalKind::Rom, "ESP-ROM:esp32c6-20220919"),
                        story_line(DeviceTerminalKind::Rom, "SPIWP:0xee"),
                        story_line(DeviceTerminalKind::Rom, "mode:DIO, clock div:2"),
                    ],
                    terminal_dropped: 0,
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
                    firmware_face: lpa_studio_core::DeviceFirmwareFace::Blank,
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
                    // The blank-flash boot loop is a REPEAT, and the fold
                    // collapses it: four identical header complaints are
                    // one line with a ×4 badge, not four lines that push
                    // the ROM banner out of the panel.
                    terminal: vec![
                        story_line(DeviceTerminalKind::Rom, "ESP-ROM:esp32c6-20220919"),
                        story_repeat(DeviceTerminalKind::Rom, "invalid header: 0xffffffff", 4),
                        story_line(
                            DeviceTerminalKind::Studio,
                            "Blank flash — the chip named itself in the boot banner",
                        ),
                        story_line(
                            DeviceTerminalKind::Failure,
                            "identification timed out — nothing answered the hello",
                        ),
                    ],
                    terminal_dropped: 0,
                    escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
                },
                // The EMPTY face (M3): a LightPlayer that has SAID it has
                // nothing on it, wearing the one inline picker.
                DeviceView {
                    id: DeviceId(4),
                    title: "Seeed XIAO ESP32-C6 · Aug 30".to_string(),
                    status: DeviceStatus::Ready,
                    state_label: "Ready".to_string(),
                    detail: Some("LightPlayer · seeed/xiao-esp32-c6".to_string()),
                    freshness_label: Some("last heard just now".to_string()),
                    identity_label: Some("60:55:f9:0a:0b:0d".to_string()),
                    detected_chip: Some("esp32c6".to_string()),
                    board_id: Some("seeed/xiao-esp32-c6".to_string()),
                    firmware_face: lpa_studio_core::DeviceFirmwareFace::LightPlayer {
                        firmware: Some("fw-esp32c6 abc1234".to_string()),
                        wire: lpa_studio_core::DeviceWireVersion::Match,
                    },
                    degraded: None,
                    loaded_project: DeviceLoadedProject::Empty,
                    can_receive_project: true,
                    // Nothing on it to remove — the empty face's picker is
                    // the verb here.
                    can_remove_project: false,
                    activity: None,
                    last_outcome: Some(OutcomeView {
                        summary: "firmware installed — seeed/xiao-esp32-c6".to_string(),
                        ok: true,
                    }),
                    // A flash's narration, kept across the reconnect
                    // ladder's reopen — the log the bench had to read in the
                    // browser console.
                    terminal: vec![
                        story_line(DeviceTerminalKind::Studio, "Flashing firmware"),
                        story_line(DeviceTerminalKind::Studio, "Connecting to the chip"),
                        story_line(DeviceTerminalKind::Studio, "Writing firmware"),
                        story_line(
                            DeviceTerminalKind::Studio,
                            "Waiting for the board to come back (1/5)",
                        ),
                        story_line(DeviceTerminalKind::Rom, "ESP-ROM:esp32c6-20220919"),
                        story_line(
                            DeviceTerminalKind::Board,
                            "[INIT] fw-esp32 initialized, starting server loop",
                        ),
                        story_line(
                            DeviceTerminalKind::Wire,
                            "hello · proto 1 · seeed/xiao-esp32-c6 · fw-esp32c6 abc1234",
                        ),
                        story_line(DeviceTerminalKind::Wire, "loaded · 0 projects"),
                        story_line(
                            DeviceTerminalKind::Outcome,
                            "firmware installed — seeed/xiao-esp32-c6",
                        ),
                    ],
                    terminal_dropped: 0,
                    escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
                },
                // The remembered board (D7): known, named, and not on the
                // bus — the roster still projects it, and the page splits
                // it out of the grid into the quiet line underneath.
                DeviceView {
                    id: DeviceId(5),
                    title: "Garage strip".to_string(),
                    status: DeviceStatus::Offline,
                    state_label: "Not connected".to_string(),
                    detail: None,
                    freshness_label: Some("last heard 6 min ago".to_string()),
                    identity_label: Some("dev000000000garage1".to_string()),
                    detected_chip: Some("esp32c6".to_string()),
                    board_id: Some("seeed/xiao-esp32-c6".to_string()),
                    firmware_face: lpa_studio_core::DeviceFirmwareFace::LightPlayer {
                        firmware: Some("fw-esp32c6 abc1234".to_string()),
                        wire: lpa_studio_core::DeviceWireVersion::Match,
                    },
                    degraded: None,
                    loaded_project: DeviceLoadedProject::Unknown,
                    can_receive_project: false,
                    can_remove_project: false,
                    activity: None,
                    last_outcome: None,
                    // Nothing live to show: the link is gone, so the
                    // terminal has nothing to say and the tile draws none.
                    terminal: Vec::new(),
                    terminal_dropped: 0,
                    // The two verbs an absent board can honestly offer.
                    escapes: vec![DeviceEscape::Reconnect, DeviceEscape::Forget],
                },
            ],
        },
    }
}

/// One typed terminal line, as the fold hands it over (P1).
fn story_line(kind: DeviceTerminalKind, text: &str) -> DeviceTerminalLine {
    DeviceTerminalLine {
        kind,
        text: text.to_string(),
        repeats: 1,
    }
}

/// A line the fold COLLAPSED: `repeats` consecutive identical arrivals
/// shown once with a ×N badge (a blank board's header complaint, a healthy
/// board's heartbeat).
fn story_repeat(kind: DeviceTerminalKind, text: &str, repeats: u32) -> DeviceTerminalLine {
    DeviceTerminalLine {
        kind,
        text: text.to_string(),
        repeats,
    }
}

/// The page stories' roster: what the grid should hold under D7 — one
/// pending link, two connected boards (one running, one empty), and one
/// board Studio only remembers.
///
/// Cut from [`roster_fixture`] rather than written again, so the cards in
/// the page stories are the same cards the state stories measure.
fn roster_page_fixture() -> DeviceRosterView {
    let full = roster_fixture();
    let mut devices = full.roster.devices;
    // 0 = running · 3 = empty · 4 = the remembered board.
    let remembered = devices.remove(4);
    let empty = devices.remove(3);
    let running = devices.remove(0);
    DeviceRosterView {
        transport_available: true,
        open_addresses: full.open_addresses,
        roster: RosterView {
            // The blank board's link, the one a fresh plug actually looks
            // like: settled at needs-firmware, wearing the board pick.
            pending: vec![full.roster.pending[1].clone()],
            devices: vec![running, empty, remembered],
        },
    }
}

/// The six states of `devices_card_states`, labelled, with the editor
/// address the running faces need for Open.
///
/// Flashing and Sending are the SAME board mid-activity — the point of the
/// story is that an activity changes what the rows say and never how tall
/// they are.
#[story(
    description = "One card per FIRMWARE FACE — the sheet that did not exist when an older board shipped drawn as a blank chip (bench 2026-09-04: a proto-19 classic on a proto-20 Studio read \"Blank flash — needs firmware\" and \"no firmware\" while its terminal decoded the hello naming fw-esp32v3 and a heartbeat carrying a red fault). Seven cards in 400px columns, each in ITS OWN words, decided in core and tested per variant. Two VERBS for two situations (ruled 2026-09-04): a running LightPlayer offers UPDATE FIRMWARE, matching its line's \"update recommended\"; a needs-firmware face offers FLASH FIRMWARE with the board pick, since nothing is known. OLDER (a running LightPlayer one wire version behind — still Ready, the project and its fault still on the project line, the firmware line reading \"<firmware> · <board> — older than Studio, update recommended\", and Update firmware as ONE click because the registry knows the board: offered, never forced — warn, then proceed); OLDER, BOARD UNKNOWN (the bench classic verbatim: its hello says `?` because the board id comes from the manifest Studio stamps at flash and this board was flashed from the CLI, the registry has no board either, and a classic chip fits three boards — so the SAME Update verb opens the pick once, and the panel says why); NEWER (the same the other way, no recommendation); PRE-HELLO (speaks the framing, never said hello); FOREIGN (a recognised factory firmware, named); BOOTLOADER (parked in ROM download mode); SILENT (open port, nothing heard, Retry beside Reset). The chip is the STATUS, unchanged by the wire version; the face's sentence lives in the Firmware zone — and every card measures the same height (AC2)."
)]
fn devices_card_firmware_faces() -> Element {
    let faces = firmware_face_fixtures();
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[repeat(2,400px)] tw:items-start tw:gap-3",
                for (label , card , open_uid) in faces {
                    div { key: "{label}", class: "tw:grid tw:gap-2",
                        p { class: "tw:m-0 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground",
                            "{label}"
                        }
                        DeviceRosterCard {
                            card,
                            open_uid,
                            projects: packages(),
                            examples: examples(),
                            on_action: |_| {},
                        }
                    }
                }
            }
        }
    }
}

/// The firmware faces a settled board can wear besides the current
/// LightPlayer (which the states sheet already covers) — the older face
/// twice, once with its board known and once without, because the verb
/// row differs (one click vs. the pick once).
fn firmware_face_fixtures() -> Vec<(&'static str, DeviceView, Option<String>)> {
    use lpa_studio_core::{DeviceFirmwareFace, DeviceWireVersion};

    let running = roster_fixture().roster.devices.remove(0);
    let open_uid = Some("dev000000daqf6dvvqz".to_string());

    // An older classic, running, with a fault — and REGISTERED as a
    // Dig-Uno, so Update firmware is one click.
    let older = DeviceView {
        id: DeviceId(31),
        title: "Shop classic · Sep 4".to_string(),
        status: DeviceStatus::Degraded,
        state_label: "Degraded".to_string(),
        detail: Some("LightPlayer · fw-esp32v3 7c80a27".to_string()),
        identity_label: Some("30:76:f5:ec:f6:34".to_string()),
        detected_chip: Some("esp32".to_string()),
        board_id: Some("quinled/dig-uno".to_string()),
        firmware_face: DeviceFirmwareFace::LightPlayer {
            firmware: Some("fw-esp32v3 7c80a27".to_string()),
            wire: DeviceWireVersion::BoardOlder {
                board: 19,
                studio: 20,
            },
        },
        degraded: Some("Recovery red: /studio.show/s disabled after repeated crashes".to_string()),
        loaded_project: DeviceLoadedProject::Running {
            label: "studio".to_string(),
        },
        terminal: vec![
            story_line(
                DeviceTerminalKind::Wire,
                "hello · proto 19 · quinled/dig-uno · fw-esp32v3 7c80a27 (dirty)",
            ),
            story_line(
                DeviceTerminalKind::Studio,
                "firmware speaks wire proto 19, Studio speaks 20 — older firmware, proceeding anyway",
            ),
            story_line(
                DeviceTerminalKind::Outcome,
                "fw-esp32v3 7c80a27 (older firmware than Studio)",
            ),
            story_repeat(
                DeviceTerminalKind::Wire,
                "heartbeat · studio · FAULT red",
                12,
            ),
        ],
        ..running.clone()
    };
    // The bench case, verbatim: the same older classic, but its hello
    // reports board `?` (flashed from the CLI, so no stamped manifest) and
    // the registry has no board either. A classic chip fits three boards,
    // so the SAME Update verb opens the pick once — and says why.
    let older_unknown = DeviceView {
        id: DeviceId(37),
        title: "Bench classic · Sep 4".to_string(),
        board_id: None,
        terminal: vec![
            story_line(
                DeviceTerminalKind::Wire,
                "hello · proto 19 · ? · fw-esp32v3 7c80a27 (dirty)",
            ),
            story_line(
                DeviceTerminalKind::Studio,
                "firmware speaks wire proto 19, Studio speaks 20 — older firmware, proceeding anyway",
            ),
            story_line(
                DeviceTerminalKind::Outcome,
                "fw-esp32v3 7c80a27 (older firmware than Studio)",
            ),
            story_repeat(
                DeviceTerminalKind::Wire,
                "heartbeat · studio · FAULT red",
                12,
            ),
        ],
        ..older.clone()
    };
    let newer = DeviceView {
        id: DeviceId(32),
        title: "Dev board · Sep 4".to_string(),
        status: DeviceStatus::Ready,
        state_label: "Ready".to_string(),
        degraded: None,
        firmware_face: DeviceFirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 e1f2a3b".to_string()),
            wire: DeviceWireVersion::BoardNewer {
                board: 21,
                studio: 20,
            },
        },
        terminal: vec![
            story_line(
                DeviceTerminalKind::Wire,
                "hello · proto 21 · seeed/xiao-esp32-c6 · fw-esp32c6 e1f2a3b",
            ),
            story_line(
                DeviceTerminalKind::Studio,
                "firmware speaks wire proto 21, Studio speaks 20 — newer firmware, proceeding anyway",
            ),
            story_repeat(DeviceTerminalKind::Wire, "heartbeat · porch-sign", 6),
        ],
        ..running.clone()
    };

    // The four verdicts that ask for a flash, each on a card that has
    // nothing else to say: no project, no picture, the face's own line.
    let attention = |id: u64, title: &str, state: &str, face: DeviceFirmwareFace| DeviceView {
        id: DeviceId(id),
        title: title.to_string(),
        status: DeviceStatus::NeedsAttention,
        state_label: state.to_string(),
        detail: None,
        freshness_label: Some("last heard 2 s ago".to_string()),
        identity_label: None,
        detected_chip: Some("esp32c6".to_string()),
        board_id: None,
        firmware_face: face,
        degraded: None,
        loaded_project: DeviceLoadedProject::Unknown,
        can_receive_project: false,
        can_remove_project: false,
        activity: None,
        last_outcome: None,
        terminal: vec![story_line(
            DeviceTerminalKind::Rom,
            "ESP-ROM:esp32c6-20220919",
        )],
        terminal_dropped: 0,
        escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
    };
    let pre_hello = DeviceView {
        terminal: vec![
            story_line(
                DeviceTerminalKind::Board,
                "[INIT] fw-esp32 initialized, starting server loop",
            ),
            story_repeat(DeviceTerminalKind::Wire, "UnloadProject", 3),
            story_line(
                DeviceTerminalKind::Outcome,
                "speaks the framing but never said hello (pre-hello firmware)",
            ),
        ],
        ..attention(
            33,
            "Old lamp",
            "No LightPlayer hello — pre-hello firmware",
            DeviceFirmwareFace::NoHello,
        )
    };
    let foreign = DeviceView {
        terminal: vec![
            story_line(DeviceTerminalKind::Rom, "ESP-ROM:esp32c6-20220919"),
            story_line(
                DeviceTerminalKind::Board,
                "Hello from Seeed Studio XIAO ESP32-C6",
            ),
            story_line(DeviceTerminalKind::Outcome, "Seeed XIAO factory firmware"),
        ],
        ..attention(
            34,
            "New XIAO",
            "Running Seeed XIAO factory firmware",
            DeviceFirmwareFace::Foreign {
                label: Some("Seeed XIAO factory firmware".to_string()),
            },
        )
    };
    let bootloader = DeviceView {
        terminal: vec![
            story_line(DeviceTerminalKind::Rom, "ESP-ROM:esp32c6-20220919"),
            story_line(DeviceTerminalKind::Rom, "waiting for download"),
            story_line(DeviceTerminalKind::Outcome, "waiting in ROM download mode"),
        ],
        ..attention(
            35,
            "Parked board",
            "Waiting in ROM download mode",
            DeviceFirmwareFace::Bootloader,
        )
    };
    let silent = DeviceView {
        status: DeviceStatus::NotResponding,
        freshness_label: None,
        terminal: Vec::new(),
        escapes: vec![
            DeviceEscape::Retry,
            DeviceEscape::Disconnect,
            DeviceEscape::Forget,
        ],
        ..attention(
            36,
            "Quiet board",
            "Not responding",
            DeviceFirmwareFace::Silent,
        )
    };

    vec![
        ("Older than Studio", older, open_uid.clone()),
        ("Older, board unknown", older_unknown, open_uid.clone()),
        ("Newer than Studio", newer, open_uid),
        ("Pre-hello firmware", pre_hello, None),
        ("Foreign firmware", foreign, None),
        ("Bootloader", bootloader, None),
        ("Silent", silent, None),
    ]
}

fn card_state_fixtures() -> Vec<(&'static str, DeviceView, Option<String>)> {
    let running = roster_fixture().roster.devices.remove(0);
    let open_uid = Some("dev000000daqf6dvvqz".to_string());

    let flashing = DeviceView {
        status: DeviceStatus::Busy,
        state_label: "Flashing firmware".to_string(),
        activity: Some(DeviceActivityView {
            kind: DeviceActivityKind::Flash,
            label: "Flashing firmware".to_string(),
            percent: Some(62),
            cancellable: true,
            cancel_requested: false,
        }),
        can_remove_project: false,
        escapes: vec![
            DeviceEscape::Cancel,
            DeviceEscape::Disconnect,
            DeviceEscape::Forget,
        ],
        ..running.clone()
    };
    let sending = DeviceView {
        status: DeviceStatus::Busy,
        state_label: "Sending the project".to_string(),
        activity: Some(DeviceActivityView {
            kind: DeviceActivityKind::Push,
            label: "Sending the project".to_string(),
            // No percentage: the push knows its file count, not its bytes,
            // so the slot sweeps rather than lying about progress.
            percent: None,
            cancellable: true,
            cancel_requested: false,
        }),
        can_remove_project: false,
        escapes: vec![
            DeviceEscape::Cancel,
            DeviceEscape::Disconnect,
            DeviceEscape::Forget,
        ],
        ..running.clone()
    };

    vec![
        ("Running", running, open_uid.clone()),
        (
            "Nothing loaded",
            roster_fixture().roster.devices.remove(3),
            None,
        ),
        (
            "Needs firmware",
            roster_fixture().roster.devices.remove(2),
            None,
        ),
        ("Flashing · 62%", flashing, open_uid.clone()),
        ("Sending", sending, open_uid.clone()),
        ("Degraded", degraded_card_fixture(), open_uid),
    ]
}

/// A board whose port is open and which has stopped answering: the quiet
/// state. Retry re-runs identification without a replug, which is the whole
/// reason the projection grants it here and nowhere else.
fn not_responding_card_fixture() -> DeviceView {
    let mut card = roster_fixture().roster.devices.remove(0);
    card.status = DeviceStatus::NotResponding;
    card.state_label = "Not responding".to_string();
    card.freshness_label = Some("last heard 4 min ago".to_string());
    // It has not said what is on it since it went quiet, so the card says
    // nothing about a project either.
    card.loaded_project = DeviceLoadedProject::Unknown;
    card.can_remove_project = false;
    card.escapes = vec![
        DeviceEscape::Retry,
        DeviceEscape::Disconnect,
        DeviceEscape::Forget,
    ];
    card.terminal.push(story_line(
        DeviceTerminalKind::Failure,
        "no heartbeat for 4 min — the port is open and the board is silent",
    ));
    card
}

/// The card the armed story shows three times: a running board carrying
/// BOTH destructive chips — Remove project in the verb row, Forget in the
/// footer.
fn armed_card_fixture() -> DeviceView {
    roster_fixture().roster.devices.remove(0)
}

#[story]
fn store_unavailable_with_issue() -> Element {
    let home = UiHomeView {
        sim: None,
        projects: Vec::new(),
        examples: examples(),
        devices: Default::default(),
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

/// P5 (device-card-v2 plan): the terminal renderer alone, fed the shape
/// [`Evidence::fold`] actually produces — a capped, typed, repeat-collapsed
/// tail plus a drop count — rather than raw board chatter. `TERMINAL_CAP`
/// in `lpa-devices` is 200; this fixture stands in for 250 raw lines
/// having arrived (200 kept, 50 dropped), mixing every
/// [`DeviceTerminalKind`], a single ×6 repeat, several decoded wire
/// summaries and one 400-character line to exercise the fold control.
#[story(
    description = "The terminal renderer alone (P5): natural oldest-first order, typed colours, a ×6 repeat badge, a 400-char line folded to 120 chars + a click-to-expand control, wire rows tagged and coloured live-blue, and the dropped-lines notice for the 50 lines the 200-line cap pushed out. Pinned to the bottom on load."
)]
fn device_terminal_processed() -> Element {
    rsx! {
        section { class: "tw:p-4",
            article { class: "ux-armed-scope tw:flex tw:w-[340px] tw:flex-col tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                header { class: "tw:grid tw:min-w-0 tw:gap-1.5 tw:px-4 tw:pt-4 tw:pb-3",
                    h3 { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground",
                        "Terminal — processed tail"
                    }
                }
                DeviceTerminal {
                    lines: device_terminal_story_lines(),
                    dropped: 50,
                    height_class: "tw:h-40",
                }
            }
        }
    }
}

/// A 400-character line — long enough to trip the fold at 160 and show a
/// realistic "the panel used to just eat this" block-plan dump.
fn device_terminal_story_long_line() -> String {
    let prefix = "Esp32C6RmtWs281xDriver: block plan published: outputs=[{pin:2,px:300,fmt:grb},{pin:3,px:300,fmt:grb}] clock=pll_f80m/1 lut=gamma-2.2-8bit dither=temporal-4 frame_us=23100 margin_words=3 — this is the kind of line that used to eat the panel — ";
    let filler_len = 400_usize.saturating_sub(prefix.chars().count());
    format!("{prefix}{}", "…".repeat(filler_len))
}

/// 200 typed lines (the model's `TERMINAL_CAP`) mixing every
/// [`DeviceTerminalKind`], the ×6 repeat, the 400-char fold line, and a
/// run of decoded wire heartbeats.
fn device_terminal_story_lines() -> Vec<DeviceTerminalLine> {
    let mut lines = vec![
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Rom,
            text: "ESP-ROM:esp32c6-20220919".to_string(),
            repeats: 1,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Rom,
            text: "Build:Sep 19 2022".to_string(),
            repeats: 1,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Rom,
            text: "rst:0x1 (POWERON),boot:0x2c (SPI_FAST_FLASH_BOOT)".to_string(),
            repeats: 1,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Board,
            text: "[INIT] fw-esp32 initialized, starting server loop".to_string(),
            repeats: 1,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Wire,
            text: "hello · proto 1 · seeed/xiao-esp32-c6 · fw 2026.09.01".to_string(),
            repeats: 1,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Studio,
            text: "Sending meteor · 14 files · 38 KB".to_string(),
            repeats: 1,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Wire,
            text: "ack · /projects/meteor/project.json".to_string(),
            repeats: 1,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Outcome,
            text: "Sent Meteor in 2.1 s".to_string(),
            repeats: 1,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Board,
            text: "Boot: auto-loaded project meteor".to_string(),
            repeats: 1,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Board,
            text: device_terminal_story_long_line(),
            repeats: 1,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Recovery,
            text: "Esp32OutputProvider::flush: handle=2: RMT channel busy (retrying)".to_string(),
            repeats: 6,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Recovery,
            text: "[RECOVERY] node /studio.show/s: crash 2/2 (OOM at compute compile, 250 B short of 300000 B)"
                .to_string(),
            repeats: 1,
        },
        DeviceTerminalLine {
            kind: DeviceTerminalKind::Failure,
            text: "[RECOVERY] node /studio.show/s disabled after 2 crashes — black fallback → fault pattern"
                .to_string(),
            repeats: 1,
        },
    ];

    // Fill the rest of the 200-line cap with decoded wire heartbeats, the
    // bulk of what a running board actually says.
    let heartbeats_needed = 200_usize.saturating_sub(lines.len());
    for index in 0..heartbeats_needed {
        lines.push(DeviceTerminalLine {
            kind: DeviceTerminalKind::Wire,
            text: format!(
                "heartbeat · 43 fps · heap {} KB · meteor · up {} s",
                110 - (index % 8),
                41 + index * 5
            ),
            repeats: 1,
        });
    }

    lines
}

/// The empty face's card, as a board with a real catalog id and a library
/// big enough to have made the old inline picker taller than the card
/// (P6's whole reason for existing).
fn pick_popover_card() -> DeviceView {
    let mut card = roster_fixture().roster.devices.remove(3);
    // A catalogued board id, so the New tab has its starter card to show
    // rather than the "can't tell which board this is" reason.
    card.board_id = Some("seeed/xiao-esp32-c6".to_string());
    card
}

/// Forty saved projects: the library size the inline picker could not hold.
fn pick_popover_library() -> Vec<UiPackageCard> {
    let names = [
        "porch-sign",
        "meteor",
        "shelf-glow",
        "kitchen-strip",
        "dome-test",
        "logo-sign",
        "aurora",
        "candle",
        "spiral",
        "rainfall",
    ];
    (0..40)
        .map(|index| UiPackageCard {
            uid: format!("prj{index:022}"),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: format!(
                "2026-08-{:02}-{:04}-{}",
                (index % 28) + 1,
                900 + index * 7,
                names[index as usize % names.len()],
            ),
            last_saved_at: Some(STORY_NOW - f64::from(index) * 3600.0),
            provenance: None,
            on_device: None,
            open_elsewhere: false,
            running_in_sim: false,
            target: None,
            health: PackageHealth::Ready,
        })
        .collect()
}

/// Six bundled examples, so the Examples tab is a grid rather than a row.
fn pick_popover_examples() -> Vec<UiExampleCard> {
    [
        "Basic",
        "Meteor",
        "Plasma",
        "Rainbow",
        "Logo sign",
        "Candle",
    ]
    .into_iter()
    .map(|name| UiExampleCard {
        id: format!("examples/{}", name.to_lowercase().replace(' ', "-")),
        name: name.to_string(),
        kind: "Module".to_string(),
    })
    .collect()
}

#[story(
    description = "The gallery pick popover, open (P6, AC8). The card's verb row holds ONE 30px control — the trigger — and the options live in a panel in the browser's top layer, so a library of forty projects can no longer make the card taller than the viewport (the reflow rule, AC2). Tabs are the three sources core's push_offer already groups, with their counts; the search box filters titles client-side; the cards are the gallery's own thumbs with their provenance, and a picked one wears the app-wide selection grammar (spectrum ring + wash + check). Picking closes the panel and updates the trigger — nothing is journaled until the CTA beside it dispatches the Push."
)]
fn device_pick_popover_open() -> Element {
    rsx! {
        section { class: "tw:min-h-[520px] tw:w-[420px] tw:p-4",
            div { class: "tw:flex tw:h-[30px] tw:min-w-0 tw:items-center tw:gap-1.5 tw:overflow-hidden tw:whitespace-nowrap",
                ProjectPickPopover {
                    card: pick_popover_card(),
                    projects: pick_popover_library(),
                    examples: pick_popover_examples(),
                    initially_open: true,
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The board pick popover, open and filtered (P6, AC4; renderings P10). The chip the boot banner named narrows the served catalog, and the panel SAYS so — which chip, which source answered it, how many boards fit — with \"show all\" as the escape; the flash preflight's chip guard, not the filter, is what makes a wrong pick fail safely, which is what the foot line is for. Each tile now LEADS with the board as lpa-boards draws it — the same sidecar and the same renderer the boards page uses, turned a quarter turn and fitted to a 56px band, so a devkit lies along the band instead of standing in it as a sliver and tiles of a three-to-one height range still line their names up — over the name, its manufacturer and flash, and its family, marked green only where it matches the detected chip. The trigger's swatch carries the picked board's own silhouette. Two C6 boards fit, so nothing is preselected and the Flash verb waits: the pin map is written to the device, so the card never guesses."
)]
fn device_board_pick_open() -> Element {
    board_pick_story(BoardPickMode::Row, ("esp32c6", ChipSource::BootBanner))
}

#[story(
    description = "Update firmware's pick, open (ruled 2026-09-04). A running LightPlayer wears UPDATE FIRMWARE, and when its board is known that is one click. This is the other case — the bench classic: its hello reports board `?` (the board id comes from the manifest Studio stamps at flash, and this board was flashed from the CLI), the registry has no board, and a classic ESP32 chip fits three served boards — so the SAME quiet chip is the picker's trigger, and the panel earns the detour with one line under its filter: \"This board hasn't said which board it is. Pick once; Studio stamps it at flash, and next time this is one click.\" Picking a board flashes it straight away: the verb was already pressed. The verb, the reason, and whether a pick is needed at all are decided in core (`firmware_verb`) and tested there; this panel only draws them."
)]
fn device_update_pick_open() -> Element {
    board_pick_story(BoardPickMode::Verb, ("esp32", ChipSource::BootBanner))
}

/// One 420px column with the board pick popover mounted open.
fn board_pick_story(mode: BoardPickMode, chip: (&str, ChipSource)) -> Element {
    rsx! {
        section { class: "tw:min-h-[420px] tw:w-[420px] tw:p-4",
            div { class: "tw:flex tw:h-[30px] tw:min-w-0 tw:items-center tw:gap-1.5 tw:overflow-hidden tw:whitespace-nowrap",
                BoardPickPopover {
                    device: DeviceId(3),
                    chip: Some((chip.0.to_string(), chip.1)),
                    mode,
                    initially_open: true,
                    on_action: |_| {},
                }
            }
        }
    }
}
