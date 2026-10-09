//! Home page stories: the page in the states the visual gate judges.
//!
//! One page, drawn for a newcomer (`home_landing_stories.rs`), a signed-out
//! visitor, a guest, a signed-in account, a full library, the list view,
//! each tab, and a browser that has only remembered boards. The older
//! stories (`home_landing_stories.rs`, `home_gallery_stories.rs`) keep their
//! names where the state is the same as the old pages' and now draw this
//! page, so the stories comment reads those as before/after.
//!
//! (The file sits beside the other home stories rather than in `page/`: a
//! story file may be at most one directory deep under `src/app/`.)
//!
//! Every story hands its library and roster to [`StoryHomePage`], which
//! asks core's own builder for the sections, so a story cannot show a page
//! core would not produce. The fixtures here stamp each project's boards
//! the way `StudioController::home_view` does ([`stamp_on_boards`]), from a
//! real join ([`BoardProjects`]). A fixed clock, no leased previews, and the
//! Connect a board section pinned to Chrome on a computer keep a capture
//! deterministic. The sign-in line and the archive drawer read the cloud
//! session from context, which the three account stories provide and the
//! others leave out (without it both draw nothing: the house rule).

use dioxus::prelude::*;
use lpa_studio_core::app::library::PackageHealth;
use lpa_studio_core::{
    BoardPlays, BoardProjects, DeviceCardFeedView, DeviceId, DeviceLoadedProject, DeviceRosterView,
    DeviceView, FeedLiveness, RosterView, UiHomeTab, UiHomeView, UiPackageCard, UiRuntimeBand,
    stamp_on_boards,
};
use lpa_studio_web_story_macros::story;
use lpc_cloud_api::{LoginOptionsInfo, MeInfo, OidcOption};
use lpc_history::{PrefixedUid, UidPrefix};

use crate::app::home::device_offer_story_fixtures::StoryHomePage;
use crate::app::home::home_gallery_stories::{
    STORY_NOW, examples, live_card_lamp_frame, roster_fixture, sim_card_view, thumb_lamp_frame,
};
use crate::app::home::home_landing_stories::{newcomer_home, pins};
use crate::app::home::page::home_view_mode::HomeViewMode;
use crate::cloud::CloudSession;

#[story(
    description = "The home page for someone who is not signed in and has something to keep: one board, a few projects, and the quiet sign-in line above the tabs — \"Sign in to unlock your boards from any browser.\" and the sign-in word (one provider, so the word goes straight to it; several would open the chooser). Compare with home_page_guest and home_page_signed_in, which draw the same page."
)]
fn home_page_signed_out() -> Element {
    page(everyday_home(), Some(signed_out()), None, None)
}

#[story(
    description = "The same page for a guest (a browser-held account with no login to come back through): the same sign-in line as a signed-out visitor, because a guest is exactly who it is for."
)]
fn home_page_guest() -> Element {
    page(everyday_home(), Some(guest()), None, None)
}

#[story(
    description = "The same page for a real account: no sign-in line. The contrast to home_page_signed_out and home_page_guest — the line is the only difference."
)]
fn home_page_signed_in() -> Element {
    page(everyday_home(), Some(account()), None, None)
}

#[story(
    description = "A first visit by a signed-out visitor: no sign-in line, no tabs and no switch. A newcomer has nothing to keep yet, so the prompt would be noise; Connect a board is the first section, with its one hint line. This is the page a visitor to lightplayer.app meets (home_landing's newcomer has no session)."
)]
fn home_page_newcomer_signed_out() -> Element {
    page(newcomer_home(), Some(signed_out()), None, None)
}

#[story(
    description = "A full library, signed in, on the All tab: a link still being identified and a connected board and a sim under Online boards (the sim and the board both play the porch sign), Connect a board, two offline boards each with its last picture (dimmed, with how long ago), the \"Unlocking your boards\" fold, Other projects (the three that no board plays, newest first) with the add row, Your patterns, then the examples. The tab strip counts: Boards 5 · Projects 5 · Patterns 3."
)]
fn home_page_full_library() -> Element {
    page(full_home(), Some(account()), None, None)
}

