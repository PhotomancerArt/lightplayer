//! Home page stories on the old gallery's fixtures: first run, populated,
//! opening, no-store, no transport, the roster. The home page (2026-10-08)
//! replaced the Devices, Projects and Explore pages with one page, and these
//! stories draw that page from one fixture. They keep their old names on
//! purpose: a story's id is its file path plus function name, so the
//! stories comment on a PR reads the change as a before/after rather than
//! a delete and an add. The page's own states (signed out, guest, the full
//! library, the list view, each tab) are in `home_page_stories.rs`.
//!
//! ⚠️ The DEVICE-roster rows (the connected/offline/blank/safe-mode cards,
//! the empty-device push buttons, the section-label candidates over a
//! device roster) went with M2 of the device-model rebuild. What is left
//! covers the library pages and the live sim card.

use dioxus::prelude::*;
use lpa_studio_core::{DeviceCardFeedView, FeedLiveness};
use lpa_studio_web_story_macros::story;
use lpc_model::ProjectKind;

use lpa_studio_core::app::library::PackageHealth;
use lpa_studio_core::{
    ColorOrder, ControlDisplayLayout, ControlExtent, ControlLamp2d, ControlLayout2d,
    ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan, Revision,
    UiControlProductPreview, UiControlSampleFormat, UiExampleCard, UiHomeTab, UiHomeView, UiIssue,
    UiPackageCard, UiRuntimeBand,
};

use lpa_studio_core::UiAction;
use lpa_studio_core::{
    DeviceActivityKind, DeviceActivityView, DeviceEscape, DeviceId, DeviceLinkId,
    DeviceLoadedProject, DeviceRosterView, DeviceStatus, DeviceTerminalKind, DeviceTerminalLine,
    DeviceView, OutcomeView, PendingLinkView, RosterView,
};

use crate::app::home::card_thumb::CardThumb;
use crate::app::home::connect_board::ConnectBoardSection;
use crate::app::home::device_offer_story_fixtures::StoryHomePage;
use crate::app::home::device_offer_story_fixtures::{
    StoryBoardCard, StoryDeviceCard, StoryPendingCard, add_slot_tree,
};
use crate::app::home::device_pick_popover::{
    BoardPickMode, BoardPickPopover, ChipSource, ProjectPickPopover,
};
use crate::app::home::device_terminal::DeviceTerminal;
use crate::app::home::gallery_preview::ThumbPreviewBadge;
use crate::core::OffersProvider;
use lpa_studio_core::{BluetoothReach, OfferArgs, PUSH_SOURCE_PARAM, UiOffer};

/// A fixed "now" so relative times in baselines never drift.
pub(crate) const STORY_NOW: f64 = 1_800_000_000.0;

/// One of each kind, so the grouped surfaces show both sections.
pub(crate) fn examples() -> Vec<UiExampleCard> {
    vec![
        UiExampleCard {
            id: "catalog/fyeah-sign".to_string(),
            name: "Fyeah Sign".to_string(),
            kind: ProjectKind::General,
            description: "A porch sign on the full bus: clock, button and radio share one trigger."
                .to_string(),
        },
        UiExampleCard {
            id: "catalog/plasma".to_string(),
            name: "Plasma".to_string(),
            kind: ProjectKind::Pattern {
                exports: vec!["effect".to_string()],
            },
            description: "The smallest non-empty panel: one plasma shader with three bound knobs."
                .to_string(),
        },
    ]
}

pub(crate) fn packages() -> Vec<UiPackageCard> {
    vec![
        UiPackageCard {
            uid: "prj3fKq8Zr21bTxYw0AhVmDpe".to_string(),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: "2026-07-02-0930-porch-sign".to_string(),
            last_saved_at: Some(STORY_NOW - 2.0 * 3600.0),
            provenance: None,
            on_boards: vec!["Luna's porch sign".to_string()],
            open_elsewhere: false,
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
            on_boards: Vec::new(),
            open_elsewhere: false,
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
            on_boards: Vec::new(),
            open_elsewhere: false,
            target: None,
            health: PackageHealth::Ready,
        },
    ]
}

