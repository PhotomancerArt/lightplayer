//! Building [`UiHomeSections`] from the library and the roster.
//!
//! The Online/Offline split is today's [`split_roster`], not the join: a
//! board is offline when the roster says it is, and the roster's own order
//! stands. The *join* ([`BoardProjects`](crate::BoardProjects), carried on
//! `DeviceRosterView::board_projects`) answers the two questions the page
//! asks of it, and this builder never decides either again:
//!
//! - **J1**, a project's boards: [`stamp_on_boards`] writes the names onto
//!   each [`UiPackageCard`], and "Other projects" is
//!   [`BoardProjects::on_no_board`](crate::BoardProjects::on_no_board);
//! - **J2**, a board's project: [`BoardProjects::plays`](crate::BoardProjects::plays),
//!   named by the library card when it names one, else by the board's own
//!   report.

use std::collections::BTreeMap;

use crate::{BoardPlays, DeviceId, DeviceRosterView, OfferPath, split_roster};

use super::ui_home_sections::{UiHomeBoard, UiHomeBoardKind, UiHomeConnect, UiHomeSections};
use super::ui_package_card::UiPackageCard;

/// The Connect a board section's verbs under `devices/`, in the order the
/// section draws them.
const CONNECT_VERBS: [&str; 3] = ["connect-usb", "connect-ble", "connect-wifi-address"];
/// "Start a board here": published only when the page has a transport to
/// start a board on (`StudioController::publish_device_offers`).
const NEW_SIM_VERB: &str = "new-sim";

/// Fill the sections. `projects` are the library's cards (the join is on
/// `devices`, in `devices.board_projects`); a library that did not mount
/// is simply an empty list.
pub fn build_home_sections(
    projects: &[UiPackageCard],
    devices: &DeviceRosterView,
) -> UiHomeSections {
    let join = &devices.board_projects;
    let by_uid: BTreeMap<&str, &UiPackageCard> = projects
        .iter()
        .map(|card| (card.uid.as_str(), card))
        .collect();
    let project_of = |board: DeviceId| -> Option<String> {
        match join.plays(board) {
            BoardPlays::Open { project_uid } | BoardPlays::Given { project_uid, .. } => by_uid
                .get(project_uid.as_str())
                .map(|card| card.slug.clone()),
            BoardPlays::Running { label } => Some(label.clone()),
            BoardPlays::Nothing | BoardPlays::Unknown => None,
        }
    };

    let split = split_roster(devices);
    let state_of: BTreeMap<DeviceId, &str> = devices
        .roster
        .devices
        .iter()
        .map(|device| (device.id, device.state_label.as_str()))
        .collect();

    let mut online: Vec<UiHomeBoard> = devices
        .roster
        .pending
        .iter()
        .map(|pending| UiHomeBoard {
            id: pending.device,
            kind: UiHomeBoardKind::Pending,
            title: pending.title.clone(),
            status: pending.state_label.clone(),
            project: None,
            row_verbs: Vec::new(),
        })
        .collect();
    online.extend(split.connected.iter().map(|device| UiHomeBoard {
        id: device.id,
        kind: UiHomeBoardKind::Connected,
        title: device.title.clone(),
        status: device.state_label.clone(),
        project: project_of(device.id),
        // A plugged-in board nobody has connected yet offers Connect; once
        // connected its row has nothing more to say.
        row_verbs: match device.status == lpa_devices::device::DeviceStatus::Attached {
            true => vec!["connect"],
            false => Vec::new(),
        },
    }));

    let offline: Vec<UiHomeBoard> = split
        .remembered
        .iter()
        .map(|remembered| UiHomeBoard {
            id: remembered.id,
            kind: UiHomeBoardKind::Remembered,
            title: remembered.title.clone(),
            status: remembered
                .last_seen_label
                .clone()
                .or_else(|| state_of.get(&remembered.id).map(|state| state.to_string()))
                .unwrap_or_default(),
            project: project_of(remembered.id),
            row_verbs: vec!["connect-wifi", "reconnect"],
        })
        .collect();

    // Every pattern is a pattern wherever it plays; every other project is
    // in `projects`, and in `other_projects` too when no board plays it. A
    // package that would not open is a general project here: it is listed,
    // never dropped.
    let (pattern_cards, project_cards): (Vec<&UiPackageCard>, Vec<&UiPackageCard>) =
        projects.iter().partition(|card| card.is_pattern());
    let other_cards: Vec<&UiPackageCard> = join
        .on_no_board(projects)
        .into_iter()
        .filter(|card| !card.is_pattern())
        .collect();

    let newcomer = online.is_empty()
        && offline.is_empty()
        && project_cards.is_empty()
        && pattern_cards.is_empty();
    UiHomeSections {
        online,
        offline,
        connect: UiHomeConnect {
            offers: connect_offers(devices.transport_available),
            welcome: newcomer,
        },
        other_projects: newest_first(other_cards),
        projects: newest_first(project_cards),
        patterns: newest_first(pattern_cards),
        newcomer,
    }
}