#[story(
    description = "The full library in list mode: every board and project is a row, not a card. The same sections in the same order; a board's row carries its state, its project and its verbs, a project's its boards and its age."
)]
fn home_page_list_view() -> Element {
    page(full_home(), Some(account()), None, Some(HomeViewMode::List))
}

#[story(
    description = "The Boards tab: Online boards, Connect a board, Offline boards and the \"Unlocking your boards\" fold, and nothing of the library or the catalog."
)]
fn home_page_tab_boards() -> Element {
    page(full_home(), Some(account()), Some(UiHomeTab::Boards), None)
}

#[story(
    description = "The Projects tab: every library project (so a board's project, even an offline board's, keeps its Rename, Duplicate, Download and Delete), each saying which boards play it (\"On Luna's porch sign, Desk sim\"), then the example projects. Other projects, the unattached ones, is the All tab's section."
)]
fn home_page_tab_projects() -> Element {
    page(
        full_home(),
        Some(account()),
        Some(UiHomeTab::Projects),
        None,
    )
}

#[story(description = "The Patterns tab: Your patterns as cards, then the example patterns.")]
fn home_page_tab_patterns() -> Element {
    page(
        full_home(),
        Some(account()),
        Some(UiHomeTab::Patterns),
        None,
    )
}

#[story(
    description = "A browser that remembers two boards and nothing else (not a first visit: the tabs and the switch are there). Online boards is empty and so not drawn; Offline boards holds the two, each with its last picture and Reconnect; Connect a board is the way back. Other projects is its add row."
)]
fn home_page_only_offline_boards() -> Element {
    page(home(&[Board::Garage, Board::Truck], 0, 0), None, None, None)
}

/// The page, under the cloud session a story asks for (`None`: no context,
/// as in the app's earlier stories).
fn page(
    home: UiHomeView,
    session: Option<CloudSession>,
    tab: Option<UiHomeTab>,
    mode: Option<HomeViewMode>,
) -> Element {
    let body = rsx! {
        section { class: "tw:p-4",
            StoryHomePage {
                home,
                now_secs: Some(STORY_NOW),
                initial_tab: tab,
                initial_mode: mode,
                connect_pins: pins(),
                on_action: |_| {},
            }
        }
    };
    match session {
        Some(session) => rsx! {
            StorySession { session, {body} }
        },
        None => body,
    }
}

/// Provides the chrome's `Signal<CloudSession>` to the page below.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StorySession(session: CloudSession, children: Element) -> Element {
    use_context_provider(move || Signal::new(session));
    rsx! { {children} }
}

// --- The fixtures -------------------------------------------------------

/// The boards a story's roster is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Board {
    /// A fresh plug, still being identified (a blank chip).
    Pending,
    /// A board on the bus, playing the porch sign.
    Luna,
    /// A sim in this tab, playing the porch sign.
    Sim,
    /// A remembered board that last played the evening glow.
    Garage,
    /// A remembered board nothing is known about.
    Truck,
}

const PORCH_UID: &str = "prj3fKq8Zr21bTxYw0AhVmDpe";
const PORCH_SLUG: &str = "2026-07-02-0930-porch-sign";
const GLOW_UID: &str = "prj9sLm2Xc44dQnUv7BgWkEyt";