#[story(
    description = "First run with an empty library, in a browser that has no transport (the default roster: no Web Serial, so Connect a board says why instead of drawing squares that can only fail). A first visit, so no tabs and no cards/list switch. Create-first (the D17 deviation, 2026-07-27): Other projects is only its add row — New (a pure-blank create-and-open), Import, Paste — and the empty library has no paragraph of its own; the add row is the empty state. Then the examples and the footer. The same first visit in Chrome is home_landing's `landing`."
)]
fn first_run() -> Element {
    // no boards, no transport; the library holds nothing yet
    let home = UiHomeView {
        projects: Vec::new(),
        examples: examples(),
        devices: Default::default(),
        sections: Default::default(),
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
        on_boards: Vec::new(),
        open_elsewhere: false,
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
        projects,
        examples: examples(),
        devices: Default::default(),
        sections: Default::default(),
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
    description = "A full library and no boards: three projects under Other projects (newest saved first, each with its age and where it came from), then the add row. Compare with home_page_full_library, which has boards, patterns and a project that boards play."
)]
fn populated() -> Element {
    let home = UiHomeView {
        projects: packages(),
        examples: examples(),
        devices: Default::default(),
        sections: Default::default(),
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
    description = "A project another tab holds open: its card wears the neutral \"open in another tab\" badge and stays fully rendered and clickable (the refusal notice explains)."
)]
fn project_open_in_another_tab() -> Element {
    // M4b: a project another tab holds open — neutral badge, card stays
    // fully rendered and clickable (the refusal notice explains)
    let mut projects = packages();
    projects[0].open_elsewhere = true;
    let home = UiHomeView {
        projects,
        examples: examples(),
        devices: Default::default(),
        sections: Default::default(),
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
    description = "A project opening: its card shows busy, and every other card and the examples wait until it has opened."
)]
fn opening_a_project() -> Element {
    let mut home = UiHomeView {
        projects: packages(),
        examples: examples(),
        devices: Default::default(),
        sections: Default::default(),
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
                    seed: "catalog/plasma".to_string(),
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
pub(crate) fn thumb_lamp_frame() -> UiControlProductPreview {
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

/// [`thumb_lamp_frame`] as a board's card PULLS it: the same sign at 8 bits
/// per sample (each linear unorm16 level as its sRGB8 code, the engine's
/// rule), which is what the device card's feed carries over the wire.
pub(crate) fn live_card_lamp_frame() -> UiControlProductPreview {
    let frame = thumb_lamp_frame();
    let bytes: Vec<u8> = (0..frame.extent.sample_count() as usize)
        .map(|index| lpc_wire::linear16_to_srgb8(frame.unorm16_sample(index).unwrap_or(0)))
        .collect();
    UiControlProductPreview {
        sample_format: UiControlSampleFormat::Srgb8,
        bytes: bytes.into(),
        ..frame
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
    description = "No transport (a browser without Web Serial, or a build without the provider): the home page's Connect a board says so rather than showing squares that can only fail, which would read as \"you have no boards\"."
)]
fn devices_page_without_a_transport() -> Element {
    gallery(UiHomeView {
        projects: packages(),
        examples: examples(),
        devices: DeviceRosterView::default(),
        sections: Default::default(),
        library_available: true,
        opening: None,
        issue: None,
    })
}

#[story(
    description = "The home page's boards (the Devices page's roster, folded in), each one the board card: its picture, the name bar with one primary, then the project, connection, access, firmware and hardware bars, with no labels anywhere — a bar is known by its icon and what it says. Online boards holds the boards that are THERE — the new board first (\"USB · new\", \"No firmware\" in orange, Install as its primary), then the running board and the empty one (\"Nothing on it yet\", Add a project) — then Connect a board, then Offline boards: the board Studio remembers and cannot see is a card of its own there (the 2026-10-08 ADR reverses the old \"disconnect → disappear\" line), \"Offline\" on its connection bar, its firmware the version it last ran (\"last seen\"), Connect as its primary and Forget in its hardware details, so an unplugged board can still be removed without plugging it back in. \"Unlocking your boards\" is the closed fold under them."
)]
fn devices_page_roster() -> Element {
    // The roster with a pending link, two connected boards and one
    // remembered board, on the All tab.
    let home = UiHomeView {
        projects: packages(),
        examples: examples(),
        devices: roster_page_fixture(),
        sections: Default::default(),
        library_available: true,
        opening: None,
        issue: None,
    };
    rsx! {
        section { class: "tw:p-4",
            StoryHomePage { home, now_secs: Some(STORY_NOW), on_action: |_| {} }
        }
    }
}

#[story(
    description = "The Connect a board section's target menu, open (D44, PD16, D1, spike 2 + 2b). The section keeps its three squares — USB, Bluetooth, Network — and grows a quiet second verb under them, \"start a board here ▾\", because the place where the next board comes from should offer BOTH ways a board can appear. The menu is two groups: Desktop alone at the top (it is the default target and the one every new project gets), then every catalog board this build can actually start, in catalog order, each row a silhouette · name · tag. The tag is a lowercase WORD — the same word the `?on=` grammar uses — rather than a chip or a sentence, and it says what picking that row would START. THE THING TO LOOK AT: the XIAO ESP32-C6 now appears TWICE, because this build can emulate it and sim-versus-emu is the user's choice, never a default Studio flips. `emu` comes first — exact, then fast — and the two rows are otherwise identical, which is the claim: one board, two runtimes. The hint line under the rows has earned its place and explains the two words; it names NO modifier key, because the two rows are the whole of the choice. Picking a row mints a record of that kind, powers it on, and the card lands under Online boards. The panel floats in the top layer, so the section is exactly as tall open as shut and the page never reflows."
)]
fn devices_target_pick_open() -> Element {
    rsx! {
        // Tall enough for the WHOLE panel — every row plus the hint line
        // under them. The panel floats in the top layer, so a section that
        // merely fits the trigger clips exactly the half this story is for.
        section { class: "tw:grid tw:min-h-[720px] tw:w-[360px] tw:content-start tw:p-4",
            OffersProvider { offers: add_slot_tree(true, BluetoothReach::Ready),
                ConnectBoardSection {
                    ble_reach: Some(BluetoothReach::Ready),
                    page_url: Some("https://lightplayer.app/".to_string()),
                    pick_open: true,
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "A powered-off sim, where Q5 put it: under Offline boards (the Boards tab), the same board card an unplugged board gets. Powering a sim off keeps its record and takes everything else — so the card says what it has (a name, \"Simulated XIAO ESP32-C6\" on the hardware bar) and nothing it does not: \"In this tab · off\", and a dark picture rather than a stale one. The one thing that differs from an unplugged board's card is the primary: a runtime this tab makes has no port grant to ask the browser back for, so it reads POWER ON, dispatching the same `Connect` the model already has (PD8/Q15 — a sim adds a link and an effect backend, never a fifth flow). Forget (hardware details) keeps its confirm, whose words are the sim's own: \"Forget this sim? Its record and name go; nothing else exists.\" (D46)."
)]
fn devices_card_sim_powered_off() -> Element {
    let home = UiHomeView {
        projects: packages(),
        examples: examples(),
        devices: powered_off_sim_fixture(),
        sections: Default::default(),
        library_available: true,
        opening: None,
        issue: None,
    };
    rsx! {
        section { class: "tw:p-4",
            StoryHomePage {
                home,
                now_secs: Some(STORY_NOW),
                initial_tab: Some(UiHomeTab::Boards),
                on_action: |_| {},
            }
        }
    }
}

/// A roster holding exactly one device: a sim that has been powered off.
///
/// The band map is what makes it a sim to [`split_roster`] — the model
/// deliberately does not know — and that is what turns the tile's
/// Reconnect slot into Power on.
fn powered_off_sim_fixture() -> DeviceRosterView {
    let mut card = sim_card_view(21, "XIAO ESP32-C6 (sim)", "seeed/xiao-esp32-c6");
    card.status = DeviceStatus::Offline;
    card.state_label = "Powered off".to_string();
    card.escapes = vec![DeviceEscape::Reconnect, DeviceEscape::Forget];
    card.freshness_label = Some("last heard 4 min ago".to_string());
    let id = card.id;
    DeviceRosterView {
        access: Default::default(),
        wifi: Default::default(),
        lan_links: Default::default(),
        wifi_connects: Default::default(),
        wifi_address_connect: None,
        updates: Default::default(),
        transport_available: true,
        usb_available: true,
        layout: Default::default(),
        backup_download: None,
        board_projects: Default::default(),
        link_kinds: Default::default(),
        last_seen: Default::default(),
        ends: Default::default(),
        cards: Vec::new(),
        feeds: Default::default(),
        runtime_bands: [(id, UiRuntimeBand::sim("seeed/xiao-esp32-c6", Some("cpu")))]
            .into_iter()
            .collect(),
        open_addresses: Default::default(),
        roster: RosterView {
            pending: Vec::new(),
            devices: vec![card],
        },
    }
}

#[story(
    description = "An offline board with its LAST PICTURE (the honest-device-preview follow-up, 2026-09-07): the same page as `devices_page_roster` (here on the Boards tab), but the remembered board's feed carries the frame Studio persisted to its per-uid sidecar the last time the board was fed. Its card under Offline boards draws that frame dimmed (last known, not current), and its status corner reads \"3 h ago\" — the age measured from when the board actually published it (the STORED capture stamp, not the reload); the corner's details say \"last frame · 3 h ago\". Nothing else on the card moves: same height, same bars, same Connect and Forget. Compare against `devices_page_roster`, whose remembered board has no sidecar and keeps its picture dark."
)]
fn devices_page_remembered_last_frame() -> Element {
    let mut devices = roster_page_fixture();
    let remembered = devices
        .roster
        .devices
        .iter()
        .find(|device| device.status == DeviceStatus::Offline)
        .map(|device| device.id)
        .expect("the page fixture has a remembered board");
    devices.feeds.insert(
        remembered,
        DeviceCardFeedView {
            frame: Some(thumb_lamp_frame()),
            frame_age_secs: Some(3.0 * 3_600.0),
            engine_fps: None,
            liveness: FeedLiveness::Offline,
        },
    );
    let home = UiHomeView {
        projects: packages(),
        examples: examples(),
        devices,
        sections: Default::default(),
        library_available: true,
        opening: None,
        issue: None,
    };
    rsx! {
        section { class: "tw:p-4",
            StoryHomePage {
                home,
                now_secs: Some(STORY_NOW),
                initial_tab: Some(UiHomeTab::Boards),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "The board card's height rule (AC2), as a measurement: six states in 400px columns — running, nothing loaded, needs firmware, flashing at 62%, sending (indeterminate), and degraded. Every card is a picture, the name bar and the same five bars — project · connection · access · firmware · hardware — divided by SUBJECT, with no labels: a bar is known by its icon and what it says. Running: the board's project, \"USB · connected\", Edit as the primary. Nothing loaded: \"Nothing on it yet\" with Add a project. Needs firmware: \"No firmware\" in orange on the firmware bar, Install as the primary, the status corner orange. Degraded: the fault in orange on the project bar, Clear faults in its details. An activity narrates in the bar whose subject it changes, with its spinner, its iridescent foot and its Cancel there: compare Flashing (the firmware bar: \"Flashing firmware · 62%\") with Sending (the project bar: \"Sending the project\", no percent, so its foot sweeps); meanwhile the name bar's Edit waits, disabled, saying \"Busy:\" and the same words. Every row exists in every state, so all six cards MUST measure the same height, and a board event — a heartbeat, a fault, a lost link, a new terminal line — can never move a card nor make the gallery jump while a flash runs. Laid out three rows of two rather than six across so every state fits the captured sheet."
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
                        StoryDeviceCard {
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
    description = "The card's picture with the live feed (plan 2026-09-06 device-card-live-feed): the same Running card five times at 400px, each with the board's own published frame joined at the app view. LIVE — \"USB · live\" on the connection bar, the status corner reading \"43 fps\" (the board's engine rate off its heartbeat); STALE — the frame stays, the corner reading its age, \"12 s ago\"; OFFLINE — the last in-session frame dimmed (last known, not current); LENS — the editor holds the wire, the feed is paused, the last frame dimmed, \"editor has the wire\" in the corner's details; NO LAYOUT — frames arrive but the board's lamp layout exceeded the wire's read budget, so the picture stays dark and the corner's details say why. The lamp field is aspect-fit and letterboxed INSIDE the fixed picture row — it never follows the layout's aspect — so all five cards measure exactly the height of `devices_card_states`' cards: the picture arriving moves nothing. Compare `devices_card_states` for the never-fed card."
)]
fn devices_card_live_feed() -> Element {
    let running = card_state_fixtures().remove(0);
    let (_, card, open_uid) = running;
    // The feed's rate is the board's own, off its heartbeat (core joins one
    // from the other): the view says it too.
    let card = DeviceView {
        engine_fps: Some(43),
        ..card
    };
    let frame = live_card_lamp_frame();
    let feed = |liveness: FeedLiveness, with_layout: bool| DeviceCardFeedView {
        frame: Some(match with_layout {
            true => frame.clone(),
            false => UiControlProductPreview {
                display_layout: None,
                ..frame.clone()
            },
        }),
        frame_age_secs: Some(12.0),
        engine_fps: Some(43),
        liveness,
    };
    let looks = [
        ("Live", feed(FeedLiveness::Live, true)),
        ("Stale", feed(FeedLiveness::Stale, true)),
        ("Offline", feed(FeedLiveness::Offline, true)),
        ("Lens", feed(FeedLiveness::Lens, true)),
        ("No layout", feed(FeedLiveness::Live, false)),
    ];
    rsx! {
        section { class: "tw:p-4",
            // Two per row like `devices_card_states`, so the sheet holds
            // every card whole at the capture width.
            div { class: "tw:grid tw:grid-cols-[repeat(2,400px)] tw:items-start tw:gap-3",
                for (label , feed) in looks {
                    div { key: "{label}", class: "tw:grid tw:gap-2",
                        p { class: "tw:m-0 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground",
                            "{label}"
                        }
                        StoryDeviceCard {
                            card: card.clone(),
                            open_uid: open_uid.clone(),
                            feed: Some(feed),
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
    description = "A new board's card at each identification stage, beside a settled neighbour (follow-up filed at the ship of PR #518): the same board card, its bars decided in core per stage, so a link still identifying reads like the cards around it. NOTHING HEARD (a board parked in ROM or saying nothing): the hardware bar says \"chip unknown\", the firmware bar \"Known once it identifies\", and the connection bar \"USB · new\" with its identifying as the bar's work — no bar blank, because an empty row under an identifying board reads as a fault; its primary, Connect, waits, saying how far it has got. CHIP ONLY (the boot banner named it, still identifying): the hardware bar names the chip, \"ESP32-C6\". CHIP + MAC (the flash preflight probed it, verdict settled blank): the firmware bar says \"No firmware\" in orange, the MAC is in the hardware details, and the primary is Install. SETTLED (the running neighbour from devices_card_states) is here for the level check: every card is one height, so a new board's bars sit exactly where its neighbour's do."
)]
fn devices_card_pending() -> Element {
    let stages = pending_stage_fixtures();
    let (settled_label, settled, settled_open) = card_state_fixtures()
        .into_iter()
        .next()
        .expect("the state sheet's first card is the running neighbour");
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[repeat(2,400px)] tw:items-start tw:gap-3",
                for (label , pending) in stages {
                    div { key: "{label}", class: "tw:grid tw:gap-2",
                        p { class: "tw:m-0 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground",
                            "{label}"
                        }
                        StoryPendingCard { pending, on_action: |_| {} }
                    }
                }
                div { key: "settled", class: "tw:grid tw:gap-2",
                    p { class: "tw:m-0 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground",
                        "settled neighbour · {settled_label}"
                    }
                    StoryDeviceCard {
                        card: settled,
                        open_uid: settled_open,
                        projects: packages(),
                        examples: examples(),
                        on_action: |_| {},
                    }
                }
            }
        }
    }
}

/// The three identification stages of `devices_card_pending`, labelled.
fn pending_stage_fixtures() -> Vec<(&'static str, PendingLinkView)> {
    let identifying = roster_fixture().roster.pending[0].clone();
    let blank = roster_fixture().roster.pending[1].clone();
    vec![
        (
            "nothing heard",
            PendingLinkView {
                link: DeviceLinkId(5),
                device: DeviceId(105),
                title: "Fake ESP32 (usb-5)".to_string(),
                detail: Some("found 4 s ago".to_string()),
                detected_chip: None,
                mac: None,
                ..identifying.clone()
            },
        ),
        ("chip only", identifying),
        (
            "chip + mac",
            PendingLinkView {
                mac: Some("60:55:f9:0a:0b:0c".to_string()),
                ..blank
            },
        ),
    ]
}

#[story(
    description = "The quiet state: a board whose port is open and which has stopped saying anything (NotResponding). It invents no failure: the connection bar says \"USB · not responding\" in the warning tone, with Retry (re-run identification, no replug needed) at its end, and its details carry the honest staleness (\"last heard 4 min ago\") and Disconnect; Reset is in the hardware details, Forget apart. Nothing is claimed about what is loaded: the board has not said, so the project bar reads \"Not known yet\" and the picture stays dark. The firmware bar still names the firmware the record remembers, and the hardware bar the board — going quiet does not unlearn what the board already said."
)]
fn devices_card_not_responding() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:w-[400px]",
                StoryDeviceCard {
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
    description = "An update that moves a board's files to the new layout (the C6 repartition), in its four faces, all drawn from core's own copy (`device_layout_view`). The board card asks the question in its firmware details, which core raises so they open by themselves. Top left: the question while the update waits — Studio has already stored a backup in this browser, so Continue is live, and it acts on ONE press: the details are the question, so Continue does not arm a second time (G1 walk 2026-10-03; it keeps its Lasting tint, and the app agent still hands it to the user). Download backup is always there, Cancel leaves the board untouched. Under the question the firmware bar's work says \"Waiting for your answer…\", not \"Flashing firmware…\". Top right: the same question when this browser could NOT keep the backup and the board will be nearly full afterwards — Continue stays disabled until the backup is downloaded. Bottom left: the refusal when the files do not fit; nothing was changed, and the files can still be downloaded. Bottom right: a board that came back holding its files after an interrupted update — the firmware bar says \"Files waiting\" in orange, with Finish update at its end."
)]
fn devices_card_layout_change() -> Element {
    use lpa_studio_core::app::devices::device_layout_step::LayoutStaging;
    use lpa_studio_core::{
        DeviceBoardFs, DeviceFirmwareFace, DeviceFlashLayoutView, DeviceLayoutVerdict,
        DeviceWireVersion, device_layout_view,
    };

    let base = DeviceView {
        title: "Porch C6".to_string(),
        detected_chip: Some("esp32c6".to_string()),
        board_id: Some("seeed/xiao-esp32-c6".to_string()),
        firmware_face: DeviceFirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 abc1234".to_string()),
            wire: DeviceWireVersion::Match,
            age: lpa_studio_core::DeviceFirmwareAge::Unknown,
        },
        ..roster_fixture().roster.devices.remove(0)
    };
    let waiting = lpa_studio_core::DeviceFlashStep::WaitingForAnswer.label();
    let asking = |verdict: DeviceLayoutVerdict| DeviceView {
        status: DeviceStatus::Busy,
        state_label: waiting.to_string(),
        activity: Some(DeviceActivityView {
            kind: DeviceActivityKind::Flash,
            label: waiting.to_string(),
            percent: None,
            cancellable: true,
            cancel_requested: false,
            layout: Some(DeviceFlashLayoutView {
                verdict,
                awaiting_consent: true,
            }),
            update: None,
        }),
        escapes: vec![
            DeviceEscape::Cancel,
            DeviceEscape::Disconnect,
            DeviceEscape::Forget,
        ],
        ..base.clone()
    };
    let stored = asking(DeviceLayoutVerdict::Migrate {
        files: 9,
        bytes: 48_128,
        free_blocks: 150,
        tight: false,
        backup_stored: true,
        device_uid: Some("dev000000daqf6dvvqz".to_string()),
    });
    let unstored = asking(DeviceLayoutVerdict::Migrate {
        files: 31,
        bytes: 551_936,
        free_blocks: 24,
        tight: true,
        backup_stored: false,
        device_uid: Some("dev000000daqf6dvvqz".to_string()),
    });
    let refused_verdict = DeviceLayoutVerdict::Refused {
        files: 40,
        blocks_needed: Some(170),
        blocks_total: 176,
        blocks_reserved: 16,
        block_bytes: 4096,
    };
    let refused = DeviceView {
        last_outcome: Some(OutcomeView {
            summary: "the board's files don't fit the new firmware — nothing was changed"
                .to_string(),
            ok: false,
        }),
        ..base.clone()
    };
    let refused_staging = LayoutStaging {
        verdict: refused_verdict,
        plan: None,
        archive: None,
        restoring: None,
        downloaded: false,
    };
    let held = DeviceView {
        loaded_project: DeviceLoadedProject::Empty,
        can_remove_project: false,
        ..base.clone()
    };
    let cell = |card: DeviceView, fs: DeviceBoardFs, staged: Option<&LayoutStaging>| {
        // The verbs are offers; the shell would provide the view's tree, so
        // the story provides the one core publishes for this card — its
        // device verbs, and the layout verbs under the same prefix.
        let mut offers = crate::app::home::device_offer_story_fixtures::card_tree(
            &card,
            lpa_studio_core::DeviceFace::Wire,
            false,
            &[],
            &[],
        );
        let prefix = offers
            .device_prefix(card.id)
            .cloned()
            .expect("the card's verbs are placed");
        let layout = device_layout_view(&card, prefix, fs, true, staged, None, None, &mut offers);
        rsx! {
            div { class: "tw:grid tw:content-start tw:gap-2",
                StoryBoardCard {
                    card,
                    layout,
                    extra_offers: Some(offers),
                    on_action: |_| {},
                }
            }
        }
    };
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[repeat(auto-fill,minmax(300px,400px))] tw:items-start tw:gap-4",
                {cell(stored, DeviceBoardFs::Mounted, None)}
                {cell(unstored, DeviceBoardFs::Mounted, None)}
                {cell(refused, DeviceBoardFs::Mounted, Some(&refused_staging))}
                {cell(held, DeviceBoardFs::LegacyHeld, None)}
            }
        }
    }
}

