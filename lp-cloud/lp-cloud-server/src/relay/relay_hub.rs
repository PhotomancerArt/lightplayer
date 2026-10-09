//! The relay's routing core: which boards are online, which browser socket
//! is which route on which board. Sans-IO.
//!
//! Every leg (a board's device socket, a browser's session socket) has a
//! [`LegId`] the registry minted. The hub is fed what happened on a leg and
//! answers with [`HubAction`]s — bytes for a leg, or a leg to close — which
//! the registry hands to the legs' tasks. It never awaits, never reads a
//! clock (times arrive as arguments), and never touches the store: the
//! account check happened before a board is registered here.
//!
//! What it holds:
//!
//! - **Boards by id** ([`OnlineBoard`]): the accounts the board proved, its
//!   name, wire version, LAN address, public address, when it came, and its
//!   open routes (at most [`MAX_ROUTES_PER_BOARD`]).
//! - **Routes**: browser leg → (board, route id), and back.
//!
//! The rules, each pinned by a test below:
//!
//! - A board re-registering with the same id **replaces** the old leg: the
//!   old leg is closed and its sessions end ([`RelayCloseCode::BoardGone`]),
//!   so a reboot never leaves a ghost.
//! - A board leaving ends every session on it.
//! - Frames pass byte-identical: the hub wraps a browser's message in a
//!   [`RelayFrame::Frame`] for the board and unwraps the board's for the
//!   browser, and reads none of it.
//! - A board closing a route `Busy` (it holds fewer sessions than the hub
//!   allows) closes the browser [`RelayCloseCode::Busy`].
//! - A board sending a frame out of turn is closed
//!   [`RelayCloseCode::PolicyViolation`].
//! - Shutdown closes every leg [`RelayCloseCode::GoingAway`], so boards
//!   take the short backoff.

use std::collections::HashMap;
use std::net::IpAddr;

use lp_cloud_domain::BoardAccounts;
use lpc_cloud_api::{BoardPresence, MAX_LISTED_BOARDS};
use lpc_history::PrefixedUid;
use lpc_relay::{
    LanAddress, MAX_ROUTES_PER_BOARD, RefuseReason, RelayBoardId, RelayCloseCode, RelayFrame,
    RouteCloseReason, encode_route_frame,
};

/// A leg's id, minted by the registry: unique for the process's life.
pub type LegId = u64;

/// The most boards one account may have online at once. A registration
/// whose every account is already at it is refused
/// [`RefuseReason::TooManyBoards`]; `ListBoards` shows at most this many
/// anyway ([`MAX_LISTED_BOARDS`]).
pub const MAX_BOARDS_PER_ACCOUNT: usize = MAX_LISTED_BOARDS;

/// What the hub asks the legs to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubAction {
    /// Send one binary message on a leg.
    Send { leg: LegId, bytes: Vec<u8> },
    /// Close a leg with this code.
    Close { leg: LegId, code: RelayCloseCode },
}

/// A board, as its registration told the hub.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardRegistration {
    pub id: RelayBoardId,
    /// The device leg it registered on.
    pub leg: LegId,
    /// The accounts it proved.
    pub accounts: BoardAccounts,
    pub label: String,
    pub wire_proto: u32,
    pub lan: Option<LanAddress>,
    /// The address its leg reached the relay from (`Fly-Client-IP`).
    pub public_ip: Option<IpAddr>,
    /// When it registered, f64 epoch seconds.
    pub since: f64,
}

/// A board online now.
#[derive(Debug, Clone, PartialEq)]
pub struct OnlineBoard {
    pub registration: BoardRegistration,
    /// Open routes: route id → the browser leg it is.
    routes: Vec<(u16, LegId)>,
    next_route: u16,
}

impl OnlineBoard {
    /// Whether `user` is one of the accounts this board proved it holds.
    #[must_use]
    pub fn has_account(&self, user: PrefixedUid) -> bool {
        self.registration.accounts.users.contains(&user)
    }