/// Five projects and three patterns, newest first. The porch sign is played
/// by the connected board and the sim, the evening glow by the garage board;
/// the other three are on no board.
fn library() -> Vec<UiPackageCard> {
    let day = 86_400.0;
    vec![
        project(PORCH_UID, PORCH_SLUG, STORY_NOW - 2.0 * 3_600.0, None),
        project(
            GLOW_UID,
            "2026-07-04-1102-evening-glow",
            STORY_NOW - 5.0 * day,
            Some("Remixed from Basic"),
        ),
        project(
            "prj1aBc3De56fGhIj8KlMnOpq",
            "2026-05-28-1740-porch-sign",
            STORY_NOW - 40.0 * day,
            Some("Forked from 2026-07-02-0930-porch-sign"),
        ),
        project(
            "prj7WxYz1Ab23cDeF4gHiJkLm",
            "2026-06-11-0815-hallway-strip",
            STORY_NOW - 12.0 * day,
            None,
        ),
        project(
            "prj2QrSt5Uv67wXyZ8aBcDeFg",
            "2026-04-03-2010-shelf-light",
            STORY_NOW - 90.0 * day,
            None,
        ),
        pattern(
            "prj4HiJk9Lm01nOpQ2rStUvWx",
            "2026-07-06-0745-meteor-shower",
            STORY_NOW - day,
        ),
        pattern(
            "prj6YzAb3Cd45eFgH6iJkLmNo",
            "2026-06-20-1930-slow-plasma",
            STORY_NOW - 18.0 * day,
        ),
        pattern(
            "prj8PqRs7Tu89vWxY0zAbCdEf",
            "2026-05-02-1215-ember",
            STORY_NOW - 60.0 * day,
        ),
    ]
}

fn project(uid: &str, slug: &str, saved: f64, provenance: Option<&str>) -> UiPackageCard {
    UiPackageCard {
        uid: uid.to_string(),
        kind: "Module".to_string(),
        project_kind: "General".to_string(),
        exports: Vec::new(),
        slug: slug.to_string(),
        last_saved_at: Some(saved),
        provenance: provenance.map(str::to_string),
        on_boards: Vec::new(),
        open_elsewhere: false,
        target: None,
        health: PackageHealth::Ready,
    }
}

fn pattern(uid: &str, slug: &str, saved: f64) -> UiPackageCard {
    UiPackageCard {
        project_kind: "Pattern".to_string(),
        exports: vec!["effect".to_string()],
        ..project(uid, slug, saved, None)
    }
}

/// The roster of `boards`, in the order given, with the join that says what
/// each plays.
fn roster(boards: &[Board]) -> DeviceRosterView {
    let base = roster_fixture();
    let mut pending = Vec::new();
    let mut devices = Vec::new();
    let mut feeds: Vec<(DeviceId, DeviceCardFeedView)> = Vec::new();
    let mut bands: Vec<(DeviceId, UiRuntimeBand)> = Vec::new();
    let mut answers: Vec<(DeviceId, BoardPlays)> = Vec::new();
    let mut open_addresses: Vec<(u64, String)> = base.open_addresses.clone().into_iter().collect();
    for board in boards {
        match board {
            Board::Pending => pending.push(base.roster.pending[1].clone()),
            Board::Luna => {
                let mut card = base.roster.devices[0].clone();
                card.loaded_project = DeviceLoadedProject::Running {
                    label: PORCH_SLUG.to_string(),
                };
                feeds.push((card.id, live_feed()));
                answers.push((card.id, given(PORCH_UID, true)));
                devices.push(card);
            }
            Board::Sim => {
                let mut card = sim_card_view(21, "Desk sim", "seeed/xiao-esp32-c6");
                card.loaded_project = DeviceLoadedProject::Running {
                    label: PORCH_SLUG.to_string(),
                };
                bands.push((
                    card.id,
                    UiRuntimeBand::sim("seeed/xiao-esp32-c6", Some("cpu")),
                ));
                feeds.push((card.id, live_feed()));
                open_addresses.push((card.id.0, "dev000000daqf6dvvqx".to_string()));
                answers.push((card.id, given(PORCH_UID, true)));
                devices.push(card);
            }
            Board::Garage => {
                let card = base.roster.devices[4].clone();
                feeds.push((card.id, last_picture(3.0 * 3_600.0)));
                answers.push((card.id, given(GLOW_UID, false)));
                devices.push(card);
            }
            Board::Truck => {
                let card = DeviceView {
                    id: DeviceId(6),
                    title: "Truck".to_string(),
                    freshness_label: Some("last heard 1 day ago".to_string()),
                    identity_label: Some("dev000000000truck01".to_string()),
                    ..base.roster.devices[4].clone()
                };
                feeds.push((card.id, last_picture(26.0 * 3_600.0)));
                devices.push(card);
            }
        }
    }
    DeviceRosterView {
        transport_available: true,
        usb_available: true,
        board_projects: BoardProjects::from_answers(answers),
        feeds: feeds.into_iter().collect(),
        runtime_bands: bands.into_iter().collect(),
        open_addresses: open_addresses.into_iter().collect(),
        roster: RosterView { pending, devices },
        ..DeviceRosterView::default()
    }
}