/// A board needing its files back, with no pending backup of its own
/// (Decision 11, plan P01): "Restore from a backup file…" is the only
/// restore verb, with the board's Update beside it (defect 2026-10-06: the
/// files state never withholds an update). Picking a file opens the OS file
/// dialog directly (never a generic `UiAction` dispatch; see
/// `device_backup_import`'s module doc), so there is nothing more for this
/// card to draw until a file is chosen.
fn restore_from_file_menu_cell() -> Element {
    needs_files_back_cell(None)
}

/// A board that came back without its files (its filesystem just
/// formatted), with `pending` the backup this browser still holds for it.
fn needs_files_back_cell(pending: Option<&lpa_studio_core::BackupEntry>) -> Element {
    use lpa_studio_core::{
        DeviceBoardFs, DeviceFirmwareFace, DeviceWireVersion, device_layout_view,
    };

    let card = DeviceView {
        title: "LP-8e30".to_string(),
        detected_chip: Some("esp32c6".to_string()),
        board_id: Some("seeed/xiao-esp32-c6".to_string()),
        firmware_face: DeviceFirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 abc1234".to_string()),
            wire: DeviceWireVersion::Match,
            age: lpa_studio_core::DeviceFirmwareAge::Unknown,
        },
        loaded_project: DeviceLoadedProject::Empty,
        can_remove_project: false,
        ..roster_fixture().roster.devices.remove(0)
    };
    let mut offers = crate::app::home::device_offer_story_fixtures::card_tree(
        &card,
        lpa_studio_core::DeviceFace::Wire,
        false,
        &[],
        &[],
    );
    let prefix = offers
        .device_prefix(card.id)
        .cloned()
        .expect("the card's verbs are placed");
    let layout = device_layout_view(
        &card,
        prefix,
        DeviceBoardFs::Formatted,
        false,
        None,
        pending,
        Some("60:55:f9:0a:0b:0c"),
        &mut offers,
    );
    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-2",
            StoryBoardCard {
                card,
                layout,
                extra_offers: Some(offers),
                on_action: |_| {},
            }
        }
    }
}

