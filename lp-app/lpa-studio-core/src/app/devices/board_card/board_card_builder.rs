//! Build a board's card: [`board_card`] for a board on the roster,
//! [`pending_board_card`] for a link still saying who it is.
//!
//! Pure functions of their input. The card points only at verbs the board's
//! own offers publish (a debug assertion, and a test): a button the card
//! drew that the tree did not offer would be one the app agent cannot see
//! and a press that could only fail.
//!
//! The rows come first; the surface reads them finished (its progress is
//! the running row's), and its state names the one row it speaks for,
//! which then goes quiet: the surface is the card's one loud place.

use lpa_devices::view::PendingLinkView;

use super::access_bar::{access_bar, pending_access_bar};
use super::bluetooth_bar::bluetooth_bar;
use super::board_card_input::BoardCardInput;
use super::board_picture::board_picture;
use super::board_surface::{board_surface, carried_layer, pending_surface, surface_state};
use super::cloud_bar::cloud_bar;
use super::detail_sections::notice;
use super::firmware_bar::{firmware_bar, pending_firmware_bar};
use super::hardware_bar::{hardware_bar, pending_hardware_bar};
use super::primary_action::{pending_action, pending_primary, primary_action, state_action};
use super::project_bar::{edit_action, pending_project_bar, project_bar};
use super::status_corner::{layer_title, status_corner};
use super::ui_bar_work::{BarWorkState, UiBarWork};
use super::ui_board_card::{UiBoardCard, UiBoardPresence};
use super::ui_board_panel::UiBoardPanel;
use super::ui_board_picture::{PictureSource, UiBoardPicture};
use super::ui_detail_panel::UiDetailPanel;
use super::ui_name_bar::UiNameBar;
use super::ui_stack_bar::{BarLayer, UiStackBar};
use super::ui_status_corner::{CornerMark, UiCornerDetails, UiStatusCorner};
use super::usb_bar::{pending_link_bars, usb_bar};
use super::wifi_bar::wifi_bar;
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::{OfferPath, RichLine, RichSection, RichWeight, UiOffer, UiStatusKind};

/// A board's card, built from `input`.
pub fn board_card(input: &BoardCardInput<'_>) -> UiBoardCard {
    let bars: Vec<UiStackBar> = vec![
        project_bar(input),
        usb_bar(input),
        wifi_bar(input),
        bluetooth_bar(input),
        cloud_bar(input),
        access_bar(input),
        firmware_bar(input),
        hardware_bar(input),
    ]
    .into_iter()
    .map(with_failure_notice)
    .map(with_work_tone)
    .collect();
    let running_work = bars
        .iter()
        .filter_map(|bar| bar.work.as_ref())
        .find(|work| work.state == BarWorkState::Running);
    let panel = connected_panel(input);
    let picture = board_picture(input);
    let primary = primary_action(input, running_work);
    let state = surface_state(input);
    let surface = board_surface(
        input,
        state,
        &bars,
        &picture,
        primary.as_ref(),
        state_action(input, state).as_ref(),
    );
    let bars = quiet_rows(bars, carried_layer(input, state));
    let card = UiBoardCard {
        board: input.board.clone(),
        device: input.view.id,
        // A board another tab holds is plugged in and running: Online,
        // whatever it is here (`split_roster` says the same).
        presence: match input.offline()
            && input.view.held_elsewhere.is_none()
            && !input.online_at_cloud()
        {
            true => UiBoardPresence::Offline,
            false => UiBoardPresence::Online,
        },
        picture,
        surface,
        status: status_corner(input, &bars, panel.as_ref()),
        name_bar: UiNameBar {
            title: input.view.title.clone(),
            place: None,
            rename: input.offer("rename").map(|rename| rename.path.clone()),
            primary,
        },
        bars,
        panel,
    };
    debug_assert_offered(&card, input.offers);
    card
}

/// A row's tone reads how its work stands: green for a few seconds after
/// work that went well, red (striped) after work that failed; while work
/// runs the row is neutral, as its builder left it.
fn with_work_tone(mut bar: UiStackBar) -> UiStackBar {
    match bar.work.as_ref().map(|work| &work.state) {
        Some(BarWorkState::Done) => bar.tone = UiStatusKind::Good,
        Some(BarWorkState::Failed { .. }) => bar.tone = UiStatusKind::Error,
        Some(BarWorkState::Running) | None => {}
    }
    bar
}