/// What the join says a board plays: a library project, given and verified.
fn given(project_uid: &str, at_head: bool) -> BoardPlays {
    BoardPlays::Given {
        project_uid: project_uid.to_string(),
        at_head,
    }
}

/// A board's live picture: the canned 72-lamp sign, a second old.
fn live_feed() -> DeviceCardFeedView {
    DeviceCardFeedView {
        frame: Some(live_card_lamp_frame()),
        frame_age_secs: Some(1.0),
        engine_fps: Some(43),
        liveness: FeedLiveness::Live,
    }
}

/// The picture an offline board kept, `age_secs` old.
fn last_picture(age_secs: f64) -> DeviceCardFeedView {
    DeviceCardFeedView {
        frame: Some(thumb_lamp_frame()),
        frame_age_secs: Some(age_secs),
        engine_fps: None,
        liveness: FeedLiveness::Offline,
    }
}

/// The page's view for `boards`, the first `projects` general projects and
/// the first `patterns` patterns of the library: stamped and ready for
/// [`StoryHomePage`], which asks core for the sections.
fn home(boards: &[Board], projects: usize, patterns: usize) -> UiHomeView {
    let (general, pattern): (Vec<_>, Vec<_>) =
        library().into_iter().partition(|card| !card.is_pattern());
    let mut cards: Vec<UiPackageCard> = general.into_iter().take(projects).collect();
    cards.extend(pattern.into_iter().take(patterns));
    let devices = roster(boards);
    stamp_on_boards(&mut cards, &devices);
    UiHomeView {
        projects: cards,
        examples: examples(),
        devices,
        sections: Default::default(),
        library_available: true,
        opening: None,
        issue: None,
    }
}

/// A sim playing the porch sign, and a three-project library: the README's
/// home shot.
pub(crate) fn one_sim_home() -> UiHomeView {
    home(&[Board::Sim], 3, 0)
}

/// A board, a few projects and a pattern: what the account stories draw.
fn everyday_home() -> UiHomeView {
    home(&[Board::Luna, Board::Garage], 3, 1)
}

/// Everything: five boards, five projects, three patterns.
fn full_home() -> UiHomeView {
    home(
        &[
            Board::Pending,
            Board::Luna,
            Board::Sim,
            Board::Garage,
            Board::Truck,
        ],
        5,
        3,
    )
}

// --- Cloud sessions -----------------------------------------------------

fn signed_out() -> CloudSession {
    CloudSession::Anonymous {
        options: Some(options()),
    }
}

fn guest() -> CloudSession {
    CloudSession::SignedIn {
        me: me(true),
        options: Some(options()),
    }
}

fn account() -> CloudSession {
    CloudSession::SignedIn {
        me: me(false),
        options: Some(options()),
    }
}

/// One provider, so the sign-in word goes straight to it.
fn options() -> LoginOptionsInfo {
    LoginOptionsInfo {
        oidc: vec![OidcOption {
            id: "google".to_string(),
            label: "Google".to_string(),
            start_path: "/auth/google".to_string(),
        }],
        dev_picker: None,
    }
}