/// A board that was just emptied by a Remove but has another folder on it,
/// wearing core's `project_note` in place of "Nothing loaded".
fn removed_board_note_cell() -> Element {
    let card = DeviceView {
        can_remove_project: false,
        ..roster_fixture().roster.devices.remove(3)
    };
    let offers = crate::app::home::device_offer_story_fixtures::card_tree(
        &card,
        lpa_studio_core::DeviceFace::Wire,
        false,
        &[],
        &[],
    );
    let prefix = offers
        .device_prefix(card.id)
        .cloned()
        .expect("the card's verbs are placed");
    let layout = Some(lpa_studio_core::UiDeviceLayout::note_only(
        prefix, "studio-b",
    ));
    rsx! {
        StoryBoardCard {
            card,
            projects: packages(),
            examples: examples(),
            layout,
            extra_offers: Some(offers),
            on_action: |_| {},
        }
    }
}

#[story(
    description = "After a Remove that leaves another folder on the board: the project bar, which would say \"Nothing on it yet\", says what the board will start at its next power-up — \"studio-b starts at next power-up\" — in core's words, with the longer sentence (\"studio-b is still on the board and will start when it's next powered on.\") in its details. It is the same fixed bar, so the card is exactly as tall as the \"Nothing loaded\" card in `devices_card_states`. Shown at desktop card width (left) and phone card width (right). It goes away as soon as anything else happens to the board (a push, a project reported loaded). Needs Yona's look before merge."
)]
fn devices_card_removed_board_note() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:flex tw:flex-wrap tw:items-start tw:gap-4",
                div { class: "tw:w-[400px] tw:max-w-full", {removed_board_note_cell()} }
                div { class: "tw:w-[343px] tw:max-w-full", {removed_board_note_cell()} }
            }
        }
    }
}

#[story(
    description = "The gap Decision 11 of the repartition ADR closes: a board needing its files back, with no pending backup in THIS browser — a different machine, cleared storage, or an interrupted migration whose only surviving copy is the file it offered as a download. The firmware bar says \"Its files need restoring\"; its details hold \"Restore from a backup file…\" as the one restore verb (no Restore files, no Download backup: nothing is stored here) and the board's Update: an update never touches the board's files (defect 2026-10-06). Pressing it opens the OS file picker directly — a file dialog cannot be a `UiAction`, as with the library's zip Import — so there is no sheet to capture here; the next two stories show the words it leads to."
)]
fn devices_card_restore_from_file_menu() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:w-[400px]", {restore_from_file_menu_cell()} }
        }
    }
}

#[story(
    description = "A board that came back without its files while THIS browser still holds their backup (the update that moved them was cut off mid-write; the repartition walk's W7): the firmware bar says \"Its files need restoring\" with Restore files, and its details name the backup's date and hold Download backup, Restore from a backup file… and the board's Update — an update never touches the board's files, so a files problem never hides it (defect 2026-10-06). The card keeps its height."
)]
fn devices_card_restore_files_beside_update() -> Element {
    let pending = lpa_studio_core::BackupEntry {
        base_mac: "60:55:f9:0a:0b:0c".to_string(),
        archive: "60-55-f9-0a-0b-0c-1791000000.zip".to_string(),
        captured_at_epoch_seconds: 1_791_000_000.0,
        purpose: "layout-migration".to_string(),
        status: lpa_studio_core::BackupStatus::Pending,
        file_count: 23,
        total_bytes: 96_000,
    };
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:w-[400px]", {needs_files_back_cell(Some(&pending))} }
        }
    }
}

#[story(
    description = "The one question a backup naming a DIFFERENT board ever asks (ease over ceremony: it never stacks a second confirmation on top). It is a native confirm — `check_backup_file`'s own words, asked by the web shell before anything is dispatched; `device_backup_import::tests::a_different_board_names_both_in_one_question` pins this exact sentence. Saying yes puts the file into the store as THIS board's pending backup and the ordinary Restore files flow (its own sheet) takes it from there — no second restore path."
)]
fn devices_card_restore_from_file_mismatch() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:w-[400px] tw:gap-2",
                {restore_from_file_menu_cell()}
                p { class: "tw:m-0 tw:rounded-md tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-2.5 tw:py-2 tw:text-sm tw:leading-snug tw:text-status-warning-foreground",
                    "Picking a backup from a different board asks: \"This backup is from LP-8e30, not LP-0b0c. Restore it onto this board anyway?\""
                }
            }
        }
    }
}

#[story(
    description = "A file that is not a LightPlayer backup is refused in words, never a raw code — `check_backup_file`'s own refusal, surfaced as a native alert before anything is dispatched. `device_backup_import::tests::a_bad_archive_is_refused_in_words_not_a_code` and `backup_archive.rs`'s own tests pin the three shapes: no manifest.json at all, a format version this build does not read (v1 or v3), and an entry that would escape the device's filesystem."
)]
fn devices_card_restore_from_file_refused() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:w-[400px] tw:gap-2",
                {restore_from_file_menu_cell()}
                p { class: "tw:m-0 tw:rounded-md tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-2.5 tw:py-2 tw:text-sm tw:leading-snug tw:text-status-warning-foreground",
                    "A file this Studio cannot use says so plainly: \"not a LightPlayer backup: no manifest.json\", or \"backup format 1 is not one this build reads (it reads 2)\"."
                }
            }
        }
    }
}

#[story(
    description = "An update's steps, as the card names them (G1 walk 2026-10-03, Yona: \"the 'Flashing firmware…' label isn't really right for the first phase\"). In the order an update that moves a board's files runs, each on the firmware bar: Reading the board… (its layout, and its files when they must move — nothing is written yet), Waiting for your answer… (the question is up; no stale percent from the read), Flashing firmware…, Moving files…, Checking the files… (the read-back, and the board's own boot proving they mounted). The words are the device model's own (`FlashStep`); the disabled Edit says them after \"Busy:\". A flash that moves no files reads the board, then says Flashing firmware… to the end, as before."
)]
fn devices_card_update_steps() -> Element {
    use lpa_studio_core::DeviceFlashStep;

    let running = DeviceView {
        title: "Porch C6".to_string(),
        detected_chip: Some("esp32c6".to_string()),
        board_id: Some("seeed/xiao-esp32-c6".to_string()),
        firmware_face: lpa_studio_core::DeviceFirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 abc1234".to_string()),
            wire: lpa_studio_core::DeviceWireVersion::Match,
            age: lpa_studio_core::DeviceFirmwareAge::Unknown,
        },
        terminal: vec![
            story_line(DeviceTerminalKind::Studio, "Flashing firmware"),
            story_line(DeviceTerminalKind::Studio, "Reading the board's files"),
        ],
        ..roster_fixture().roster.devices.remove(0)
    };
    let at = |step: DeviceFlashStep, percent: Option<u8>| DeviceView {
        status: DeviceStatus::Busy,
        state_label: step.label().to_string(),
        activity: Some(DeviceActivityView {
            kind: DeviceActivityKind::Flash,
            label: step.label().to_string(),
            percent,
            cancellable: true,
            cancel_requested: false,
            layout: None,
            update: None,
        }),
        can_remove_project: false,
        escapes: vec![
            DeviceEscape::Cancel,
            DeviceEscape::Disconnect,
            DeviceEscape::Forget,
        ],
        ..running.clone()
    };
    let steps = [
        at(DeviceFlashStep::ReadingBoard, Some(40)),
        at(DeviceFlashStep::WaitingForAnswer, None),
        at(DeviceFlashStep::FlashingFirmware, Some(35)),
        at(DeviceFlashStep::MovingFiles, Some(70)),
        at(DeviceFlashStep::CheckingFiles, Some(100)),
    ];
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[repeat(auto-fill,minmax(300px,400px))] tw:items-start tw:gap-4",
                for card in steps {
                    StoryDeviceCard {
                        card,
                        projects: vec![],
                        examples: vec![],
                        on_action: |_| {},
                    }
                }
            }
        }
    }
}