/// The rows go quiet: the row whose problem the surface says keeps its
/// state word and its button, and loses its attention or error tone. The
/// surface is the one loud place (P2).
fn quiet_rows(bars: Vec<UiStackBar>, carried: Option<BarLayer>) -> Vec<UiStackBar> {
    bars.into_iter()
        .map(|mut bar| {
            let loud = matches!(bar.tone, UiStatusKind::Attention | UiStatusKind::Error);
            if Some(bar.layer) == carried && loud {
                bar.tone = UiStatusKind::Neutral;
            }
            bar
        })
        .collect()
}

/// A row whose work failed says why in its details, first: its notice is
/// the failure, with the row's Retry, and the status corner reads it from
/// there like any notice. The row itself says only that it failed ("send
/// failed", "connection failed"), never the sentence (P5).
fn with_failure_notice(mut bar: UiStackBar) -> UiStackBar {
    if let Some(UiBarWork {
        state: BarWorkState::Failed { retry },
        words,
        detail,
        ..
    }) = &bar.work
    {
        bar.details.sections.insert(
            0,
            notice(
                layer_title(bar.layer),
                UiStatusKind::Error,
                detail.clone().unwrap_or_else(|| words.clone()),
                retry.clone(),
            ),
        );
    }
    bar
}

/// The board's panel on its card: every control of the project's root
/// panel, while the session is connected here and its project is ready
/// (CD7), with Edit (or its lock) at its foot — the project bar's own
/// action, built by the same function (CD15) — and auto-save only at the
/// edit tier (Q12), for the ⋯ menu.
fn connected_panel(input: &BoardCardInput<'_>) -> Option<UiBoardPanel> {
    let picks = input.panel.filter(|_| input.connection.is_connected())?;
    Some(UiBoardPanel {
        auto_save: picks.auto_save.filter(|_| input.can_edit()),
        edit: edit_action(input),
        ..picks.clone()
    })
}

/// A new board's card: a link the roster is still identifying. `link` is
/// the kind of the link it arrived on.
pub fn pending_board_card(
    pending: &PendingLinkView,
    board: &OfferPath,
    offers: &[UiOffer],
    link: UiLinkKind,
) -> UiBoardCard {
    let state = match pending.detail.as_deref() {
        Some(detail) => format!("{} · {detail}", pending.state_label),
        None => pending.state_label.clone(),
    };
    let primary = pending_primary(pending, board, offers);
    let surface = pending_surface(
        pending,
        primary.as_ref(),
        pending_action(pending, board, offers).as_ref(),
    );
    let card = UiBoardCard {
        board: board.clone(),
        device: pending.device,
        presence: UiBoardPresence::New,
        picture: UiBoardPicture {
            source: PictureSource::None,
            frame: None,
            dim: false,
            light: None,
        },
        surface,
        status: UiStatusCorner {
            mark: CornerMark::Blank,
            details: UiCornerDetails {
                sections: vec![RichSection {
                    title: "Running".to_string(),
                    tone: UiStatusKind::Neutral,
                    sentence: None,
                    lines: vec![RichLine::new("State", state)],
                    chip: None,
                    affordances: Vec::new(),
                    weight: RichWeight::Advisory,
                }],
                // A link that has said nothing yet has nothing in its
                // terminal; the panel stands so the corner reads the same.
                panels: vec![UiDetailPanel::Terminal {
                    lines: Vec::new(),
                    dropped: 0,
                }],
            },
        },
        name_bar: UiNameBar {
            title: pending.title.clone(),
            place: None,
            rename: offers
                .iter()
                .find(|offer| offer.path == board.clone().child("rename"))
                .map(|rename| rename.path.clone()),
            primary,
        },
        bars: std::iter::once(pending_project_bar(pending))
            .chain(pending_link_bars(pending, link))
            .chain([
                pending_access_bar(pending),
                pending_firmware_bar(pending, board, offers),
                pending_hardware_bar(pending, board, offers),
            ])
            .collect(),
        panel: None,
    };
    debug_assert_offered(&card, offers);
    card
}