/// Write each project's boards onto its card: the names of the boards the
/// join says play it, in roster order (empty when none does).
pub fn stamp_on_boards(projects: &mut [UiPackageCard], devices: &DeviceRosterView) {
    let titles: BTreeMap<DeviceId, &str> = devices
        .roster
        .devices
        .iter()
        .map(|device| (device.id, device.title.as_str()))
        .collect();
    for card in projects {
        card.on_boards = devices
            .board_projects
            .boards_playing(&card.uid)
            .into_iter()
            .filter_map(|board| titles.get(&board).map(|title| title.to_string()))
            .collect();
    }
}

/// The offers the Connect a board section draws. `devices/new-sim` rides
/// along only where the page has a transport, as it is only published then.
fn connect_offers(transport_available: bool) -> Vec<OfferPath> {
    let mut offers: Vec<OfferPath> = CONNECT_VERBS
        .into_iter()
        .map(|verb| OfferPath::devices().child(verb))
        .collect();
    if transport_available {
        offers.push(OfferPath::devices().child(NEW_SIM_VERB));
    }
    offers
}

/// The cards' uids, newest saved first; ties by slug. A card with no save
/// time sorts last.
fn newest_first(mut cards: Vec<&UiPackageCard>) -> Vec<String> {
    cards.sort_by(|a, b| {
        let saved = |card: &UiPackageCard| card.last_saved_at.unwrap_or(f64::NEG_INFINITY);
        saved(b)
            .total_cmp(&saved(a))
            .then_with(|| a.slug.cmp(&b.slug))
    });
    cards.into_iter().map(|card| card.uid.clone()).collect()
}

#[cfg(test)]
mod tests {
    use lpa_devices::device::DeviceStatus;
    use lpa_devices::view::{FirmwareFace, LoadedProject, PendingLinkView};

    use crate::app::library::PackageHealth;
    use crate::{BoardProjects, DeviceView};

    use super::*;

    #[test]
    fn a_newcomer_has_no_board_and_no_project() {
        let sections = build_home_sections(&[], &roster(Vec::new(), Vec::new()));
        assert!(sections.newcomer);
        assert!(sections.connect.welcome);
        assert!(sections.online.is_empty() && sections.offline.is_empty());
        assert!(sections.projects.is_empty() && sections.patterns.is_empty());
    }

    #[test]
    fn one_board_and_nothing_else_is_not_a_newcomer() {
        let devices = roster(vec![board(1, "Desk C6", DeviceStatus::Ready)], Vec::new());
        let sections = build_home_sections(&[], &devices);
        assert!(!sections.newcomer);
        assert!(!sections.connect.welcome);
        assert_eq!(titles(&sections.online), ["Desk C6"]);
    }

    #[test]
    fn remembered_boards_alone_are_not_a_newcomer() {
        let devices = roster(vec![board(1, "Truck", DeviceStatus::Offline)], Vec::new());
        let sections = build_home_sections(&[], &devices);
        assert!(!sections.newcomer);
        assert!(sections.online.is_empty());
        assert_eq!(titles(&sections.offline), ["Truck"]);
        assert_eq!(sections.offline[0].kind, UiHomeBoardKind::Remembered);
    }

    #[test]
    fn a_library_with_projects_and_no_boards_is_not_a_newcomer() {
        let sections = build_home_sections(&[card("alpha", 1.0)], &roster(Vec::new(), Vec::new()));
        assert!(!sections.newcomer);
        let sections =
            build_home_sections(&[pattern("ember", 1.0)], &roster(Vec::new(), Vec::new()));
        assert!(!sections.newcomer, "a pattern is library content too");
    }