#[story(
    description = "An older LightPlayer board — the hello of a board on a wire this Studio cannot read (every fielded C6 at wire 32, in a wire-33 Studio) — in a fresh browser, attached mid-stream so no boot banner named its chip. Both firmware bars say \"Older LightPlayer\", not the old contradictory \"No firmware\". LEFT: its hello named the board Studio stamped on it (`hardware.boardId`, the one other field read off an older hello), so the card knows the board, and through it the chip, and its firmware details offer Update with no board pick (G1 walk 2026-10-03, Yona: \"it really shouldn't say 8 boards fit … ideally we'd know what board it is\"). RIGHT: the same board when nothing names it (never stamped, nothing remembered) — the one case left for the pick: Install, over every board."
)]
fn devices_card_older_firmware() -> Element {
    let older = DeviceView {
        title: "Spare C6".to_string(),
        status: DeviceStatus::NeedsAttention,
        state_label: "Older LightPlayer firmware".to_string(),
        detected_chip: None,
        board_id: Some("seeed/xiao-esp32-c6".to_string()),
        identity_label: Some("10:bd:a3:b0:8e:30".to_string()),
        firmware_face: lpa_studio_core::DeviceFirmwareFace::OlderLightPlayer { proto: Some(32) },
        remembered_firmware: None,
        loaded_project: DeviceLoadedProject::Unknown,
        can_remove_project: false,
        last_outcome: None,
        activity: None,
        escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
        terminal: vec![
            story_line(DeviceTerminalKind::Board, "Opened the port"),
            story_line(
                DeviceTerminalKind::Wire,
                "hello · proto 32 · seeed/xiao-esp32-c6 · another wire: only its version and \
                 board were read",
            ),
        ],
        ..roster_fixture().roster.devices.remove(0)
    };
    let unnamed = DeviceView {
        board_id: None,
        terminal: vec![
            story_line(DeviceTerminalKind::Board, "Opened the port"),
            story_line(
                DeviceTerminalKind::Wire,
                "hello · proto 32 · ? · another wire: only its version and board were read",
            ),
        ],
        ..older.clone()
    };
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[repeat(2,400px)] tw:items-start tw:gap-4",
                StoryDeviceCard {
                    card: older,
                    projects: vec![],
                    examples: vec![],
                    on_action: |_| {},
                }
                StoryDeviceCard {
                    card: unnamed,
                    projects: vec![],
                    examples: vec![],
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The board card's two Lasting verbs armed, beside the idle card (2K+, devices-treatments spike gate 2026-08-31; D8). Middle: Forget armed in the hardware details' danger zone. Right: Remove project armed in the project details' danger zone — D8, the OTHER destructive verb, in another bar's details, which is why a capture that proves the arming needs both. Each arms in place, two clicks: its row reads its confirm in the error tint with the quiet drain, and nothing else on the card dims (the old card-wide marking is gone). Blur or the 4s window stands down. Captured with the story-only armed_preview hooks; the knock and the quiet drain track are motion and do not capture."
)]
fn devices_card_armed() -> Element {
    let card = armed_card_fixture();
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[repeat(3,340px)] tw:items-start tw:gap-3",
                StoryDeviceCard {
                    card: card.clone(),
                    projects: vec![],
                    examples: vec![],
                    on_action: |_| {},
                }
                StoryDeviceCard {
                    card: card.clone(),
                    projects: vec![],
                    examples: vec![],
                    armed_preview: true,
                    on_action: |_| {},
                }
                StoryDeviceCard {
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
    description = "Running vs Degraded, side by side under Online boards on the Boards tab (a fault is never black, 2026-09-02). First: the healthy running card. Second: a board in the SAME state reporting a faulted node — the project bar takes the attention tone and names the node and the runtime's own reason, in the bar that otherwise holds the project name (one line; the whole fault in its details), and the status corner carries the same notice. The running card is deliberately kept: a degraded board is still running, which is why Edit stays its primary and the fault reads as a bar's words rather than as a new state. This is the card that lied for two days while a quarantined shader rendered black (2026-09-01 bench). The degraded card also carries one extra verb, in the project details with the fault it answers — Clear faults — which forgets the board's crash ledger and re-arms the faulted nodes; the healthy card does not offer it, because there would be nothing for it to do."
)]
fn devices_page_degraded_card() -> Element {
    // Two boards, not one board twice: the page lists a board once. The
    // second is the first's state with a faulted node, under its own handle.
    let healthy = roster_fixture().roster.devices.remove(0);
    let mut degraded = degraded_card_fixture();
    degraded.id = DeviceId(11);
    degraded.title = "Roof sign".to_string();
    let base = roster_fixture();
    let mut open_addresses = base.open_addresses.clone();
    open_addresses.insert(11, "dev000000daqf6dvvqy".to_string());
    let home = UiHomeView {
        projects: Vec::new(),
        examples: examples(),
        devices: DeviceRosterView {
            open_addresses,
            roster: RosterView {
                pending: Vec::new(),
                devices: vec![healthy, degraded],
            },
            ..base
        },
        sections: Default::default(),
        library_available: true,
        opening: None,
        issue: None,
    };
    rsx! {
        section { class: "tw:p-4",
            StoryHomePage {
                home,
                now_secs: Some(STORY_NOW),
                initial_tab: Some(UiHomeTab::Boards),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    label = "Agent light — a device card's verb",
    description = "The agent light (agentic-UI M8) on a board card. The assistant pressed this board's Remove project (`devices/<board>/remove-project`) — or handed it to you on a card — so that row, in the project details, wears the assistant's orchid ring (spinning live, still here), as it would on the docked lens card: every control that draws an offer is keyed by the offer's path. Both cards have the project details open. LEFT: at rest. RIGHT: lit. Nothing else on the card moves."
)]
fn devices_card_agent_lit() -> Element {
    let card = roster_fixture().roster.devices.remove(0);
    let lit = crate::app::agent::story_activity([(
        story_board_prefix(card.id).child("remove-project"),
        lpa_studio_core::AgentActivityKind::Pressed,
    )]);
    let project_details = crate::app::board_card::CardPart::Bar(lpa_studio_core::BarLayer::Project);
    rsx! {
        div { class: "tw:grid tw:max-w-xl tw:grid-cols-2 tw:gap-3 tw:p-4",
            StoryBoardCard {
                card: card.clone(),
                details_open: Some(project_details),
                on_action: |_| {},
            }
            crate::app::agent::AgentActivityProvider { activity: lit,
                StoryBoardCard {
                    card,
                    details_open: Some(project_details),
                    on_action: |_| {},
                }
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
pub(crate) fn roster_fixture() -> DeviceRosterView {
    DeviceRosterView {
        access: Default::default(),
        wifi: Default::default(),
        lan_links: Default::default(),
        wifi_connects: Default::default(),
        wifi_address_connect: None,
        updates: Default::default(),
        transport_available: true,
        usb_available: true,
        layout: Default::default(),
        backup_download: None,
        board_projects: Default::default(),
        link_kinds: Default::default(),
        last_seen: Default::default(),
        ends: Default::default(),
        cards: Vec::new(),
        feeds: Default::default(),
        runtime_bands: Default::default(),
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
                    mac: None,
                    firmware_blocked: None,
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
                    mac: None,
                    firmware_blocked: None,
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
                        age: lpa_studio_core::DeviceFirmwareAge::Unknown,
                    },
                    remembered_firmware: None,
                    degraded: None,
                    // The RUNNING face (M3): what the board itself reports,
                    // named by the storage dir it runs from.
                    engine_fps: None,
                    link_counters: None,
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
                    firmware_blocked: None,
                    escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
                    update_blocked: None,
                    last_update_outcome: None,
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
                        age: lpa_studio_core::DeviceFirmwareAge::Unknown,
                    },
                    remembered_firmware: None,
                    degraded: None,
                    loaded_project: DeviceLoadedProject::Unknown,
                    engine_fps: None,
                    link_counters: None,
                    // Busy: one activity per device, so no second verb.
                    can_receive_project: false,
                    can_remove_project: false,
                    activity: Some(DeviceActivityView {
                        kind: DeviceActivityKind::Identify,
                        label: "Identifying…".to_string(),
                        percent: Some(40),
                        cancellable: true,
                        cancel_requested: false,
                        layout: None,
                        update: None,
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
                    firmware_blocked: None,
                    escapes: vec![
                        DeviceEscape::Cancel,
                        DeviceEscape::Disconnect,
                        DeviceEscape::Forget,
                    ],
                    update_blocked: None,
                    last_update_outcome: None,
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
                    remembered_firmware: None,
                    degraded: None,
                    loaded_project: DeviceLoadedProject::Unknown,
                    engine_fps: None,
                    link_counters: None,
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
                    firmware_blocked: None,
                    escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
                    update_blocked: None,
                    last_update_outcome: None,
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
                        age: lpa_studio_core::DeviceFirmwareAge::Unknown,
                    },
                    remembered_firmware: None,
                    degraded: None,
                    loaded_project: DeviceLoadedProject::Empty,
                    engine_fps: None,
                    link_counters: None,
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
                    firmware_blocked: None,
                    escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
                    update_blocked: None,
                    last_update_outcome: None,
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
                    // No link, no window, no verdict: the face is Unknown
                    // (the Firmware zone says so), and the header names
                    // what the record remembers, marked as memory.
                    firmware_face: lpa_studio_core::DeviceFirmwareFace::Unknown,
                    remembered_firmware: Some("fw-esp32c6 abc1234".to_string()),
                    degraded: None,
                    loaded_project: DeviceLoadedProject::Unknown,
                    engine_fps: None,
                    link_counters: None,
                    can_receive_project: false,
                    can_remove_project: false,
                    activity: None,
                    last_outcome: None,
                    // Nothing live to show: the link is gone, so the
                    // terminal has nothing to say and the tile draws none.
                    terminal: Vec::new(),
                    terminal_dropped: 0,
                    // The two verbs an absent board can honestly offer.
                    firmware_blocked: None,
                    escapes: vec![DeviceEscape::Reconnect, DeviceEscape::Forget],
                    update_blocked: None,
                    last_update_outcome: None,
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
pub(crate) fn roster_page_fixture() -> DeviceRosterView {
    let full = roster_fixture();
    let mut devices = full.roster.devices;
    // 0 = running · 3 = empty · 4 = the remembered board.
    let remembered = devices.remove(4);
    let empty = devices.remove(3);
    let running = devices.remove(0);
    DeviceRosterView {
        access: Default::default(),
        wifi: Default::default(),
        lan_links: Default::default(),
        wifi_connects: Default::default(),
        wifi_address_connect: None,
        updates: Default::default(),
        transport_available: true,
        usb_available: true,
        layout: Default::default(),
        backup_download: None,
        board_projects: Default::default(),
        link_kinds: Default::default(),
        last_seen: Default::default(),
        ends: Default::default(),
        cards: Vec::new(),
        feeds: Default::default(),
        runtime_bands: Default::default(),
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
    description = "One card per FIRMWARE FACE — the sheet that did not exist when an older board shipped drawn as a blank chip (bench 2026-09-04: a proto-19 classic on a proto-20 Studio read \"Blank flash — needs firmware\" and \"no firmware\" while its terminal decoded the hello naming fw-esp32v3 and a heartbeat carrying a red fault). Eight cards in 400px columns, each in ITS OWN words on its firmware bar, decided in core and tested per variant. Two verbs for two situations (ruled 2026-09-04): a running LightPlayer's firmware bar offers UPDATE; a needs-firmware face's primary is INSTALL, with the board pick, since nothing is known. OLDER (a running LightPlayer one wire version behind — still running, its fault in orange on the project bar): the firmware bar is blue, the version alone, with Update and no board pick because the registry knows the board — offered, never forced; OLDER, BOARD UNKNOWN (the bench classic verbatim: its hello says `?` because the board id comes from the manifest Studio stamps at flash and this board was flashed from the CLI, the registry has no board either, and a classic chip fits several boards — so the SAME Update opens the board pick once, and the panel says why); NEWER (plain, no recommendation); PRE-HELLO, FOREIGN, BOOTLOADER and SILENT, each named in orange — \"Pre-hello firmware\", \"Other firmware\", \"In download mode\", \"No response\" — with the whole verdict in the firmware details and Install as the primary (SILENT's connection bar also says \"USB · not responding\", with Retry); and ATTACHED — NOT LISTENING (the older classic after Disconnect, bench 2026-09-04): \"USB · not connected\" with Connect as the primary, while the firmware bar keeps the version the record remembers with \"last seen\" — memory marked as memory, never a live claim. The status corner carries each card's worst notice, and every card measures the same height (AC2)."
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
                        StoryDeviceCard {
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
/// row differs (one click vs. the pick once) — plus the closed window that
/// wears none and remembers one.
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
            age: lpa_studio_core::DeviceFirmwareAge::Unknown,
        },
        degraded: Some("Recovery red: /studio.show/s disabled after repeated crashes".to_string()),
        engine_fps: None,
        link_counters: None,
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
    // the registry has no board either. A classic chip fits several boards,
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
            age: lpa_studio_core::DeviceFirmwareAge::Unknown,
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
        remembered_firmware: None,
        degraded: None,
        loaded_project: DeviceLoadedProject::Unknown,
        engine_fps: None,
        link_counters: None,
        can_receive_project: false,
        can_remove_project: false,
        activity: None,
        last_outcome: None,
        terminal: vec![story_line(
            DeviceTerminalKind::Rom,
            "ESP-ROM:esp32c6-20220919",
        )],
        terminal_dropped: 0,
        firmware_blocked: None,
        escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
        update_blocked: None,
        last_update_outcome: None,
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

    // The bench case after Disconnect (2026-09-04): the same classic, port
    // handed back. The window restarted, so the face is Unknown and the
    // Firmware zone honestly says nothing was reported — while the header
    // keeps the identity the board already earned: chip and board from the
    // record, and the firmware it last ran, marked "· last seen" rather
    // than passed off as live.
    let attached_closed = DeviceView {
        id: DeviceId(37),
        title: "Bench classic · Sep 4".to_string(),
        status: DeviceStatus::Attached,
        state_label: "Attached — not listening".to_string(),
        detail: None,
        freshness_label: Some("last heard 8 s ago".to_string()),
        identity_label: Some("30:76:f5:ec:f6:34".to_string()),
        detected_chip: Some("esp32".to_string()),
        board_id: Some("quinled/dig-uno".to_string()),
        firmware_face: DeviceFirmwareFace::Unknown,
        remembered_firmware: Some("fw-esp32v3 7c80a27".to_string()),
        degraded: None,
        loaded_project: DeviceLoadedProject::Unknown,
        engine_fps: None,
        link_counters: None,
        can_receive_project: false,
        can_remove_project: false,
        activity: None,
        last_outcome: None,
        terminal: vec![
            story_line(
                DeviceTerminalKind::Wire,
                "hello · proto 19 · ? · fw-esp32v3 7c80a27 (dirty)",
            ),
            story_repeat(
                DeviceTerminalKind::Wire,
                "heartbeat · studio · FAULT red",
                5,
            ),
            story_line(DeviceTerminalKind::Studio, "Closed the port"),
        ],
        terminal_dropped: 0,
        // The link is still attached (the port was closed, not unplugged),
        // so the projection offers Disconnect — which is also what keeps
        // the terminal and the verb rows drawn at their fixed heights.
        firmware_blocked: None,
        escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
        update_blocked: None,
        last_update_outcome: None,
    };

    vec![
        ("Older than Studio", older, open_uid.clone()),
        ("Older, board unknown", older_unknown, open_uid.clone()),
        ("Newer than Studio", newer, open_uid),
        ("Pre-hello firmware", pre_hello, None),
        ("Foreign firmware", foreign, None),
        ("Bootloader", bootloader, None),
        ("Silent", silent, None),
        ("Attached — not listening", attached_closed, None),
    ]
}

/// A sim-backed device card: the SAME `DeviceView` a board gets, plus the
/// record's own title and board, so the only difference the sheet shows is
/// the band.
pub(crate) fn sim_card_view(id: u64, title: &str, board_id: &str) -> DeviceView {
    let running = roster_fixture().roster.devices.remove(0);
    DeviceView {
        id: DeviceId(id),
        title: title.to_string(),
        board_id: Some(board_id.to_string()),
        identity_label: Some("02:1a:2b:3c:4d:5e".to_string()),
        ..running
    }
}

#[story(
    description = "The sim as a device (PD9/PD11): three faces of the SAME board card a board gets, the one difference being what its bars say. The hardware bar reads \"Simulated <board>\" with \"in this tab\" as its aside (the tier the worker was granted is in the hardware details), and the connection bar \"In this tab · live\". READY (Desktop, GPU) — the card a library open lands on; READY (a board sim, CPU) — the same card acting as a XIAO ESP32-C6, which is what makes \"it acts as its target\" legible; UNDER THE LENS — the editor holds the wire, so the picture is dimmed and the status corner says so, exactly as it does for a board. No title prefix, no tinted edge, no second glyph: everything else is the board card verbatim (D38)."
)]
fn devices_card_sim_faces() -> Element {
    let desktop = sim_card_view(11, "Desktop sim", "lightplayer/desktop");
    let board = sim_card_view(12, "C6 sim", "seeed/xiao-esp32-c6");
    let lens = DeviceView {
        loaded_project: DeviceLoadedProject::Running {
            label: "porch-sign".to_string(),
        },
        ..desktop.clone()
    };
    let lens_feed = DeviceCardFeedView {
        frame: Some(thumb_lamp_frame()),
        liveness: FeedLiveness::Lens,
        frame_age_secs: Some(2.0),
        engine_fps: None,
    };
    let faces: Vec<(
        &str,
        DeviceView,
        Option<UiRuntimeBand>,
        Option<DeviceCardFeedView>,
    )> = vec![
        (
            "Ready · Desktop · GPU",
            desktop.clone(),
            Some(UiRuntimeBand::sim("lightplayer/desktop", Some("gpu"))),
            None,
        ),
        (
            "Ready · a board sim · CPU",
            board,
            Some(UiRuntimeBand::sim("seeed/xiao-esp32-c6", Some("cpu"))),
            None,
        ),
        (
            "Under the lens",
            lens,
            Some(UiRuntimeBand::sim("lightplayer/desktop", Some("gpu"))),
            Some(lens_feed),
        ),
    ];
    rsx! {
        section { class: "tw:p-4 tw:grid tw:gap-4",
            div { class: "tw:grid tw:grid-cols-[repeat(2,400px)] tw:items-start tw:gap-3",
                for (label , card , runtime , feed) in faces {
                    div { key: "{label}", class: "tw:grid tw:gap-2",
                        p { class: "tw:m-0 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground",
                            "{label}"
                        }
                        StoryDeviceCard {
                            card,
                            runtime,
                            feed,
                            open_uid: Some("dev000000daqf6dvvqz".to_string()),
                            projects: packages(),
                            examples: examples(),
                            on_action: |_| {},
                        }
                    }
                }
            }
            // Powered off: an offline sim is not a live card — its record
            // sits under Offline boards with Power on in the Reconnect slot
            // (Q5), which `devices_card_sim_powered_off` captures.
        }
    }
}

#[story(
    description = "The emu as a device (D1/D25): the SAME board card a sim and a board get. The hardware bar reads \"Emulated XIAO ESP32-C6\" with \"in this tab\" as its aside; compare against `devices_card_sim_faces`, which differs only in that word. THE THING TO JUDGE IS THE SPEED, in the hardware details: a sim's details carry the shader tier its worker was GRANTED; an emu grants no tier, so the honest thing to show is the number that actually varies — how fast the emulated board runs against wall time. Three readings, and they are the real measured range: 0.5× is a desk tab doing ordinary work; 0.04× is a tab nobody is looking at, written to two decimals so it never reads \"0.0×\"; and NOTHING MEASURED YET shows no speed at all rather than a zero. The cards here are closed, so the speed is one click into the hardware bar."
)]
fn devices_card_emu_band() -> Element {
    let faces: Vec<(&str, Option<f64>)> = vec![
        ("Ready · a desk tab", Some(0.5)),
        ("Ready · the tab is hidden", Some(0.042)),
        ("Ready · nothing measured yet", None),
    ];
    rsx! {
        section { class: "tw:p-4 tw:grid tw:gap-4",
            div { class: "tw:grid tw:grid-cols-[repeat(2,400px)] tw:items-start tw:gap-3",
                for (label , dilation) in faces {
                    div { key: "{label}", class: "tw:grid tw:gap-2",
                        p { class: "tw:m-0 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground",
                            "{label}"
                        }
                        StoryDeviceCard {
                            card: sim_card_view(31, "XIAO ESP32-C6 (emu)", "seeed/xiao-esp32-c6"),
                            runtime: Some(UiRuntimeBand::emu("seeed/xiao-esp32-c6", dilation)),
                            feed: None,
                            open_uid: Some("dev000000daqf6dvvqz".to_string()),
                            projects: packages(),
                            examples: examples(),
                            on_action: |_| {},
                        }
                    }
                }
                // The sim's band, at the same scale, so the one clause that
                // differs is read side by side rather than remembered.
                div { class: "tw:grid tw:gap-2",
                    p { class: "tw:m-0 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground",
                        "For comparison · the sim's band"
                    }
                    StoryDeviceCard {
                        card: sim_card_view(32, "XIAO ESP32-C6 (sim)", "seeed/xiao-esp32-c6"),
                        runtime: Some(UiRuntimeBand::sim("seeed/xiao-esp32-c6", Some("cpu"))),
                        feed: None,
                        open_uid: Some("dev000000daqf6dvvqz".to_string()),
                        projects: packages(),
                        examples: examples(),
                        on_action: |_| {},
                    }
                }
            }
        }
    }
}

#[story(
    description = "An emu that came up on a BLANK CHIP (D22/D24): the needs-firmware face, on the same board card, \"Emulated XIAO ESP32-C6\" on its hardware bar. An emu is born flashed — the record's first power-on hands the worker a manifest URL and the worker fetches the packaged build, writes it into the 4 MiB image and boots into it — so this face means the fetch did not happen: this build serves no image for the board, or the network refused. THE CLAIM TO CHECK IS THAT NOTHING IS SPECIAL HERE. It is verbatim what a blank board on the desk gets (compare `devices_card_firmware_faces`): \"No firmware\" on the firmware bar, the verdict in its details, Install with the board pick. (The terminal in the status corner is the story fixture's shared transcript — furniture here, not evidence.) And the verb means what it says — mode A writes the emulated chip directly and resets it, with no ROM downloader in the way — which is exactly why the two verbs a sim has nothing honest to do are real on an emu. The hardware bar is the one row that tells you where this board is; no speed in its details: a chip that never booted has reported no time."
)]
fn devices_card_emu_needs_firmware() -> Element {
    let card = DeviceView {
        status: DeviceStatus::NeedsAttention,
        state_label: "Blank flash — needs firmware".to_string(),
        detail: Some("chip: esp32c6".to_string()),
        firmware_face: lpa_studio_core::DeviceFirmwareFace::Blank,
        remembered_firmware: None,
        loaded_project: DeviceLoadedProject::Unknown,
        can_receive_project: false,
        can_remove_project: false,
        escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
        ..sim_card_view(33, "XIAO ESP32-C6 (emu)", "seeed/xiao-esp32-c6")
    };
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:w-[400px] tw:gap-2",
                StoryDeviceCard {
                    card,
                    runtime: Some(UiRuntimeBand::emu("seeed/xiao-esp32-c6", None)),
                    feed: None,
                    open_uid: Some("dev000000daqf6dvvqz".to_string()),
                    projects: packages(),
                    examples: examples(),
                    on_action: |_| {},
                }
            }
        }
    }
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
            layout: None,
            update: None,
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
            layout: None,
            update: None,
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
        projects: Vec::new(),
        examples: examples(),
        devices: Default::default(),
        sections: Default::default(),
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

