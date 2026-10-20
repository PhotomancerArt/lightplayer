//! The signed-in account's board list, from the cloud into Studio core and
//! back: your boards on every computer (roadmap
//! `lp2025/2026-10-19-2300-your-boards-follow-you`, M1 and M2).
//!
//! Studio core does not know the cloud. This bridge follows the session
//! and, signed in (a guest counts as signed out, as for the account's keys):
//!
//! - asks `ListAccountBoards` at once, then every 30 s while the page is
//!   visible (D4), and hands every board on the list to core as
//!   [`StudioCommand::AccountBoards`], online or not, stamped with this
//!   page's clock. An answer that does not come is `None`: core keeps
//!   nothing. Signing out hands in `None` and stops the asking.
//! - makes the calls core asks for ([`send_account_board_request`]):
//!   `ReportBoard` when a board here holds the account's key, `ForgetBoard`
//!   for Remove from your account; hands each answer back as
//!   [`StudioCommand::AccountBoardAnswered`], then reads the list again.
//! - asks `BoardPictures`, watching, for the boards whose cards draw
//!   lightplayer.app's picture (the home page's
//!   `DeviceRosterView::cloud_picture_asks`: listed boards no link of this
//!   tab reaches), every half second while the page is visible — the
//!   relay's watched pace (one picture every 500 ms, a lease each read
//!   renews; the cloud relay ADR's "The cadence") — and hands the pictures
//!   to core as [`StudioCommand::AccountBoardPictures`]. Each ask names the
//!   picture core holds, so an unchanged one comes back without its
//!   colours. No card asks, no call; a hidden page asks nothing, and the
//!   boards fall back to their idle pace by themselves.
//!
//! Nothing is persisted (D6).

use dioxus::prelude::*;
use lpa_studio_core::app::studio::studio_view_channel::CommandSender;
use lpa_studio_core::{
    AccountBoardAnswer, AccountBoardLink, AccountBoardPicture, AccountBoardPresence,
    AccountBoardReport, AccountBoardRequest, AccountBoardRow, AccountBoardRows, CloudPictureAsk,
    StudioCommand, UiStudioView,
};
use lpc_cloud_api::{
    AccountBoard, AccountBoardList, BoardPicture, BoardPictureList, BoardPresence, CloudError,
    KnownPicture, ReportBoard, SeenOn,
};

use crate::cloud::CloudSession;

/// How often the account's list is asked while the page is visible (D4).
pub const LIST_EVERY_MS: u32 = 30_000;

/// How often the pictures are asked while a card shows one: the relay's
/// watched pace.
pub const PICTURES_EVERY_MS: u32 = 500;

/// Provide the bridge. Call once, in the app, with core's command queue and
/// the published view (the home page's picture asks are read off it).
pub fn use_account_boards(tx: CommandSender, view: Signal<UiStudioView>) {
    let session = use_context::<Signal<CloudSession>>();
    // Each sign-in starts one pair of loops; a newer sign-in, or a sign
    // out, ends the older ones at their next wake.
    let generation = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0u64)));
    use_effect(move || {
        let current = session();
        let Some(signed_in) = crate::cloud::account_access::signed_in_state(&current) else {
            return;
        };
        let mine = generation.get() + 1;
        generation.set(mine);
        if signed_in.is_none() {
            tx.send(StudioCommand::AccountBoards(None));
            return;
        }
        start_asking(tx.clone(), view, generation.clone(), mine);
    });
}