    #[test]
    fn a_pending_link_comes_first_then_the_connected_boards_in_roster_order() {
        let devices = roster(
            vec![
                board(7, "Second", DeviceStatus::Ready),
                board(3, "First", DeviceStatus::Ready),
            ],
            vec![pending(9, "New board")],
        );
        let sections = build_home_sections(&[], &devices);
        assert_eq!(titles(&sections.online), ["New board", "Second", "First"]);
        assert_eq!(sections.online[0].kind, UiHomeBoardKind::Pending);
        assert_eq!(sections.online[0].id, DeviceId(9));
        assert_eq!(sections.online[1].kind, UiHomeBoardKind::Connected);
        assert_eq!(sections.online[0].project, None);
    }

    #[test]
    fn the_split_is_the_rosters_own_offline_status() {
        let devices = roster(
            vec![
                board(1, "Here", DeviceStatus::Ready),
                board(2, "Gone", DeviceStatus::Offline),
                board(3, "Plugged", DeviceStatus::Attached),
            ],
            Vec::new(),
        );
        let sections = build_home_sections(&[], &devices);
        assert_eq!(titles(&sections.online), ["Here", "Plugged"]);
        assert_eq!(titles(&sections.offline), ["Gone"]);
    }

    #[test]
    fn row_verbs_are_names_the_web_resolves() {
        let devices = roster(
            vec![
                board(1, "Ready", DeviceStatus::Ready),
                board(2, "Attached", DeviceStatus::Attached),
                board(3, "Gone", DeviceStatus::Offline),
            ],
            vec![pending(4, "Identifying")],
        );
        let sections = build_home_sections(&[], &devices);
        assert_eq!(sections.online[0].row_verbs, Vec::<&str>::new(), "pending");
        assert_eq!(sections.online[1].row_verbs, Vec::<&str>::new(), "ready");
        assert_eq!(sections.online[2].row_verbs, ["connect"], "attached only");
        assert_eq!(sections.offline[0].row_verbs, ["connect-wifi", "reconnect"]);
    }

    #[test]
    fn a_boards_status_is_the_rosters_words() {
        let mut gone = board(2, "Truck", DeviceStatus::Offline);
        gone.freshness_label = Some("last heard 2 weeks ago".to_string());
        let silent = board(3, "Shed", DeviceStatus::Offline);
        let devices = roster(
            vec![board(1, "Desk C6", DeviceStatus::Ready), gone, silent],
            vec![pending(4, "Identifying")],
        );
        let sections = build_home_sections(&[], &devices);
        assert_eq!(sections.online[0].status, "Identifying\u{2026}");
        assert_eq!(sections.online[1].status, "Ready");
        assert_eq!(sections.offline[0].status, "last heard 2 weeks ago");
        assert_eq!(
            sections.offline[1].status, "Offline",
            "no freshness: the state"
        );
    }

    #[test]
    fn a_board_names_the_project_the_join_says_it_plays_else_its_own_label() {
        let alpha = card("alpha", 1.0);
        let mut devices = roster(
            vec![
                board(1, "Open", DeviceStatus::Ready),
                board(2, "Given", DeviceStatus::Ready),
                board(3, "Running", DeviceStatus::Ready),
                board(4, "Nothing", DeviceStatus::Ready),
                board(5, "Unknown", DeviceStatus::Ready),
                board(6, "Truck", DeviceStatus::Offline),
            ],
            Vec::new(),
        );
        devices.board_projects = BoardProjects::from_answers([
            (
                DeviceId(1),
                BoardPlays::Open {
                    project_uid: alpha.uid.clone(),
                },
            ),
            (
                DeviceId(2),
                BoardPlays::Given {
                    project_uid: alpha.uid.clone(),
                    at_head: false,
                },
            ),
            (
                DeviceId(3),
                BoardPlays::Running {
                    label: "studio".to_string(),
                },
            ),
            (DeviceId(4), BoardPlays::Nothing),
            (DeviceId(5), BoardPlays::Unknown),
            (
                DeviceId(6),
                BoardPlays::Given {
                    project_uid: alpha.uid.clone(),
                    at_head: true,
                },
            ),
        ]);
        let sections = build_home_sections(&[alpha.clone()], &devices);
        let plays: Vec<Option<&str>> = sections
            .online
            .iter()
            .map(|board| board.project.as_deref())
            .collect();
        assert_eq!(
            plays,
            [
                Some(alpha.slug.as_str()),
                Some(alpha.slug.as_str()),
                Some("studio"),
                None,
                None
            ]
        );
        assert_eq!(
            sections.offline[0].project.as_deref(),
            Some(alpha.slug.as_str()),
            "an offline board keeps playing what it was given (J2)"
        );
    }