/// The home page from one fixture — what the old Devices, Projects and
/// Explore pages stacked here now all draw as one page.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn GalleryPages(
    home: UiHomeView,
    #[props(default)] now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        StoryHomePage { home, now_secs, on_action }
    }
}

/// P5 (device-card-v2 plan): the terminal renderer alone, fed the shape
/// [`Evidence::fold`] actually produces — a capped, typed, repeat-collapsed
/// tail plus a drop count — rather than raw board chatter. `TERMINAL_CAP`
/// in `lpa-devices` is 200; this fixture stands in for 250 raw lines
/// having arrived (200 kept, 50 dropped), mixing every
/// [`DeviceTerminalKind`], a single ×6 repeat, several decoded wire
/// summaries, one 400-character block-plan dump, and — last, so the pinned
/// view opens on them — the two long verdict lines a person pastes into a
/// bug report: the 2026-09-04 bench's stamp-failed flash outcome, and a
/// long identification failure. None of them fold (2026-09-04): they wrap
/// whole so a select + Cmd+C copies the reason, not "+77 chars".
#[story(
    description = "The terminal renderer alone (P5): natural oldest-first order, typed colours, a ×6 repeat badge, wire rows tagged and coloured live-blue, the dropped-lines notice for the 50 lines the 200-line cap pushed out, and — pinned into view at the bottom — two long verdict lines (a 193-char green flash outcome from the 2026-09-04 bench and a 234-char red identification failure) wrapping WHOLE under a hanging indent. No line ever folds (2026-09-04): the box is a fixed-height scroller, so a long line costs scroll, never card height, and selecting the panel + Cmd+C copies exactly what the board said. Pinned to the bottom on load."
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