/// Every offer path the card points at must be one `offers` publishes.
fn debug_assert_offered(card: &UiBoardCard, offers: &[UiOffer]) {
    if cfg!(debug_assertions) {
        for path in card.offer_paths() {
            debug_assert!(
                offers.iter().any(|offer| &offer.path == path),
                "the card points at `{path}`, which is not offered: {:?}",
                offers
                    .iter()
                    .map(|offer| offer.path.to_string())
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::device::DeviceStatus;
    use lpa_devices::view::OutcomeView;
    use lpa_devices::{ActivityKind, HoldLevel};

    use super::super::board_connection::BoardConnection;
    use super::super::card_fixtures::{CardFixture, activity, board, feed};
    use super::super::primary_action::tests::pending_view;
    use super::super::ui_stack_bar::BarLayer;
    use super::*;
    use crate::app::devices::activity_ends::ActivityEnd;
    use crate::app::devices::device_card_feed_view::FeedLiveness;

    #[test]
    fn a_card_has_its_eight_rows_in_order() {
        let card = board_card(&CardFixture::ready().input());
        let layers: Vec<BarLayer> = card.bars.iter().map(|bar| bar.layer).collect();
        assert_eq!(layers, BarLayer::ALL);
        assert_eq!(card.name_bar.title, "Porch");
        assert_eq!(card.name_bar.place, None);
        assert_eq!(card.presence, UiBoardPresence::Online);
        assert_eq!(card.board.to_string(), "devices/mac-a0f26287b48c");
    }

    /// AC4: a new board's to-do rows are lit, and only those. Installed and
    /// signed out: Project (none · Add), Wi‑Fi (not connected · Set up) and
    /// lightplayer.app (not connected · Sign in). Signed in, lightplayer.app
    /// waits on Wi‑Fi — its fix is the Wi‑Fi row's — so it is not lit. A
    /// blank chip: Firmware (none · Install).
    #[test]
    fn a_new_boards_to_do_rows_are_lit_and_only_those() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = lpa_devices::view::LoadedProject::Empty;
        fixture.view.can_remove_project = false;
        fixture.plays = crate::app::devices::board_plays::BoardPlays::Nothing;
        fixture.wifi = Some(crate::UiDeviceWifi {
            status: Some(lpc_wire::server::NetworkStatus {
                wifi: true,
                cloud_relay: true,
                networks: Vec::new(),
                station: lpc_wire::server::StationState::NotConnected,
                relay: lpc_wire::server::RelayState::NoAccount,
            }),
            ..crate::UiDeviceWifi::new(fixture.view.id, true)
        });
        let lit = |fixture: &mut CardFixture| -> Vec<BarLayer> {
            board_card(&fixture.input())
                .bars
                .iter()
                .filter(|bar| bar.todo)
                .map(|bar| bar.layer)
                .collect()
        };
        assert_eq!(
            lit(&mut fixture),
            [BarLayer::Project, BarLayer::Wifi, BarLayer::Cloud]
        );
        fixture.account = Some("Yona".to_string());
        assert_eq!(lit(&mut fixture), [BarLayer::Project, BarLayer::Wifi]);

        let mut blank = CardFixture::ready();
        blank.view.status = DeviceStatus::NeedsAttention;
        blank.view.firmware_face = lpa_devices::view::FirmwareFace::Blank;
        blank.view.loaded_project = lpa_devices::view::LoadedProject::Unknown;
        blank.view.can_receive_project = false;
        blank.view.can_remove_project = false;
        blank.plays = crate::app::devices::board_plays::BoardPlays::Unknown;
        assert_eq!(lit(&mut blank), [BarLayer::Firmware]);
    }

    /// The card can only point at verbs core offered: in every state a test
    /// can reach, each path it names is published.
    #[test]
    fn every_card_action_names_a_published_offer() {
        let mut failed = CardFixture::ready();
        failed.view.last_outcome = Some(OutcomeView {
            summary: "The board did not answer".to_string(),
            ok: false,
        });
        failed.ended = Some(ActivityEnd {
            kind: ActivityKind::Push,
            ok: false,
            at: failed.now - 5.0,
        });
        let mut offline = CardFixture::offline();
        offline.wifi_address = true;
        for mut fixture in [
            CardFixture::ready(),
            CardFixture::ready().with_activity(activity(
                ActivityKind::Push,
                "Sending the project",
                Some(10),
            )),
            CardFixture::ready()
                .over(crate::UiLinkKind::Bluetooth)
                .locked(),
            offline,
            failed,
        ] {
            let offers = fixture.offers();
            let card = board_card(&fixture.input());
            assert!(!card.offer_paths().is_empty(), "{card:?}");
            for path in card.offer_paths() {
                assert!(
                    offers.iter().any(|offer| &offer.path == path),
                    "{path} is not offered"
                );
            }
        }
    }

    /// CD7 and CD15: connected with its project ready, the card carries the
    /// whole panel (the bars are still built), its primary is Done, and Edit
    /// sits at the panel's foot — the project bar's own action. The ⋯ holds
    /// the panel's reset and, at the edit tier, auto-save.
    #[test]
    fn a_connected_card_carries_the_panel_with_edit_at_its_foot() {
        let mut fixture = connected(CardFixture::ready());
        let card = board_card(&fixture.input());
        let panel = card.panel.clone().expect("the panel");
        assert_eq!(panel.controls().count(), 3, "every control");
        assert_eq!(panel.auto_save, Some(true), "USB is the edit tier");
        assert!(
            card.status
                .details
                .panels
                .contains(&UiDetailPanel::PanelSettings {
                    target: panel.target,
                    held: 0,
                    auto_save: Some(true),
                }),
            "the ⋯ holds the reset and auto-save: {:?}",
            card.status.details.panels
        );
        let edit = panel.edit.clone().expect("Edit at the panel's foot");
        assert_eq!(edit.word, "Edit");
        assert_eq!(edit.icon.as_deref(), Some("edit"));
        assert_eq!(
            Some(&edit),
            card.bar(BarLayer::Project).action.as_ref(),
            "one function builds both"
        );
        assert_eq!(card.bars.len(), 8, "the rows are still built");
        assert_eq!(
            card.name_bar
                .primary
                .as_ref()
                .and_then(|primary| primary.word()),
            Some("Done")
        );
        assert!(card.offer_paths().contains(&&edit.offer));

        // Not connected (opening, reconnecting, watched): no panel.
        for connection in [
            BoardConnection::Watched,
            BoardConnection::Connecting,
            BoardConnection::Reconnecting,
        ] {
            fixture.connection = connection.clone();
            assert_eq!(board_card(&fixture.input()).panel, None, "{connection:?}");
        }
        // Connected, but the project not ready (the controller has no
        // picks yet): no panel.
        let mut opening = connected(CardFixture::ready());
        opening.panel = None;
        assert_eq!(board_card(&opening.input()).panel, None);
    }

    /// AC7: a play-only link's panel hides auto-save, and its Edit wears
    /// the lock (`unlock`, the password sheet).
    #[test]
    fn a_play_only_connected_card_hides_auto_save_and_locks_edit() {
        let mut fixture = connected(CardFixture::ready().over(crate::UiLinkKind::Bluetooth));
        fixture.access = Some(crate::UiDeviceAccess {
            unlock: Some(crate::UiUnlockOffer::PlayOnly),
            grant: Some(crate::UiAccessGrant {
                tier: lpc_access::Tier::Play,
                key: Some("friends".to_string()),
            }),
            ..crate::UiDeviceAccess::default()
        });
        let card = board_card(&fixture.input());
        let panel = card.panel.expect("the panel");
        assert_eq!(panel.auto_save, None, "below the edit tier");
        assert!(
            !card.status.details.panels.iter().any(|panel| matches!(
                panel,
                UiDetailPanel::PanelSettings {
                    auto_save: Some(_),
                    ..
                }
            )),
            "no auto-save in the ⋯ either"
        );
        let edit = panel.edit.expect("Edit, locked");
        assert_eq!(edit.icon.as_deref(), Some("lock"));
        assert!(edit.offer.to_string().ends_with("/unlock"));
        assert_eq!(edit.draw, crate::UiActionDraw::Sheet);
    }

    #[test]
    fn an_offline_board_is_an_offline_card() {
        let card = board_card(&CardFixture::offline().input());
        assert_eq!(card.presence, UiBoardPresence::Offline);
        assert_eq!(card.status.mark, CornerMark::Quiet);
    }

    /// A board of the account online at lightplayer.app, that no link here
    /// reaches (Yona's "Fabric Test", set up on a phone, 10-19): an Online
    /// card whose surface is its picture from lightplayer.app, current,
    /// with no words; Connect through lightplayer.app is the primary; its
    /// lightplayer.app row says online; the details say where the picture
    /// is from and how old it is.
    #[test]
    fn a_board_online_at_lightplayer_app_is_an_online_card_with_its_cloud_picture() {
        let mut fixture = CardFixture::online_at_cloud();
        fixture.now = 1_003.0;
        let card = board_card(&fixture.input());
        use crate::UiPrimary;
        assert_eq!(card.presence, UiBoardPresence::Online);
        assert_eq!(card.surface.state, crate::SurfaceState::OnlineAtCloud);
        assert_eq!(card.surface.notice, None);
        assert_eq!(card.surface.picture, crate::SurfacePicture::Live);
        assert_eq!(card.picture.source, PictureSource::Cloud);
        assert!(!card.picture.dim);
        assert!(card.picture.frame.is_some());
        let Some(UiPrimary::Offer(primary)) = card.name_bar.primary.clone() else {
            panic!("a primary: {:?}", card.name_bar.primary);
        };
        assert_eq!(primary.word, "Connect");
        assert_eq!(primary.icon.as_deref(), Some("cloud"));
        // A board this browser remembers connects by its own `connect`,
        // through lightplayer.app; one it never met has no registry row,
        // and presses `connect-relay` itself.
        assert!(primary.offer.to_string().ends_with("/connect"));
        let mut never_met = CardFixture::online_at_cloud();
        never_met.uid = None;
        let Some(UiPrimary::Offer(primary)) = board_card(&never_met.input()).name_bar.primary
        else {
            panic!("a primary for a board never met");
        };
        assert!(primary.offer.to_string().ends_with("/connect-relay"));
        assert_eq!(primary.icon.as_deref(), Some("cloud"));
        let cloud = card
            .bars
            .iter()
            .find(|bar| bar.layer == BarLayer::Cloud)
            .unwrap();
        assert_eq!(cloud.aside.as_deref(), Some("online"));
        let running = card
            .status
            .details
            .sections
            .iter()
            .find(|section| section.title == "Running")
            .unwrap();
        let line = |label: &str| {
            running
                .lines
                .iter()
                .find(|line| line.label == label)
                .map(|line| line.value.clone())
        };
        assert_eq!(line("Picture").as_deref(), Some("From lightplayer.app."));
        assert_eq!(line("State").as_deref(), Some("Online at lightplayer.app"));
    }

    /// A board on relay protocol 1 sends lightplayer.app no picture: the
    /// card says nothing about that, and its surface is the dark, no words.
    #[test]
    fn a_board_with_no_cloud_picture_is_dark_and_says_nothing() {
        let mut fixture = CardFixture::online_at_cloud();
        fixture.cloud = Some(super::super::card_fixtures::cloud_board(false));
        let card = board_card(&fixture.input());
        assert_eq!(card.surface.state, crate::SurfaceState::OnlineAtCloud);
        assert_eq!(card.surface.notice, None);
        assert_eq!(card.picture.source, PictureSource::None);
        assert!(card.picture.frame.is_none());
    }

    /// A board another tab holds is an Online card (A3: it is plugged in
    /// and running), with the picture that tab saved, dimmed and aged, and
    /// Connect as its primary.
    #[test]
    fn a_board_another_tab_holds_is_an_online_card_with_its_saved_picture() {
        for status in [DeviceStatus::Attached, DeviceStatus::Offline] {
            let mut fixture = CardFixture::held(HoldLevel::Open);
            fixture.view.status = status;
            let mut saved = feed(FeedLiveness::Offline, true);
            saved.frame_age_secs = Some(300.0);
            fixture.feed = Some(saved);
            let card = board_card(&fixture.input());

            assert_eq!(card.presence, UiBoardPresence::Online, "{status:?}");
            assert_eq!(card.picture.source, PictureSource::Saved);
            assert!(card.picture.dim);
            assert!(card.picture.frame.is_some());
            assert_eq!(card.surface.state, crate::SurfaceState::AnotherTab);
            assert_eq!(
                card.surface.picture,
                crate::SurfacePicture::Dimmed,
                "the picture dims under the notice"
            );
            assert_eq!(
                card.bar(BarLayer::Usb).tone,
                UiStatusKind::Neutral,
                "the surface says someone has it, once (P2)"
            );
            let running = card.status.details.sections.last().expect("running");
            let lines: Vec<(&str, &str)> = running
                .lines
                .iter()
                .map(|line| (line.label.as_str(), line.value.as_str()))
                .collect();
            assert_eq!(
                lines,
                [
                    ("State", "Open in another tab"),
                    ("Last frame", "5 min ago"),
                    ("Picture", "The last picture another tab saved.")
                ]
            );
            assert_eq!(
                card.name_bar
                    .primary
                    .as_ref()
                    .and_then(|primary| primary.word()),
                Some("Connect here")
            );
            assert_eq!(card.bar(BarLayer::Usb).summary, "Open in another tab");
        }
    }

    /// A held board nobody has saved a picture for says so, instead of
    /// "Not connected".
    #[test]
    fn a_held_board_with_no_picture_says_who_has_it() {
        let mut fixture = CardFixture::held(HoldLevel::Watching);
        let card = board_card(&fixture.input());
        assert_eq!(card.picture.source, PictureSource::None);
        let running = card.status.details.sections.last().expect("running");
        assert!(running.lines.iter().any(|line| line.label == "Picture"
            && line.value == "Another tab has this board. It has saved no picture yet."));
    }

    #[test]
    fn a_new_board_is_a_blank_new_card_saying_its_state() {
        let card = pending_board_card(&pending_view(), &board(), &[], UiLinkKind::Bluetooth);
        assert_eq!(card.presence, UiBoardPresence::New);
        assert_eq!(card.status.mark, CornerMark::Blank);
        assert_eq!(card.name_bar.title, "New board");
        let layers: Vec<BarLayer> = card.bars.iter().map(|bar| bar.layer).collect();
        assert_eq!(layers, BarLayer::ALL, "a new board has the eight rows too");
        assert!(matches!(
            card.status.details.panels.as_slice(),
            [UiDetailPanel::Terminal { lines, dropped: 0 }] if lines.is_empty()
        ));
        assert_eq!(
            card.name_bar
                .primary
                .as_ref()
                .and_then(|primary| primary.word()),
            None,
            "how far it has got, not a greyed Connect"
        );
        assert_eq!(card.panel, None);
    }

    /// `fixture`, connected on its card with its project's panel: the
    /// brightness, a fader and a palette.
    fn connected(mut fixture: CardFixture) -> CardFixture {
        fixture.editor_holds_it = true;
        fixture.connection = BoardConnection::Connected;
        fixture.feed = Some(super::super::card_fixtures::lens_feed());
        let control = |channel: &str, widget| {
            crate::UiPanelControlView::new(
                channel,
                crate::UiPanelControl {
                    label: channel.to_string(),
                    address: None,
                    widget,
                    value: crate::UiSlotValue::f32(0.5),
                    emit: crate::UiPanelEmit::Value,
                    live_value: None,
                    live_gradient: None,
                    panel_target: None,
                    unit: None,
                    state: crate::UiSlotFieldState::editable(),
                    aspects: Vec::new(),
                },
            )
        };
        let fader = || crate::UiPanelWidget::Fader {
            min: 0.0,
            max: 1.0,
            step: None,
        };
        let root = crate::UiPanelGroup::new("Porch", "/")
            .with_target(lpc_wire::WireScopeRef::Module {
                owner: lpc_model::NodeId::new(1),
            })
            .with_controls(vec![
                control("brightness", fader()),
                control("level", fader()),
                control("palette", crate::UiPanelWidget::PaletteSwatch),
            ]);
        fixture.panel = Some(super::super::board_panel_picks(&root, Some(true)));
        fixture
    }
}