fn me(anonymous: bool) -> MeInfo {
    MeInfo {
        uid: PrefixedUid::mint(UidPrefix::User, &[7; 16]),
        email: "luna@example.com".to_string(),
        display_name: "Luna".to_string(),
        given_name: None,
        family_name: None,
        picture_url: None,
        provider_label: match anonymous {
            true => "Guest",
            false => "Google",
        }
        .to_string(),
        created_at: 1_752_000_000_000.0,
        anonymous,
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiHomeBoardKind, build_home_sections};

    use super::*;
    use crate::app::home::device_offer_story_fixtures::with_core_sections;

    #[test]
    fn every_page_is_what_core_builds_for_its_library_and_roster() {
        for (name, view) in pages() {
            let shown = with_core_sections(view.clone());
            assert_eq!(
                shown.sections,
                build_home_sections(&view.projects, &view.devices),
                "{name}: a story must not hand-build which board or project sits where"
            );
        }
    }

    #[test]
    fn every_project_names_the_boards_the_join_says_play_it() {
        for (name, view) in pages() {
            let titles: Vec<(DeviceId, &str)> = view
                .devices
                .roster
                .devices
                .iter()
                .map(|device| (device.id, device.title.as_str()))
                .collect();
            for card in &view.projects {
                let expected: Vec<String> = view
                    .devices
                    .board_projects
                    .boards_playing(&card.uid)
                    .into_iter()
                    .filter_map(|board| {
                        titles
                            .iter()
                            .find(|(id, _)| *id == board)
                            .map(|(_, title)| title.to_string())
                    })
                    .collect();
                assert_eq!(card.on_boards, expected, "{name}: {}", card.slug);
            }
        }
    }

    #[test]
    fn the_full_library_has_the_shape_the_story_claims() {
        let view = with_core_sections(full_home());
        let sections = &view.sections;
        assert!(!sections.newcomer);
        assert_eq!(
            sections
                .online
                .iter()
                .map(|board| board.kind)
                .collect::<Vec<_>>(),
            [
                UiHomeBoardKind::Pending,
                UiHomeBoardKind::Connected,
                UiHomeBoardKind::Connected
            ],
            "a link being identified, a board and a sim"
        );
        assert_eq!(sections.offline.len(), 2);
        assert_eq!(sections.other_projects.len(), 3);
        assert_eq!(sections.projects.len(), 5);
        assert_eq!(sections.patterns.len(), 3);
        let porch = view
            .projects
            .iter()
            .find(|card| card.uid == PORCH_UID)
            .expect("the porch sign is in the library");
        assert_eq!(porch.on_boards, ["Luna's porch sign", "Desk sim"]);
        assert!(
            !sections.other_projects.contains(&GLOW_UID.to_string()),
            "a project on an offline board is on a board"
        );
    }

    #[test]
    fn the_offline_page_is_not_a_first_visit() {
        let view = with_core_sections(home(&[Board::Garage, Board::Truck], 0, 0));
        assert!(!view.sections.newcomer);
        assert!(view.sections.online.is_empty());
        assert_eq!(view.sections.offline.len(), 2);
        assert!(
            view.devices
                .feeds
                .values()
                .all(|feed| feed.liveness == FeedLiveness::Offline && feed.frame.is_some()),
            "each remembered board carries its last picture"
        );
    }

    #[test]
    fn the_newcomer_is_a_first_visit() {
        let view = with_core_sections(newcomer_home());
        assert!(view.sections.newcomer);
    }

    /// Every page this file's stories draw, by name.
    fn pages() -> Vec<(&'static str, UiHomeView)> {
        vec![
            ("everyday", everyday_home()),
            ("one sim", one_sim_home()),
            ("full", full_home()),
            ("only offline", home(&[Board::Garage, Board::Truck], 0, 0)),
            ("newcomer", newcomer_home()),
        ]
    }
}