/// A 400-character line — the realistic "the panel used to just eat this"
/// block-plan dump, wrapping whole now that nothing folds.
fn device_terminal_story_long_line() -> String {
    let prefix = "Esp32C6RmtWs281xDriver: block plan published: outputs=[{pin:2,px:300,fmt:grb},{pin:3,px:300,fmt:grb}] clock=pll_f80m/1 lut=gamma-2.2-8bit dither=temporal-4 frame_us=23100 margin_words=3 — this is the kind of line that used to eat the panel — ";
    let filler_len = 400_usize.saturating_sub(prefix.chars().count());
    format!("{prefix}{}", "…".repeat(filler_len))
}

/// The 2026-09-04 bench line, as the flash activity minted it: a flash that
/// installed but could not stamp the board manifest degrades to a
/// success-with-a-reason (`success_without_stamp`), so it is an OUTCOME
/// line — and the fold used to show 120 characters of it plus "+77 chars".
const BENCH_STAMP_FAILED_OUTCOME: &str = "firmware installed; writing the board manifest failed (the board never became ready to write to: transport error: Transport closed — the port went away) — the compiled-in default pin map stands";

/// A long FAILURE line of the shape `device_readiness` produces when
/// nothing answered the hello: the verdict, then the recent serial snippet
/// it was reached from.
const LONG_IDENTIFICATION_FAILURE: &str = "identification timed out — Transport error: no LightPlayer firmware detected; recent serial output: invalid header: 0xffffffff · invalid header: 0xffffffff · ESP-ROM:esp32c6-20220919 · rst:0x1 (POWERON),boot:0x2c (SPI_FAST_FLASH_BOOT)";

/// 200 typed lines (the model's `TERMINAL_CAP`) mixing every
/// [`DeviceTerminalKind`], the ×6 repeat, the 400-char block-plan dump, a
/// run of decoded wire heartbeats, and the two long verdict lines last.
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

    // Fill up to the 200-line cap with decoded wire heartbeats, the bulk of
    // what a running board actually says, leaving room for the tail below.
    let tail = vec![
        story_line(DeviceTerminalKind::Studio, "Flashing firmware"),
        story_line(DeviceTerminalKind::Studio, "Writing the board manifest"),
        story_line(DeviceTerminalKind::Outcome, BENCH_STAMP_FAILED_OUTCOME),
        story_line(DeviceTerminalKind::Studio, "Identifying the board"),
        story_line(DeviceTerminalKind::Failure, LONG_IDENTIFICATION_FAILURE),
    ];
    let heartbeats_needed = 200_usize.saturating_sub(lines.len() + tail.len());
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
    lines.extend(tail);

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

/// The empty face's `push` offer over the pick stories' library.
fn pick_popover_push() -> UiOffer {
    lpa_studio_core::push_device_offer(
        &pick_popover_card(),
        story_board_prefix(pick_popover_card().id),
        &pick_popover_library(),
        &pick_popover_examples(),
        false,
    )
    .expect("an empty LightPlayer takes a project")
}