    #[test]
    fn a_project_two_boards_play_is_on_both_and_in_projects_not_in_other_projects() {
        let alpha = card("alpha", 3.0);
        let beta = card("beta", 2.0);
        let mut devices = roster(
            vec![
                board(1, "Desk C6", DeviceStatus::Ready),
                board(2, "Porch", DeviceStatus::Ready),
            ],
            Vec::new(),
        );
        devices.board_projects = BoardProjects::from_answers([
            (DeviceId(1), given(&alpha)),
            (DeviceId(2), given(&alpha)),
        ]);
        let mut cards = vec![alpha.clone(), beta.clone()];
        stamp_on_boards(&mut cards, &devices);
        assert_eq!(cards[0].on_boards, ["Desk C6", "Porch"]);
        assert!(cards[1].on_boards.is_empty());

        let sections = build_home_sections(&cards, &devices);
        assert_eq!(sections.projects, [alpha.uid.clone(), beta.uid.clone()]);
        assert_eq!(sections.other_projects, [beta.uid.clone()]);
    }

    #[test]
    fn a_project_on_an_offline_board_is_still_on_a_board() {
        let alpha = card("alpha", 1.0);
        let mut devices = roster(vec![board(1, "Truck", DeviceStatus::Offline)], Vec::new());
        devices.board_projects = BoardProjects::from_answers([(DeviceId(1), given(&alpha))]);
        let mut cards = vec![alpha.clone()];
        stamp_on_boards(&mut cards, &devices);
        assert_eq!(cards[0].on_boards, ["Truck"]);
        let sections = build_home_sections(&cards, &devices);
        assert!(sections.other_projects.is_empty());
        assert_eq!(sections.projects, [alpha.uid]);
    }

    #[test]
    fn a_pattern_is_only_in_patterns() {
        let project = card("alpha", 1.0);
        let ember = pattern("ember", 2.0);
        let sections = build_home_sections(
            &[project.clone(), ember.clone()],
            &roster(Vec::new(), Vec::new()),
        );
        assert_eq!(sections.patterns, [ember.uid]);
        assert_eq!(sections.projects, [project.uid.clone()]);
        assert_eq!(sections.other_projects, [project.uid]);
    }

    #[test]
    fn a_pattern_a_board_plays_stays_a_pattern() {
        let ember = pattern("ember", 2.0);
        let mut devices = roster(vec![board(1, "Desk C6", DeviceStatus::Ready)], Vec::new());
        devices.board_projects = BoardProjects::from_answers([(DeviceId(1), given(&ember))]);
        let mut cards = vec![ember.clone()];
        stamp_on_boards(&mut cards, &devices);
        assert_eq!(cards[0].on_boards, ["Desk C6"]);
        let sections = build_home_sections(&cards, &devices);
        assert_eq!(sections.patterns, [ember.uid]);
        assert!(sections.projects.is_empty() && sections.other_projects.is_empty());
    }

    #[test]
    fn the_lists_run_newest_saved_first_and_ties_by_slug() {
        let cards = [
            card("old", 1.0),
            card("tie-b", 5.0),
            card("new", 9.0),
            card("tie-a", 5.0),
            UiPackageCard {
                last_saved_at: None,
                ..card("never", 0.0)
            },
        ];
        let sections = build_home_sections(&cards, &roster(Vec::new(), Vec::new()));
        let slugs: Vec<&str> = sections
            .projects
            .iter()
            .map(|uid| {
                cards
                    .iter()
                    .find(|card| &card.uid == uid)
                    .map(|card| card.slug.as_str())
                    .unwrap()
            })
            .collect();
        assert_eq!(slugs[..3], ["new", "tie-a", "tie-b"][..]);
        assert_eq!(slugs[3], "old");
        assert_eq!(slugs[4], "never", "a card with no save time is last");
    }