    /// How many sessions are open on it.
    #[must_use]
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }

    /// The board as `ListBoards` shows it to a caller at `caller_ip`.
    #[must_use]
    pub fn presence(&self, caller_ip: Option<IpAddr>) -> BoardPresence {
        let board = &self.registration;
        BoardPresence {
            id: board.id.to_string(),
            label: board.label.clone(),
            wire_proto: board.wire_proto,
            lan: board.lan.map(|lan| lan.to_string()),
            same_network: board.public_ip.is_some() && board.public_ip == caller_ip,
            since: board.since,
        }
    }

    fn allocate_route(&mut self) -> u16 {
        loop {
            let route = self.next_route;
            self.next_route = self.next_route.wrapping_add(1);
            if !self.routes.iter().any(|(open, _)| *open == route) {
                return route;
            }
        }
    }
}

/// Why a route could not open on a board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenRefusal {
    /// No board with that id is online.
    Offline,
    /// The board has [`MAX_ROUTES_PER_BOARD`] sessions open.
    Busy,
}

/// See the module doc.
#[derive(Debug, Default)]
pub struct RelayHub {
    boards: HashMap<RelayBoardId, OnlineBoard>,
    /// Device leg → the board on it.
    device_legs: HashMap<LegId, RelayBoardId>,
    /// Browser leg → (board, route).
    browser_legs: HashMap<LegId, (RelayBoardId, u16)>,
}

impl RelayHub {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Put a board online, replacing (and closing) any older leg with its
    /// id. Refused [`RefuseReason::TooManyBoards`] when every account it
    /// proved already has [`MAX_BOARDS_PER_ACCOUNT`] other boards online.
    pub fn register(
        &mut self,
        registration: BoardRegistration,
    ) -> Result<Vec<HubAction>, RefuseReason> {
        let id = registration.id;
        let room = registration
            .accounts
            .users
            .iter()
            .any(|user| self.boards_online_for(*user, Some(id)) < MAX_BOARDS_PER_ACCOUNT);
        if !room {
            return Err(RefuseReason::TooManyBoards);
        }
        let mut actions = Vec::new();
        if let Some(old) = self.boards.get(&id).map(|old| old.registration.leg) {
            actions.push(HubAction::Close {
                leg: old,
                code: RelayCloseCode::BoardGone,
            });
            actions.extend(self.board_gone(old));
        }
        self.device_legs.insert(registration.leg, id);
        self.boards.insert(
            id,
            OnlineBoard {
                registration,
                routes: Vec::new(),
                next_route: 1,
            },
        );
        Ok(actions)
    }

    /// A device leg closed: its board goes offline (if the leg is still
    /// the board's), and every session on it ends.
    pub fn board_gone(&mut self, leg: LegId) -> Vec<HubAction> {
        let Some(id) = self.device_legs.remove(&leg) else {
            return Vec::new();
        };
        let Some(board) = self.boards.remove(&id) else {
            return Vec::new();
        };
        board
            .routes
            .iter()
            .map(|(_, browser)| {
                self.browser_legs.remove(browser);
                HubAction::Close {
                    leg: *browser,
                    code: RelayCloseCode::BoardGone,
                }
            })
            .collect()
    }

    /// The board online with `id`, if any.
    #[must_use]
    pub fn board(&self, id: RelayBoardId) -> Option<&OnlineBoard> {
        self.boards.get(&id)
    }

    /// Open a route on board `id` for the browser leg `browser`: the route
    /// id, and the `Open` for the board. Who may ask is not the hub's
    /// business: [`super::route_admission`] decided it before this.
    pub fn open_route(
        &mut self,
        browser: LegId,
        id: RelayBoardId,
    ) -> Result<(u16, Vec<HubAction>), OpenRefusal> {
        let board = self.boards.get_mut(&id).ok_or(OpenRefusal::Offline)?;
        if board.routes.len() >= MAX_ROUTES_PER_BOARD {
            return Err(OpenRefusal::Busy);
        }
        let route = board.allocate_route();
        board.routes.push((route, browser));
        self.browser_legs.insert(browser, (id, route));
        let device = board.registration.leg;
        Ok((
            route,
            vec![HubAction::Send {
                leg: device,
                bytes: RelayFrame::Open { route }.encode(),
            }],
        ))
    }

    /// One message from a browser leg: an lp-link frame for its route.
    pub fn from_browser(&mut self, browser: LegId, bytes: &[u8]) -> Vec<HubAction> {
        let Some((id, route)) = self.browser_legs.get(&browser).copied() else {
            return Vec::new();
        };
        let Some(board) = self.boards.get(&id) else {
            return Vec::new();
        };
        vec![HubAction::Send {
            leg: board.registration.leg,
            bytes: encode_route_frame(route, bytes),
        }]
    }