/// Where a story board's verbs live (the ref is never drawn).
fn story_board_prefix(device: DeviceId) -> lpa_studio_core::OfferPath {
    lpa_studio_core::OfferPath::board(&lpa_studio_core::BoardRef::New(device.0 as u32))
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
            on_boards: Vec::new(),
            open_elsewhere: false,
            target: None,
            health: PackageHealth::Ready,
        })
        .collect()
}

/// Six bundled examples, three of each kind, so the Examples tab shows
/// both of its sections as grids rather than rows.
fn pick_popover_examples() -> Vec<UiExampleCard> {
    [
        ("Logo sign", false),
        ("Meteor", true),
        ("Porch sign", false),
        ("Plasma", true),
        ("Zook dome", false),
        ("Candle", true),
    ]
    .into_iter()
    .map(|(name, pattern)| UiExampleCard {
        id: format!("catalog/{}", name.to_lowercase().replace(' ', "-")),
        name: name.to_string(),
        kind: if pattern {
            ProjectKind::Pattern {
                exports: vec!["effect".to_string()],
            }
        } else {
            ProjectKind::General
        },
        description: format!("{name}, in one sentence."),
    })
    .collect()
}

#[story(
    description = "The gallery pick popover open on its NEW tab, the one source that has no name yet. Under the starter card sits the optional Project name field, prefilled with the board's own title — a piece and the board that runs it usually share a name, so leaving it is the common case and the hint says so. Typing a different name swaps the hint for one offer, ticked by default: name the board the same. Nothing here is a step — the one press still dispatches one Push, now carrying the project's name (and, when ticked, the board's rename) as parameters. The board's rename is only ever offered on a NEW project; an example or a library project pushed to the board never renames it."
)]
fn device_pick_popover_new_tab() -> Element {
    rsx! {
        section { class: "tw:min-h-[520px] tw:w-[420px] tw:p-4",
            div { class: "tw:flex tw:h-[30px] tw:min-w-0 tw:items-center tw:gap-1.5 tw:overflow-hidden tw:whitespace-nowrap",
                ProjectPickPopover {
                    offer: pick_popover_push(),
                    board_id: pick_popover_card().board_id,
                    board_title: pick_popover_card().title,
                    projects: pick_popover_library(),
                    examples: pick_popover_examples(),
                    initially_open: true,
                    initial_args: OfferArgs::new()
                        .with(PUSH_SOURCE_PARAM, "new:seeed/xiao-esp32-c6"),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "Rename, on the board card: the hardware bar's details, open, hold the one verb that acts on the ENTRY rather than the board — Rename, as an inline form prefilled with the card's current title (here the derived \"<board> · <Mon D>\" a flash minted). Submitting dispatches the model's own SetName; the name is Studio's (persisted to the registry) and is never written to the board. Beside it are the board's model, chip and id, Reset, and Forget apart in the danger zone. A new board's card has no Rename — a link that has not identified itself has no entry to name, and names itself through the board pick's name field instead."
)]
fn devices_card_menu_open() -> Element {
    let card = roster_fixture().roster.devices.remove(0);
    rsx! {
        section { class: "tw:min-h-[560px] tw:p-4",
            div { class: "tw:w-[400px]",
                StoryDeviceCard {
                    card,
                    open_uid: Some("dev000000daqf6dvvqz".to_string()),
                    projects: packages(),
                    examples: examples(),
                    menu_initially_open: true,
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The board's LINK counters (plan D13), in the connection bar's details: the lp-link counters the board reports on every heartbeat, in the board's own words — frames it had to send again, frames that reached it damaged, times the link restarted and went quiet, and the bytes it sent and received, each in the unit that keeps the number short. On a clean cable every count is 0; here the board has resent 3 of its 40 sent frames (well over DD2's 5 % floor) and restarted once, so those two wear the warning tone — a restart is notable at any count, a resend only once it clears the floor. The details float, so the fixed-height card pays nothing for them; a link that reports no counters (Bluetooth, a sim) shows none."
)]
fn devices_card_menu_link_counters() -> Element {
    let mut card = roster_fixture().roster.devices.remove(0);
    card.link_counters = Some(lpa_studio_core::DeviceLinkCounters {
        resends: 3,
        damaged: 0,
        resets: 1,
        stalls: 0,
        bytes_sent: 1_363_149,
        bytes_received: 38_912,
        frames_sent: 40,
        frames_received: 300,
    });
    rsx! {
        section { class: "tw:min-h-[640px] tw:p-4",
            div { class: "tw:w-[400px]",
                StoryBoardCard {
                    card,
                    open_uid: Some("dev000000daqf6dvvqz".to_string()),
                    projects: packages(),
                    examples: examples(),
                    details_open: Some(crate::app::board_card::CardPart::Bar(
                        lpa_studio_core::BarLayer::Connection,
                    )),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The gallery pick popover, open (P6, AC8). The trigger is ONE 30px control and the options live in a panel in the browser's top layer, so a library of forty projects can never make a card taller than the viewport (the reflow rule, AC2). Tabs are the three sources core's push_offer already groups, with their counts; the search box filters titles client-side; the cards are the gallery's own thumbs with their provenance, and a picked one wears the app-wide selection grammar (spectrum ring + wash + check). Picking closes the panel and updates the trigger — nothing is journaled until the CTA beside it dispatches the Push."
)]
fn device_pick_popover_open() -> Element {
    rsx! {
        section { class: "tw:min-h-[520px] tw:w-[420px] tw:p-4",
            div { class: "tw:flex tw:h-[30px] tw:min-w-0 tw:items-center tw:gap-1.5 tw:overflow-hidden tw:whitespace-nowrap",
                ProjectPickPopover {
                    offer: pick_popover_push(),
                    board_id: pick_popover_card().board_id,
                    board_title: pick_popover_card().title,
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
    description = "The board pick popover, open and filtered (P6, AC4; renderings P10). The chip the boot banner named narrows the served catalog, and the panel SAYS so — which chip, which source answered it, how many boards fit — with \"show all\" as the escape; the flash preflight's chip guard, not the filter, is what makes a wrong pick fail safely, which is what the foot line is for. Each tile now LEADS with the board as lpa-boards draws it — the same sidecar and the same renderer the boards page uses, turned a quarter turn and fitted to a 56px band, so a devkit lies along the band instead of standing in it as a sliver and tiles of a three-to-one height range still line their names up — over the name, its manufacturer and flash, and its family, marked green only where it matches the detected chip. The trigger's swatch carries the picked board's own silhouette. Two C6 boards fit, so nothing is preselected and the Flash verb waits: the pin map is written to the device, so the card never guesses. Above the foot line sits the optional Board name field: blank keeps the derived \"<board> · <Mon D>\" the app mints at flash (shown as the placeholder once a board is picked), and a typed name rides the Flash gesture itself — the only road a still-pending link's name can take, since it has no intent to rename until this gesture adopts it. The Update verb's pick carries no field: that board already has its name."
)]
fn device_board_pick_open() -> Element {
    board_pick_story(BoardPickMode::Row, ("esp32c6", ChipSource::BootBanner))
}

#[story(
    description = "Update's board pick, open (ruled 2026-09-04). An older LightPlayer's firmware bar offers Update, with no pick when its board is known. This is the other case — the bench classic: its hello reports board `?` (the board id comes from the manifest Studio stamps at flash, and this board was flashed from the CLI), the registry has no board, and a classic ESP32 chip fits several served boards — so the SAME Update is the picker's trigger, and the panel earns the detour with one line under its filter: \"This board hasn't said which board it is. Pick once; Studio stamps it at flash, and next time this is one click.\" Picking a board flashes it straight away: the verb was already pressed. The verb, the reason, and whether a pick is needed at all are decided in core (`firmware_verb`) and tested there; this panel only draws them."
)]
fn device_update_pick_open() -> Element {
    board_pick_story(BoardPickMode::Verb, ("esp32", ChipSource::BootBanner))
}

/// One 420px column with the board pick popover mounted open: Row is a
/// blank chip's `flash` (a pending link the boot banner named), Verb the
/// bench classic's `update-firmware` with its board unknown.
fn board_pick_story(mode: BoardPickMode, chip: (&str, ChipSource)) -> Element {
    let offer = match mode {
        BoardPickMode::Row => {
            let blank = PendingLinkView {
                device: DeviceId(3),
                firmware_face: lpa_studio_core::DeviceFirmwareFace::Blank,
                detected_chip: Some(chip.0.to_string()),
                ..roster_fixture().roster.pending[1].clone()
            };
            lpa_studio_core::flash_pending_offer(&blank, story_board_prefix(DeviceId(3)))
                .expect("a blank chip flashes")
        }
        BoardPickMode::Verb => {
            let (_, older_unknown, _) = firmware_face_fixtures()
                .into_iter()
                .find(|(label, _, _)| *label == "Older, board unknown")
                .expect("the bench classic");
            lpa_studio_core::update_firmware_offer(&older_unknown, story_board_prefix(DeviceId(3)))
                .expect("a running LightPlayer updates")
        }
    };
    rsx! {
        section { class: "tw:min-h-[420px] tw:w-[420px] tw:p-4",
            div { class: "tw:flex tw:h-[30px] tw:min-w-0 tw:items-center tw:gap-1.5 tw:overflow-hidden tw:whitespace-nowrap",
                BoardPickPopover {
                    offer,
                    chip: Some((chip.0.to_string(), chip.1)),
                    mode,
                    initially_open: true,
                    on_action: |_| {},
                }
            }
        }
    }
}