    #[test]
    fn a_blocked_package_stays_listed() {
        let blocked = UiPackageCard {
            health: PackageHealth::Blocked {
                headline: "Format 2 — too old for this Studio".to_string(),
                remedy: "Export it.".to_string(),
            },
            ..card("stale", 1.0)
        };
        let sections = build_home_sections(
            &[blocked.clone(), card("alpha", 2.0)],
            &roster(Vec::new(), Vec::new()),
        );
        assert_eq!(sections.projects.len(), 2);
        assert!(sections.projects.contains(&blocked.uid));
        assert!(sections.other_projects.contains(&blocked.uid));
    }

    #[test]
    fn an_unavailable_library_counts_as_empty() {
        // The builder reads cards; a library that did not mount hands it
        // none, and the board half still stands.
        let devices = roster(vec![board(1, "Desk C6", DeviceStatus::Ready)], Vec::new());
        let sections = build_home_sections(&[], &devices);
        assert!(sections.projects.is_empty() && sections.patterns.is_empty());
        assert!(sections.other_projects.is_empty());
        assert!(!sections.newcomer);
    }

    #[test]
    fn the_connect_offers_follow_the_transport() {
        let mut devices = roster(Vec::new(), Vec::new());
        devices.transport_available = false;
        let paths = |devices: &DeviceRosterView| -> Vec<String> {
            build_home_sections(&[], devices)
                .connect
                .offers
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        assert_eq!(
            paths(&devices),
            [
                "devices/connect-usb",
                "devices/connect-ble",
                "devices/connect-wifi-address"
            ]
        );
        devices.transport_available = true;
        assert_eq!(
            paths(&devices),
            [
                "devices/connect-usb",
                "devices/connect-ble",
                "devices/connect-wifi-address",
                "devices/new-sim"
            ]
        );
    }

    // -----------------------------------------------------------------
    // Fixtures
    // -----------------------------------------------------------------

    fn titles(boards: &[UiHomeBoard]) -> Vec<&str> {
        boards.iter().map(|board| board.title.as_str()).collect()
    }

    fn given(card: &UiPackageCard) -> BoardPlays {
        BoardPlays::Given {
            project_uid: card.uid.clone(),
            at_head: true,
        }
    }

    fn roster(devices: Vec<DeviceView>, pending: Vec<PendingLinkView>) -> DeviceRosterView {
        let mut view = DeviceRosterView::default();
        view.roster.devices = devices;
        view.roster.pending = pending;
        view.transport_available = true;
        view
    }

    fn card(slug: &str, saved_at: f64) -> UiPackageCard {
        UiPackageCard {
            uid: format!("prj-{slug}"),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: slug.to_string(),
            last_saved_at: Some(saved_at),
            provenance: None,
            on_boards: Vec::new(),
            open_elsewhere: false,
            target: None,
            health: PackageHealth::Ready,
        }
    }

    fn pattern(slug: &str, saved_at: f64) -> UiPackageCard {
        UiPackageCard {
            project_kind: "Pattern".to_string(),
            exports: vec!["effect".to_string()],
            ..card(slug, saved_at)
        }
    }

    fn pending(id: u64, title: &str) -> PendingLinkView {
        PendingLinkView {
            link: lpa_devices::link::LinkId(id),
            device: DeviceId(id),
            title: title.to_string(),
            state_label: "Identifying\u{2026}".to_string(),
            detail: None,
            can_adopt: false,
            firmware_face: FirmwareFace::Unknown,
            detected_chip: None,
            mac: None,
            firmware_blocked: None,
            escapes: Vec::new(),
        }
    }

    fn board(id: u64, title: &str, status: DeviceStatus) -> DeviceView {
        DeviceView {
            id: DeviceId(id),
            title: title.to_string(),
            status,
            state_label: match status {
                DeviceStatus::Ready => "Ready".to_string(),
                DeviceStatus::Offline => "Offline".to_string(),
                other => format!("{other:?}"),
            },
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: None,
            firmware_face: FirmwareFace::Unknown,
            remembered_firmware: None,
            degraded: None,
            loaded_project: LoadedProject::Unknown,
            engine_fps: None,
            link_counters: None,
            can_receive_project: false,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            escapes: Vec::new(),
            update_blocked: None,
            last_update_outcome: None,
        }
    }
}