/// The two loops of one sign-in, `mine` of `generation`.
#[cfg(target_arch = "wasm32")]
fn start_asking(
    tx: CommandSender,
    view: Signal<UiStudioView>,
    generation: std::rc::Rc<std::cell::Cell<u64>>,
    mine: u64,
) {
    use gloo_timers::future::TimeoutFuture;
    use lpc_cloud_api::BoardPictures;
    use lpc_cloud_api::request::ListAccountBoards;

    use crate::cloud::FetchCloudPort;

    let current = {
        let generation = generation.clone();
        move || generation.get() == mine
    };
    let list_tx = tx.clone();
    let list_current = current.clone();
    spawn(async move {
        loop {
            if !list_current() {
                return;
            }
            if page_visible() {
                let answer =
                    lpa_cloud_client::call(&FetchCloudPort::new(), ListAccountBoards).await;
                if !list_current() {
                    return;
                }
                let rows = match answer {
                    Ok(list) => Some(account_board_rows(list, crate::web_app::now_secs())),
                    Err(error) => {
                        log::info!("account boards: lightplayer.app didn't answer: {error}");
                        None
                    }
                };
                list_tx.send(StudioCommand::AccountBoards(rows));
            }
            TimeoutFuture::new(LIST_EVERY_MS).await;
        }
    });
    spawn(async move {
        loop {
            TimeoutFuture::new(PICTURES_EVERY_MS).await;
            if !current() {
                return;
            }
            let asks = picture_asks(&view.peek());
            if asks.is_empty() || !page_visible() {
                continue;
            }
            let answer = lpa_cloud_client::call(
                &FetchCloudPort::new(),
                BoardPictures {
                    boards: known_pictures(&asks),
                    watch: true,
                },
            )
            .await;
            match answer {
                Ok(list) if current() => {
                    // An unchanged picture comes back without its colours:
                    // nothing to hand in.
                    let mut pictures = account_board_pictures(list);
                    pictures.retain(|picture| picture.colors.is_some());
                    if !pictures.is_empty() {
                        tx.send(StudioCommand::AccountBoardPictures(pictures));
                    }
                }
                Ok(_) => return,
                Err(error) => log::debug!("account boards: pictures didn't come: {error}"),
            }
        }
    });
}

/// Host builds never ask the cloud.
#[cfg(not(target_arch = "wasm32"))]
fn start_asking(
    _tx: CommandSender,
    _view: Signal<UiStudioView>,
    _generation: std::rc::Rc<std::cell::Cell<u64>>,
    _mine: u64,
) {
}

/// Whether the page is visible (`document.visibilityState`).
#[cfg(target_arch = "wasm32")]
fn page_visible() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_none_or(|document| document.visibility_state() == web_sys::VisibilityState::Visible)
}

/// The home page's picture asks: none off the home page.
pub fn picture_asks(view: &UiStudioView) -> Vec<CloudPictureAsk> {
    view.home
        .as_ref()
        .map(|home| home.devices.cloud_picture_asks.clone())
        .unwrap_or_default()
}

/// `ListAccountBoards`' answer as core's rows, stamped `at`: every board on
/// the list, online at lightplayer.app now or not.
pub fn account_board_rows(list: AccountBoardList, at: f64) -> AccountBoardRows {
    AccountBoardRows {
        at,
        boards: list.boards.into_iter().map(account_board_row).collect(),
    }
}

fn account_board_row(board: AccountBoard) -> AccountBoardRow {
    // Online, the presence is the newest word on the board's facts.
    let (firmware, project) = match &board.online {
        Some(online) => (
            online.firmware.clone().or(board.firmware),
            online.project.clone().or(board.project),
        ),
        None => (board.firmware, board.project),
    };
    AccountBoardRow {
        id: board.id,
        label: board
            .online
            .as_ref()
            .map_or(board.label, |online| online.label.clone()),
        model: board.model,
        firmware: firmware.filter(|firmware| !firmware.is_empty()),
        project,
        last_seen: board.seen.last_seen(),
        online: board.online.map(account_board_presence),
    }
}

fn account_board_presence(presence: BoardPresence) -> AccountBoardPresence {
    AccountBoardPresence {
        wire_proto: presence.wire_proto,
        lan: presence.lan,
        same_network: presence.same_network,
        since: presence.since,
        relay_proto: presence.relay_proto,
    }
}