    /// A browser leg closed: its route closes, and the board is told.
    pub fn browser_gone(&mut self, browser: LegId) -> Vec<HubAction> {
        let Some((id, route)) = self.browser_legs.remove(&browser) else {
            return Vec::new();
        };
        let Some(board) = self.boards.get_mut(&id) else {
            return Vec::new();
        };
        board.routes.retain(|(open, _)| *open != route);
        vec![HubAction::Send {
            leg: board.registration.leg,
            bytes: RelayFrame::Close {
                route,
                reason: RouteCloseReason::Gone,
            }
            .encode(),
        }]
    }

    /// One frame from a registered board's leg.
    pub fn from_board(&mut self, leg: LegId, frame: RelayFrame) -> Vec<HubAction> {
        let Some(id) = self.device_legs.get(&leg).copied() else {
            return Vec::new();
        };
        let Some(board) = self.boards.get_mut(&id) else {
            return Vec::new();
        };
        match frame {
            RelayFrame::Frame { route, bytes } => board
                .routes
                .iter()
                .find(|(open, _)| *open == route)
                .map(|(_, browser)| HubAction::Send {
                    leg: *browser,
                    bytes,
                })
                .into_iter()
                .collect(),
            RelayFrame::Close { route, reason } => {
                let Some(at) = board.routes.iter().position(|(open, _)| *open == route) else {
                    return Vec::new();
                };
                let (_, browser) = board.routes.swap_remove(at);
                self.browser_legs.remove(&browser);
                let code = match reason {
                    RouteCloseReason::Busy => RelayCloseCode::Busy,
                    RouteCloseReason::Normal | RouteCloseReason::Gone => RelayCloseCode::Normal,
                };
                vec![HubAction::Close { leg: browser, code }]
            }
            RelayFrame::LanChanged { lan } => {
                board.registration.lan = lan;
                Vec::new()
            }
            RelayFrame::Hello(_)
            | RelayFrame::Challenge { .. }
            | RelayFrame::Proof { .. }
            | RelayFrame::Registered { .. }
            | RelayFrame::Refused { .. }
            | RelayFrame::Open { .. }
            | RelayFrame::Project(_)
            | RelayFrame::Picture(_)
            | RelayFrame::PictureRate(_) => {
                let mut actions = vec![HubAction::Close {
                    leg,
                    code: RelayCloseCode::PolicyViolation,
                }];
                actions.extend(self.board_gone(leg));
                actions
            }
        }
    }

    /// The boards `user` sees in `ListBoards`, oldest first, at most
    /// [`MAX_LISTED_BOARDS`], with `sameNetwork` against `caller_ip`.
    #[must_use]
    pub fn boards_for(&self, user: PrefixedUid, caller_ip: Option<IpAddr>) -> Vec<BoardPresence> {
        let mut boards: Vec<&OnlineBoard> = self
            .boards
            .values()
            .filter(|board| board.has_account(user))
            .collect();
        boards.sort_by(|a, b| {
            a.registration
                .since
                .total_cmp(&b.registration.since)
                .then_with(|| a.registration.id.cmp(&b.registration.id))
        });
        boards
            .into_iter()
            .take(MAX_LISTED_BOARDS)
            .map(|board| board.presence(caller_ip))
            .collect()
    }

    /// Close every leg, boards first, with "going away".
    pub fn shutdown(&mut self) -> Vec<HubAction> {
        let mut actions: Vec<HubAction> = self
            .device_legs
            .keys()
            .map(|leg| HubAction::Close {
                leg: *leg,
                code: RelayCloseCode::GoingAway,
            })
            .collect();
        actions.extend(self.browser_legs.keys().map(|leg| HubAction::Close {
            leg: *leg,
            code: RelayCloseCode::GoingAway,
        }));
        self.boards.clear();
        self.device_legs.clear();
        self.browser_legs.clear();
        actions
    }

    /// How many boards are online.
    #[must_use]
    pub fn board_count(&self) -> usize {
        self.boards.len()
    }