/// Make the call core asked for, hand its answer back on `tx`, then read
/// the list again so every card says what lightplayer.app now holds. Host
/// builds never ask the cloud.
pub fn send_account_board_request(
    request: &AccountBoardRequest,
    tx: Option<lpa_studio_core::app::studio::studio_view_channel::CommandSender>,
) {
    #[cfg(target_arch = "wasm32")]
    {
        use crate::cloud::FetchCloudPort;
        use lpc_cloud_api::ForgetBoard;
        use lpc_cloud_api::request::ListAccountBoards;

        let Some(tx) = tx else {
            return;
        };
        let request = request.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let port = FetchCloudPort::new();
            let answer = match &request {
                AccountBoardRequest::Report(report) => {
                    lpa_cloud_client::call(&port, report_board(report))
                        .await
                        .map(|_| ())
                }
                AccountBoardRequest::Forget { id } => {
                    lpa_cloud_client::call(&port, ForgetBoard { id: id.clone() })
                        .await
                        .map(|_| ())
                }
            };
            tx.send(StudioCommand::AccountBoardAnswered(account_board_answer(
                request, answer,
            )));
            if let Ok(list) = lpa_cloud_client::call(&port, ListAccountBoards).await {
                tx.send(StudioCommand::AccountBoards(Some(account_board_rows(
                    list,
                    crate::web_app::now_secs(),
                ))));
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (request, tx);
    }
}

/// Core's report as the cloud's `ReportBoard`.
pub fn report_board(report: &AccountBoardReport) -> ReportBoard {
    ReportBoard {
        id: report.id.clone(),
        label: report.label.clone(),
        model: report.model.clone(),
        firmware: report.firmware.clone(),
        project: report.project.clone(),
        seen_on: match report.seen_on {
            AccountBoardLink::Usb => SeenOn::Usb,
            AccountBoardLink::Ble => SeenOn::Ble,
            AccountBoardLink::Lan => SeenOn::Lan,
        },
    }
}

/// The cloud's answer to `request`, as core reads it.
pub fn account_board_answer(
    request: AccountBoardRequest,
    answer: Result<(), lpa_cloud_client::SyncError>,
) -> AccountBoardAnswer {
    match (answer, request) {
        (Ok(()), AccountBoardRequest::Report(report)) => {
            AccountBoardAnswer::Reported { id: report.id }
        }
        (Ok(()), AccountBoardRequest::Forget { id }) => AccountBoardAnswer::Forgotten { id },
        (
            Err(lpa_cloud_client::SyncError::Cloud(CloudError::BoardListFull { max })),
            AccountBoardRequest::Report(report),
        ) => AccountBoardAnswer::ListFull { id: report.id, max },
        (Err(error), request) => AccountBoardAnswer::Failed {
            request,
            error: match error {
                lpa_cloud_client::SyncError::Transport(_) => "it didn't answer.".to_string(),
                error => error.to_string(),
            },
        },
    }
}

/// The asks as `BoardPictures` names them.
pub fn known_pictures(asks: &[CloudPictureAsk]) -> Vec<KnownPicture> {
    asks.iter()
        .map(|ask| KnownPicture {
            id: ask.id.clone(),
            seq: ask.seq,
        })
        .collect()
}

/// `BoardPictures`' answer as core's pictures.
pub fn account_board_pictures(list: BoardPictureList) -> Vec<AccountBoardPicture> {
    list.pictures
        .into_iter()
        .map(|picture: BoardPicture| AccountBoardPicture {
            id: picture.id,
            seq: picture.seq,
            at: picture.at,
            outputs: picture.outputs,
            colors: picture.colors.map(|colors| colors.0),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_cloud_api::Base64Bytes;

    #[test]
    fn a_list_becomes_core_rows_stamped_with_the_pages_clock() {
        let presence = BoardPresence {
            id: "10bda3b08e30".to_string(),
            label: "Fabric Test".to_string(),
            wire_proto: 42,
            lan: Some("192.168.4.20:80".to_string()),
            same_network: true,
            since: 5.0,
            relay_proto: 2,
            firmware: Some(String::new()),
            project: Some("rainbow".to_string()),
        };
        let rows = account_board_rows(
            AccountBoardList {
                boards: vec![
                    AccountBoard {
                        id: presence.id.clone(),
                        label: "Old name".to_string(),
                        model: None,
                        firmware: None,
                        project: None,
                        joined_at: 1.0,
                        seen: lpc_cloud_api::BoardSeen {
                            relay: Some(5.0),
                            ..Default::default()
                        },
                        online: Some(presence),
                    },
                    AccountBoard {
                        id: "020000000001".to_string(),
                        label: "Porch".to_string(),
                        model: Some("seeed-xiao-esp32c6".to_string()),
                        firmware: Some("2026.10.01-1".to_string()),
                        project: Some("rocaille".to_string()),
                        joined_at: 1.0,
                        seen: lpc_cloud_api::BoardSeen {
                            usb: Some(3.0),
                            ble: Some(4.0),
                            ..Default::default()
                        },
                        online: None,
                    },
                ],
            },
            9.0,
        );
        assert_eq!(rows.at, 9.0);
        let row = &rows.boards[0];
        assert_eq!(row.id, "10bda3b08e30");
        assert_eq!(row.label, "Fabric Test", "online: the board's word now");
        assert_eq!(row.firmware, None, "an empty version is no version");
        assert_eq!(row.project.as_deref(), Some("rainbow"));
        assert!(row.online.as_ref().unwrap().same_network);
        let offline = &rows.boards[1];
        assert_eq!(offline.label, "Porch");
        assert_eq!(offline.model.as_deref(), Some("seeed-xiao-esp32c6"));
        assert_eq!(offline.last_seen, Some(4.0), "the newest sighting");
        assert_eq!(offline.online, None);
    }

    #[test]
    fn a_report_goes_out_as_report_board_and_a_full_list_comes_back_by_name() {
        let report = AccountBoardReport {
            id: "10bda3b08e30".to_string(),
            label: "lp-8e30".to_string(),
            model: Some("seeed-xiao-esp32c6".to_string()),
            firmware: None,
            project: None,
            seen_on: AccountBoardLink::Ble,
        };
        assert_eq!(report_board(&report).seen_on, SeenOn::Ble);
        let full = account_board_answer(
            AccountBoardRequest::Report(report.clone()),
            Err(lpa_cloud_client::SyncError::Cloud(
                CloudError::BoardListFull { max: 64 },
            )),
        );
        assert_eq!(
            full,
            AccountBoardAnswer::ListFull {
                id: report.id.clone(),
                max: 64
            }
        );
        let forgot = account_board_answer(
            AccountBoardRequest::Forget {
                id: report.id.clone(),
            },
            Ok(()),
        );
        assert_eq!(forgot, AccountBoardAnswer::Forgotten { id: report.id });
    }

    #[test]
    fn asks_name_the_picture_held_and_answers_carry_their_colours() {
        let known = known_pictures(&[CloudPictureAsk {
            id: "10bda3b08e30".to_string(),
            seq: Some(3),
        }]);
        assert_eq!(known[0].seq, Some(3));
        let pictures = account_board_pictures(BoardPictureList {
            pictures: vec![BoardPicture {
                id: "10bda3b08e30".to_string(),
                online: true,
                seq: 4,
                at: 2.0,
                outputs: vec![1],
                colors: Some(Base64Bytes(vec![1, 2, 3])),
            }],
        });
        assert_eq!(pictures[0].colors.as_deref(), Some(&[1u8, 2, 3][..]));
        assert_eq!(pictures[0].seq, 4);
    }

    #[test]
    fn off_the_home_page_nothing_is_asked() {
        assert!(picture_asks(&UiStudioView::empty()).is_empty());
    }
}