    /// How many boards `user` has online, not counting `except`.
    fn boards_online_for(&self, user: PrefixedUid, except: Option<RelayBoardId>) -> usize {
        self.boards
            .values()
            .filter(|board| Some(board.registration.id) != except && board.has_account(user))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::UidPrefix;

    #[test]
    fn a_registered_board_is_listed_for_its_accounts_only() {
        let mut hub = RelayHub::new();
        hub.register(registration(1, 10, &[ALICE])).unwrap();
        hub.register(registration(2, 11, &[ALICE, BOB])).unwrap();

        let alices: Vec<String> = hub
            .boards_for(user(ALICE), None)
            .into_iter()
            .map(|b| b.id)
            .collect();
        assert_eq!(alices, [id(1).to_string(), id(2).to_string()]);
        assert_eq!(hub.boards_for(user(BOB), None).len(), 1);
        assert!(hub.boards_for(user(CAROL), None).is_empty());
    }

    #[test]
    fn same_network_compares_public_addresses_and_lan_is_reported() {
        let mut hub = RelayHub::new();
        let mut board = registration(1, 10, &[ALICE]);
        board.public_ip = Some("203.0.113.7".parse().unwrap());
        board.lan = Some(LanAddress {
            ip: [192, 168, 4, 20],
            port: 80,
        });
        hub.register(board).unwrap();

        let home = hub.boards_for(user(ALICE), Some("203.0.113.7".parse().unwrap()));
        assert!(home[0].same_network);
        assert_eq!(home[0].lan.as_deref(), Some("192.168.4.20:80"));
        let away = hub.boards_for(user(ALICE), Some("198.51.100.1".parse().unwrap()));
        assert!(!away[0].same_network);
        assert!(!hub.boards_for(user(ALICE), None)[0].same_network);
    }

    #[test]
    fn re_registering_replaces_the_old_leg_and_ends_its_sessions() {
        let mut hub = RelayHub::new();
        hub.register(registration(1, 10, &[ALICE])).unwrap();
        hub.open_route(100, id(1)).unwrap();

        let actions = hub.register(registration(1, 20, &[ALICE])).unwrap();
        assert_eq!(
            actions,
            [
                HubAction::Close {
                    leg: 10,
                    code: RelayCloseCode::BoardGone
                },
                HubAction::Close {
                    leg: 100,
                    code: RelayCloseCode::BoardGone
                },
            ]
        );
        assert_eq!(hub.board(id(1)).unwrap().registration.leg, 20);
        assert!(
            hub.board_gone(10).is_empty(),
            "the old leg's close is a no-op"
        );
        assert!(hub.board(id(1)).is_some(), "and leaves the new leg online");
    }

    #[test]
    fn frames_pass_byte_identical_both_ways() {
        let mut hub = RelayHub::new();
        hub.register(registration(1, 10, &[ALICE])).unwrap();
        let (route, open) = hub.open_route(100, id(1)).unwrap();
        assert_eq!(
            open,
            [HubAction::Send {
                leg: 10,
                bytes: RelayFrame::Open { route }.encode()
            }]
        );

        let lp_link = vec![0xa5, 0x00, 0xff, 0x7e];
        let to_board = hub.from_browser(100, &lp_link);
        let [HubAction::Send { leg: 10, bytes }] = to_board.as_slice() else {
            panic!("{to_board:?}");
        };
        assert_eq!(
            RelayFrame::decode(bytes).unwrap(),
            RelayFrame::Frame {
                route,
                bytes: lp_link.clone()
            }
        );

        assert_eq!(
            hub.from_board(
                10,
                RelayFrame::Frame {
                    route,
                    bytes: lp_link.clone()
                }
            ),
            [HubAction::Send {
                leg: 100,
                bytes: lp_link
            }]
        );
    }

    #[test]
    fn the_route_limit_is_four_and_offline_boards_refuse() {
        let mut hub = RelayHub::new();
        assert_eq!(hub.open_route(100, id(1)), Err(OpenRefusal::Offline));
        hub.register(registration(1, 10, &[ALICE])).unwrap();
        for browser in 0..MAX_ROUTES_PER_BOARD as u64 {
            hub.open_route(100 + browser, id(1)).unwrap();
        }
        assert_eq!(hub.open_route(200, id(1)), Err(OpenRefusal::Busy));
        hub.browser_gone(100);
        assert!(
            hub.open_route(200, id(1)).is_ok(),
            "a closed route frees a slot"
        );
    }

    #[test]
    fn a_board_refusing_a_route_busy_closes_the_browser_busy() {
        let mut hub = RelayHub::new();
        hub.register(registration(1, 10, &[ALICE])).unwrap();
        let (route, _) = hub.open_route(100, id(1)).unwrap();
        assert_eq!(
            hub.from_board(
                10,
                RelayFrame::Close {
                    route,
                    reason: RouteCloseReason::Busy
                }
            ),
            [HubAction::Close {
                leg: 100,
                code: RelayCloseCode::Busy
            }]
        );
        assert_eq!(hub.board(id(1)).unwrap().route_count(), 0);
        assert!(hub.from_browser(100, &[1]).is_empty());
    }

    #[test]
    fn a_browser_leaving_tells_the_board() {
        let mut hub = RelayHub::new();
        hub.register(registration(1, 10, &[ALICE])).unwrap();
        let (route, _) = hub.open_route(100, id(1)).unwrap();
        assert_eq!(
            hub.browser_gone(100),
            [HubAction::Send {
                leg: 10,
                bytes: RelayFrame::Close {
                    route,
                    reason: RouteCloseReason::Gone
                }
                .encode()
            }]
        );
    }

    #[test]
    fn a_board_leaving_ends_its_sessions() {
        let mut hub = RelayHub::new();
        hub.register(registration(1, 10, &[ALICE])).unwrap();
        hub.open_route(100, id(1)).unwrap();
        hub.open_route(101, id(1)).unwrap();
        let mut closed: Vec<LegId> = hub
            .board_gone(10)
            .into_iter()
            .map(|action| match action {
                HubAction::Close {
                    leg,
                    code: RelayCloseCode::BoardGone,
                } => leg,
                other => panic!("{other:?}"),
            })
            .collect();
        closed.sort_unstable();
        assert_eq!(closed, [100, 101]);
        assert!(hub.board(id(1)).is_none());
        assert!(hub.from_browser(100, &[1]).is_empty());
    }

    #[test]
    fn a_frame_out_of_turn_closes_the_board() {
        let mut hub = RelayHub::new();
        hub.register(registration(1, 10, &[ALICE])).unwrap();
        let actions = hub.from_board(10, RelayFrame::Open { route: 1 });
        assert_eq!(
            actions,
            [HubAction::Close {
                leg: 10,
                code: RelayCloseCode::PolicyViolation
            }]
        );
        assert!(hub.board(id(1)).is_none());
    }

    #[test]
    fn an_account_full_of_boards_refuses_one_more() {
        let mut hub = RelayHub::new();
        for n in 0..MAX_BOARDS_PER_ACCOUNT as u8 {
            hub.register(registration(n, u64::from(n), &[ALICE]))
                .unwrap();
        }
        assert_eq!(
            hub.register(registration(200, 200, &[ALICE])),
            Err(RefuseReason::TooManyBoards)
        );
        assert!(
            hub.register(registration(0, 300, &[ALICE])).is_ok(),
            "a board already online re-registers"
        );
        assert!(
            hub.register(registration(201, 201, &[ALICE, BOB])).is_ok(),
            "room under one of its accounts is enough"
        );
    }

    #[test]
    fn shutdown_closes_every_leg_going_away() {
        let mut hub = RelayHub::new();
        hub.register(registration(1, 10, &[ALICE])).unwrap();
        hub.open_route(100, id(1)).unwrap();
        let actions = hub.shutdown();
        assert_eq!(actions.len(), 2);
        assert!(actions.iter().all(|action| matches!(
            action,
            HubAction::Close {
                code: RelayCloseCode::GoingAway,
                ..
            }
        )));
        assert_eq!(hub.board_count(), 0);
    }

    const ALICE: u8 = 1;
    const BOB: u8 = 2;
    const CAROL: u8 = 3;

    fn user(n: u8) -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::User, &[n; 16])
    }

    fn id(n: u8) -> RelayBoardId {
        RelayBoardId([0x10, 0xbd, 0, 0, 0, n])
    }

    fn registration(board: u8, leg: LegId, users: &[u8]) -> BoardRegistration {
        BoardRegistration {
            id: id(board),
            leg,
            accounts: BoardAccounts {
                users: users.iter().map(|n| user(*n)).collect(),
                accounts_ok: 1,
            },
            label: format!("board {board}"),
            wire_proto: 39,
            lan: None,
            public_ip: None,
            since: f64::from(board),
        }
    }
}
