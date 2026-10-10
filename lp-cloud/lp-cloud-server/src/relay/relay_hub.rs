//! The relay's routing core: which boards are online, which browser socket
//! is which route on which board, and each board's last picture. Sans-IO.
//!
//! Every leg (a board's device socket, a browser's session socket) has a
//! [`LegId`] the registry minted. The hub is fed what happened on a leg and
//! answers with [`HubAction`]s — bytes for a leg, or a leg to close — which
//! the registry hands to the legs' tasks. It never awaits, never reads a
//! clock (times arrive as arguments, f64 epoch seconds), and never touches
//! the store: the account check happened before a board is registered here.
//!
//! What it holds:
//!
//! - **Boards by id** ([`OnlineBoard`]): the accounts the board proved, its
//!   name, wire version, relay protocol, firmware, project, LAN address,
//!   public address, when it came, and its open routes (at most
//!   [`MAX_ROUTES_PER_BOARD`]).
//! - **Routes**: browser leg → (board, route id), and back.
//! - **Pictures** ([`PictureCache`]): each protocol 2 board's last
//!   picture, kept after the board leaves, gone when the process goes.
//! - **Watches**: until when a member watching a board keeps it fast.
//!
//! The rules, each pinned by a test below:
//!
//! - **A board is sent only frames of its own protocol or older.** A
//!   protocol 1 core closes its leg on any frame it does not know, so one
//!   stray protocol 2 frame would put every fielded board in a reconnect
//!   loop. Every send to a device leg goes through one function that knows
//!   the board's protocol and drops (and logs) anything later.
//! - A board re-registering with the same id **replaces** the old leg: the
//!   old leg is closed and its sessions end ([`RelayCloseCode::BoardGone`]),
//!   so a reboot never leaves a ghost.
//! - A board leaving ends every session on it; its picture stays, offline.
//! - Frames pass byte-identical: the hub wraps a browser's message in a
//!   [`RelayFrame::Frame`] for the board and unwraps the board's for the
//!   browser, and reads none of it.
//! - A board closing a route `Busy` (it holds fewer sessions than the hub
//!   allows) closes the browser [`RelayCloseCode::Busy`].
//! - A board sending a frame out of turn is closed
//!   [`RelayCloseCode::PolicyViolation`]: a protocol 1 board sending a
//!   protocol 2 frame, and any board sending a `PictureRate`, included.
//! - **A protocol 2 board is sent a `PictureRate` right after it
//!   registers**, watched for what is left of an active watch (a board that
//!   comes back while someone watches is fast at once). A member watching
//!   a board renews a [`RelayPictureSettings::lease_s`] watch, and the
//!   board is sent a new rate only when less than half of what it was last
//!   told is left; the board falls back to idle by itself.
//! - Pictures: only the board's accounts read them (see
//!   [`super::picture_cache`]).
//! - Shutdown closes every leg [`RelayCloseCode::GoingAway`], so boards
//!   take the short backoff, and forgets every picture.

use std::collections::HashMap;
use std::net::IpAddr;

use lp_cloud_domain::BoardAccounts;
use lpc_cloud_api::{Base64Bytes, BoardPicture, BoardPresence, KnownPicture, MAX_LISTED_BOARDS};
use lpc_history::PrefixedUid;
use lpc_relay::{
    LanAddress, MAX_ROUTES_PER_BOARD, PictureRate, RELAY_PROTO_2, RefuseReason, RelayBoardId,
    RelayCloseCode, RelayFrame, RelayProject, RouteCloseReason, encode_route_frame, frame_protocol,
};

use super::picture_cache::PictureCache;
use crate::config::RelayPictureSettings;

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
    /// The relay protocol its hello spoke: what it may be sent.
    pub relay_proto: u16,
    /// The firmware version its hello carried (protocol 2 and later).
    pub firmware: Option<String>,
    /// The project it last reported (protocol 2 and later); `None` until
    /// it reports one, or while no project is loaded.
    pub project: Option<RelayProject>,
}

/// A board online now.
#[derive(Debug, Clone, PartialEq)]
pub struct OnlineBoard {
    pub registration: BoardRegistration,
    /// Open routes: route id → the browser leg it is.
    routes: Vec<(u16, LegId)>,
    next_route: u16,
    /// When the watched window the board was last sent ends, f64 epoch
    /// seconds (at or before now: not watched).
    told_watched_until: f64,
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
            relay_proto: board.relay_proto,
            firmware: board.firmware.clone(),
            project: board.project.as_ref().map(|project| project.name.clone()),
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
#[derive(Debug)]
pub struct RelayHub {
    boards: HashMap<RelayBoardId, OnlineBoard>,
    /// Device leg → the board on it.
    device_legs: HashMap<LegId, RelayBoardId>,
    /// Browser leg → (board, route).
    browser_legs: HashMap<LegId, (RelayBoardId, u16)>,
    /// Each protocol 2 board's last picture.
    pictures: PictureCache,
    /// The cadence boards are asked for.
    settings: RelayPictureSettings,
    /// Board → when its members' watch runs out, f64 epoch seconds. Kept
    /// apart from the online boards, so a board that comes back while
    /// someone watches is fast at once. Expired entries are dropped.
    watched_until: HashMap<RelayBoardId, f64>,
}

impl Default for RelayHub {
    fn default() -> Self {
        Self::new()
    }
}

impl RelayHub {
    /// A hub with the default picture cadence, its pictures numbered from 1.
    #[must_use]
    pub fn new() -> Self {
        Self::with_settings(RelayPictureSettings::default(), 1)
    }

    /// A hub asking boards for pictures at `settings`, its first picture's
    /// `seq` being `first_seq` (the registry passes the start time, so a
    /// reader's `seq` from an earlier process never matches).
    #[must_use]
    pub fn with_settings(settings: RelayPictureSettings, first_seq: u64) -> Self {
        Self {
            boards: HashMap::new(),
            device_legs: HashMap::new(),
            browser_legs: HashMap::new(),
            pictures: PictureCache::new(first_seq),
            settings,
            watched_until: HashMap::new(),
        }
    }

    /// Put a board online, replacing (and closing) any older leg with its
    /// id. Refused [`RefuseReason::TooManyBoards`] when every account it
    /// proved already has [`MAX_BOARDS_PER_ACCOUNT`] other boards online.
    /// A protocol 2 board is sent its `PictureRate` (after the
    /// `Registered` the registration sends straight on the socket, since
    /// these actions go through the leg's queue).
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
        let now = registration.since;
        self.pictures.registered(id, &registration.accounts.users);
        self.device_legs.insert(registration.leg, id);
        let mut board = OnlineBoard {
            registration,
            routes: Vec::new(),
            next_route: 1,
            told_watched_until: now,
        };
        if let Some(rate) = self.rate_on_registering(&board, now) {
            board.told_watched_until = now + f64::from(rate.watched_for_s);
            actions.extend(to_board(&board, RelayFrame::PictureRate(rate).encode()));
        }
        self.boards.insert(id, board);
        Ok(actions)
    }

    /// A device leg closed: its board goes offline (if the leg is still
    /// the board's), and every session on it ends. Its picture stays.
    pub fn board_gone(&mut self, leg: LegId) -> Vec<HubAction> {
        let Some(id) = self.device_legs.remove(&leg) else {
            return Vec::new();
        };
        let Some(board) = self.boards.remove(&id) else {
            return Vec::new();
        };
        self.pictures.offline(id);
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
        let open = to_board(board, RelayFrame::Open { route }.encode());
        Ok((route, open.into_iter().collect()))
    }

    /// One message from a browser leg: an lp-link frame for its route.
    pub fn from_browser(&mut self, browser: LegId, bytes: &[u8]) -> Vec<HubAction> {
        let Some((id, route)) = self.browser_legs.get(&browser).copied() else {
            return Vec::new();
        };
        let Some(board) = self.boards.get(&id) else {
            return Vec::new();
        };
        to_board(board, encode_route_frame(route, bytes))
            .into_iter()
            .collect()
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
        let close = RelayFrame::Close {
            route,
            reason: RouteCloseReason::Gone,
        };
        to_board(board, close.encode()).into_iter().collect()
    }

    /// One frame from a registered board's leg, at `now` (f64 epoch
    /// seconds).
    pub fn from_board(&mut self, leg: LegId, frame: RelayFrame, now: f64) -> Vec<HubAction> {
        let Some(id) = self.device_legs.get(&leg).copied() else {
            return Vec::new();
        };
        let Some(board) = self.boards.get_mut(&id) else {
            return Vec::new();
        };
        let speaks_2 = board.registration.relay_proto >= RELAY_PROTO_2;
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
            RelayFrame::Project(project) if speaks_2 => {
                self.pictures.set_project(id, project.clone());
                board.registration.project = project;
                Vec::new()
            }
            RelayFrame::Picture(picture) if speaks_2 => {
                let registration = &board.registration;
                self.pictures.put(
                    id,
                    &registration.accounts.users,
                    picture,
                    registration.project.clone(),
                    now,
                );
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

    /// `user` is looking at board `id` at `now`: keep it fast for a lease.
    /// Only a protocol 2 board online whose accounts include `user` is
    /// watched; a protocol 1 board, an offline board or anyone else
    /// changes nothing. The board is sent a new `PictureRate` only when
    /// less than half a lease is left of what it was last told.
    pub fn watch(&mut self, user: PrefixedUid, id: RelayBoardId, now: f64) -> Vec<HubAction> {
        self.watched_until.retain(|_, until| *until > now);
        let settings = self.settings;
        let Some(board) = self.boards.get_mut(&id) else {
            return Vec::new();
        };
        if board.registration.relay_proto < RELAY_PROTO_2
            || !board.has_account(user)
            || settings.watched_ms == 0
        {
            return Vec::new();
        }
        let lease = f64::from(settings.lease_s);
        let until = self.watched_until.entry(id).or_insert(now);
        *until = until.max(now + lease);
        if board.told_watched_until >= now + lease / 2.0 {
            return Vec::new();
        }
        board.told_watched_until = now + lease;
        let rate = PictureRate {
            idle_s: settings.idle_s,
            watched_ms: settings.watched_ms,
            watched_for_s: settings.lease_s,
        };
        to_board(board, RelayFrame::PictureRate(rate).encode())
            .into_iter()
            .collect()
    }

    /// `BoardPictures` for `user` at `now`: the pictures of the first
    /// [`MAX_LISTED_BOARDS`] asked-for boards that `user` may read (the
    /// rest are simply absent), colours left out where the caller already
    /// holds that `seq`; and, with `watch`, each of those boards watched.
    pub fn pictures_for(
        &mut self,
        user: PrefixedUid,
        asked: &[KnownPicture],
        watch: bool,
        now: f64,
    ) -> (Vec<BoardPicture>, Vec<HubAction>) {
        let mut pictures = Vec::new();
        let mut actions = Vec::new();
        for known in asked.iter().take(MAX_LISTED_BOARDS) {
            let Ok(id) = known.id.parse::<RelayBoardId>() else {
                continue;
            };
            if watch {
                actions.extend(self.watch(user, id, now));
            }
            let Some(entry) = self.pictures.readable(id, user) else {
                continue;
            };
            pictures.push(BoardPicture {
                id: id.to_string(),
                online: entry.online,
                seq: entry.seq,
                at: entry.at,
                outputs: entry.picture.outputs.clone(),
                colors: (known.seq != Some(entry.seq))
                    .then(|| Base64Bytes(entry.picture.colors.clone())),
            });
        }
        (pictures, actions)
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

    /// Close every leg, boards first, with "going away"; forget every
    /// picture and every watch.
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
        self.pictures.clear();
        self.watched_until.clear();
        actions
    }

    /// How many boards are online.
    #[must_use]
    pub fn board_count(&self) -> usize {
        self.boards.len()
    }

    /// How many boards have a picture, online or not.
    #[must_use]
    pub fn picture_count(&self) -> usize {
        self.pictures.len()
    }

    /// The rate a board registering at `now` is sent: protocol 2 only,
    /// watched for what is left of an active watch, and nothing when there
    /// is nothing to ask for (idle pictures off and nobody watching).
    fn rate_on_registering(&self, board: &OnlineBoard, now: f64) -> Option<PictureRate> {
        if board.registration.relay_proto < RELAY_PROTO_2 {
            return None;
        }
        let settings = self.settings;
        let left = self
            .watched_until
            .get(&board.registration.id)
            .map_or(0.0, |until| until - now);
        let watched_for_s = if settings.watched_ms == 0 || left <= 0.0 {
            0
        } else {
            left.ceil().min(f64::from(settings.lease_s)) as u16
        };
        if settings.idle_s == 0 && watched_for_s == 0 {
            return None;
        }
        Some(PictureRate {
            idle_s: settings.idle_s,
            watched_ms: settings.watched_ms,
            watched_for_s,
        })
    }

    /// How many boards `user` has online, not counting `except`.
    fn boards_online_for(&self, user: PrefixedUid, except: Option<RelayBoardId>) -> usize {
        self.boards
            .values()
            .filter(|board| Some(board.registration.id) != except && board.has_account(user))
            .count()
    }
}

/// The one way the hub puts bytes on a board's device leg: never a frame
/// of a later protocol than the board speaks (`RelayFrame::protocol`). A
/// protocol 1 core closes its leg on a frame it does not know, so such a
/// send would be a hub bug that strands fielded boards: it is dropped and
/// logged, and fails a debug build outright.
fn to_board(board: &OnlineBoard, bytes: Vec<u8>) -> Option<HubAction> {
    let board_proto = board.registration.relay_proto;
    match frame_protocol(&bytes) {
        Some(frame_proto) if frame_proto <= board_proto => Some(HubAction::Send {
            leg: board.registration.leg,
            bytes,
        }),
        frame_proto => {
            log::warn!(
                "relay: not sending board {} (protocol {board_proto}) a frame of protocol {frame_proto:?}",
                board.registration.id
            );
            debug_assert!(
                false,
                "a protocol {frame_proto:?} frame for a protocol {board_proto} board"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::UidPrefix;
    use lpc_relay::{RELAY_PROTO_1, RelayPicture};

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
    fn list_boards_shows_the_protocol_the_firmware_and_the_project() {
        let mut hub = RelayHub::new();
        hub.register(registration(1, 10, &[ALICE])).unwrap();
        hub.register(registration_v2(2, 11, &[ALICE], 5.0)).unwrap();
        hub.from_board(11, RelayFrame::Project(Some(project("Rocaille"))), 6.0);

        let boards = hub.boards_for(user(ALICE), None);
        assert_eq!(
            (
                boards[0].relay_proto,
                &boards[0].firmware,
                &boards[0].project
            ),
            (1, &None, &None)
        );
        assert_eq!(
            (
                boards[1].relay_proto,
                &boards[1].firmware,
                &boards[1].project
            ),
            (
                2,
                &Some("2026.10.09-1".to_string()),
                &Some("Rocaille".to_string())
            )
        );
        hub.from_board(11, RelayFrame::Project(None), 2.0);
        assert_eq!(hub.boards_for(user(ALICE), None)[1].project, None);
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
                },
                0.0
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
                },
                0.0
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
        let actions = hub.from_board(10, RelayFrame::Open { route: 1 }, 0.0);
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

    // ---- protocol 2 ----------------------------------------------------

    /// The never-break property, on every path the hub has: a protocol 1
    /// board's leg sees protocol 1 frames and nothing else.
    #[test]
    fn a_protocol_1_board_is_never_sent_a_protocol_2_frame() {
        let mut hub = RelayHub::new();
        let mut actions = Vec::new();
        // It registers (and so does a protocol 2 board of the same account).
        actions.extend(hub.register(registration(1, 10, &[ALICE])).unwrap());
        actions.extend(hub.register(registration_v2(2, 11, &[ALICE], 0.0)).unwrap());
        // A session opens, frames pass both ways, the session closes.
        let (route, open) = hub.open_route(100, id(1)).unwrap();
        actions.extend(open);
        actions.extend(hub.from_browser(100, &[1, 2, 3]));
        actions.extend(hub.from_board(
            10,
            RelayFrame::Frame {
                route,
                bytes: vec![4, 5],
            },
            1.0,
        ));
        actions.extend(hub.from_board(10, RelayFrame::LanChanged { lan: None }, 1.5));
        actions.extend(hub.browser_gone(100));
        // A member watches it and reads its picture, again and again.
        for t in 0..40 {
            let now = 2.0 + f64::from(t);
            actions.extend(hub.watch(user(ALICE), id(1), now));
            let (pictures, watched) = hub.pictures_for(user(ALICE), &known(&[1, 2]), true, now);
            assert!(pictures.is_empty(), "a protocol 1 board has no picture");
            actions.extend(watched);
        }
        // Another board registers, and the protocol 2 one reports.
        actions.extend(
            hub.register(registration_v2(3, 12, &[ALICE], 50.0))
                .unwrap(),
        );
        actions.extend(hub.from_board(11, RelayFrame::Picture(picture(3)), 51.0));
        actions.extend(hub.from_board(11, RelayFrame::Project(None), 51.5));
        // It re-registers (a reboot), then the process goes away.
        actions.extend(hub.register(registration(1, 13, &[ALICE])).unwrap());
        actions.extend(hub.watch(user(ALICE), id(1), 60.0));
        actions.extend(hub.shutdown());

        assert_protocol_1_only(&actions, 10);
        assert_protocol_1_only(&actions, 13);
        assert!(
            actions.iter().any(|action| matches!(
                action,
                HubAction::Send { leg: 11, bytes } if bytes[0] == 0x0c
            )),
            "the protocol 2 board beside it was sent its rates: {actions:?}"
        );
    }

    /// The guard itself: a protocol 2 frame for a protocol 1 board is
    /// never sent (and a debug build fails loudly on the bug).
    #[test]
    fn the_guard_drops_a_later_protocol_frame() {
        let mut hub = RelayHub::new();
        hub.register(registration(1, 10, &[ALICE])).unwrap();
        let board = hub.board(id(1)).unwrap().clone();
        let rate = RelayFrame::PictureRate(rate(60, 500, 0)).encode();
        let sent = std::panic::catch_unwind(|| to_board(&board, rate));
        if cfg!(debug_assertions) {
            assert!(sent.is_err(), "a debug build fails on the bug");
        } else {
            assert_eq!(sent.unwrap(), None);
        }
        assert_eq!(
            to_board(&board, RelayFrame::Open { route: 1 }.encode()),
            Some(HubAction::Send {
                leg: 10,
                bytes: RelayFrame::Open { route: 1 }.encode()
            })
        );
    }

    #[test]
    fn a_protocol_1_board_sending_a_picture_is_closed() {
        for frame in [
            RelayFrame::Picture(picture(1)),
            RelayFrame::Project(Some(project("Rocaille"))),
        ] {
            let mut hub = RelayHub::new();
            hub.register(registration(1, 10, &[ALICE])).unwrap();
            assert_eq!(
                hub.from_board(10, frame.clone(), 1.0),
                [HubAction::Close {
                    leg: 10,
                    code: RelayCloseCode::PolicyViolation
                }],
                "{frame}"
            );
            assert!(hub.board(id(1)).is_none(), "{frame}");
            assert_eq!(hub.picture_count(), 0, "{frame}");
        }
    }

    #[test]
    fn a_board_sending_a_picture_rate_is_closed() {
        for registration in [
            registration(1, 10, &[ALICE]),
            registration_v2(1, 10, &[ALICE], 0.0),
        ] {
            let mut hub = RelayHub::new();
            hub.register(registration).unwrap();
            assert_eq!(
                hub.from_board(10, RelayFrame::PictureRate(rate(60, 500, 0)), 1.0),
                [HubAction::Close {
                    leg: 10,
                    code: RelayCloseCode::PolicyViolation
                }]
            );
        }
    }

    #[test]
    fn a_protocol_2_board_is_sent_its_picture_rate_on_registering() {
        let mut hub = RelayHub::new();
        assert_eq!(
            hub.register(registration_v2(1, 10, &[ALICE], 100.0))
                .unwrap(),
            [rate_for(10, 60, 500, 0)]
        );
        assert!(
            hub.register(registration(2, 11, &[ALICE]))
                .unwrap()
                .is_empty(),
            "a protocol 1 board is sent none"
        );

        // Someone watches; the board drops and comes back 5 s later: fast at
        // once, for what is left of the watch.
        hub.watch(user(ALICE), id(1), 100.0);
        hub.board_gone(10);
        assert_eq!(
            hub.register(registration_v2(1, 12, &[ALICE], 105.0))
                .unwrap(),
            [rate_for(12, 60, 500, 10)]
        );
        // Long after the watch: idle only.
        hub.board_gone(12);
        assert_eq!(
            hub.register(registration_v2(1, 13, &[ALICE], 200.0))
                .unwrap(),
            [rate_for(13, 60, 500, 0)]
        );
    }

    #[test]
    fn the_hub_asks_for_nothing_when_its_knobs_say_none() {
        let settings = |idle_s, watched_ms| RelayPictureSettings {
            idle_s,
            watched_ms,
            lease_s: 15,
        };
        // Idle pictures off: no rate until someone watches.
        let mut hub = RelayHub::with_settings(settings(0, 500), 1);
        assert!(
            hub.register(registration_v2(1, 10, &[ALICE], 0.0))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            hub.watch(user(ALICE), id(1), 1.0),
            [rate_for(10, 0, 500, 15)]
        );
        // Never fast: a watch sends nothing.
        let mut hub = RelayHub::with_settings(settings(60, 0), 1);
        assert_eq!(
            hub.register(registration_v2(1, 10, &[ALICE], 0.0)).unwrap(),
            [rate_for(10, 60, 0, 0)]
        );
        assert!(hub.watch(user(ALICE), id(1), 1.0).is_empty());
    }

    #[test]
    fn a_member_watching_renews_at_half_the_lease() {
        let mut hub = RelayHub::new();
        hub.register(registration_v2(1, 10, &[ALICE, BOB], 0.0))
            .unwrap();
        hub.register(registration(2, 11, &[ALICE])).unwrap();

        assert_eq!(
            hub.watch(user(ALICE), id(1), 10.0),
            [rate_for(10, 60, 500, 15)]
        );
        for now in [11.0, 14.0, 17.0, 17.5] {
            assert!(hub.watch(user(BOB), id(1), now).is_empty(), "{now}");
        }
        assert_eq!(
            hub.watch(user(ALICE), id(1), 17.6),
            [rate_for(10, 60, 500, 15)],
            "under half the lease left of what it was told"
        );

        // A non-member, a guest, an offline board, a protocol 1 board.
        assert!(hub.watch(user(CAROL), id(1), 40.0).is_empty());
        assert!(hub.watch(guest(), id(1), 40.0).is_empty());
        assert!(hub.watch(user(ALICE), id(9), 40.0).is_empty());
        assert!(hub.watch(user(ALICE), id(2), 40.0).is_empty());
    }

    #[test]
    fn pictures_are_cached_with_a_sequence_and_kept_offline() {
        let mut hub = RelayHub::new();
        hub.register(registration_v2(1, 10, &[ALICE], 0.0)).unwrap();
        hub.from_board(10, RelayFrame::Project(Some(project("Rocaille"))), 0.5);
        hub.from_board(10, RelayFrame::Picture(picture(1)), 1.0);

        let (first, _) = hub.pictures_for(user(ALICE), &known(&[1]), false, 2.0);
        assert_eq!(first.len(), 1);
        assert!(first[0].online);
        assert_eq!(first[0].at, 1.0);
        assert_eq!(first[0].outputs, [3]);
        assert_eq!(first[0].colors, Some(Base64Bytes(picture(1).colors)));

        // The caller's seq: no colours again. A new picture: a new seq.
        let seq = first[0].seq;
        let again = vec![KnownPicture {
            id: id(1).to_string(),
            seq: Some(seq),
        }];
        assert_eq!(
            hub.pictures_for(user(ALICE), &again, false, 2.0).0[0].colors,
            None
        );
        hub.from_board(10, RelayFrame::Picture(picture(2)), 3.0);
        let newer = &hub.pictures_for(user(ALICE), &again, false, 4.0).0[0];
        assert_eq!(newer.seq, seq + 1);
        assert_eq!(newer.colors, Some(Base64Bytes(picture(2).colors)));

        // The board leaves: the picture stays, offline.
        hub.board_gone(10);
        let offline = &hub.pictures_for(user(ALICE), &known(&[1]), true, 5.0).0[0];
        assert!(!offline.online);
        assert_eq!(offline.seq, seq + 1);
        assert_eq!(
            hub.pictures.get(id(1)).unwrap().project,
            Some(project("Rocaille")),
            "the project's tags are kept beside it"
        );
    }

    #[test]
    fn a_picture_too_soon_after_the_last_is_dropped() {
        let mut hub = RelayHub::new();
        hub.register(registration_v2(1, 10, &[ALICE], 0.0)).unwrap();
        hub.from_board(10, RelayFrame::Picture(picture(1)), 1.0);
        assert!(
            hub.from_board(10, RelayFrame::Picture(picture(2)), 1.1)
                .is_empty(),
            "dropped, never closed"
        );
        assert!(hub.board(id(1)).is_some());
        let (pictures, _) = hub.pictures_for(user(ALICE), &known(&[1]), false, 2.0);
        assert_eq!(pictures[0].colors, Some(Base64Bytes(picture(1).colors)));
    }

    #[test]
    fn only_the_boards_accounts_read_its_picture() {
        let mut hub = RelayHub::new();
        hub.register(registration_v2(1, 10, &[ALICE, BOB], 0.0))
            .unwrap();
        hub.from_board(10, RelayFrame::Picture(picture(1)), 1.0);
        assert_eq!(
            hub.pictures_for(user(ALICE), &known(&[1]), false, 2.0)
                .0
                .len(),
            1
        );
        assert_eq!(
            hub.pictures_for(user(BOB), &known(&[1]), false, 2.0)
                .0
                .len(),
            1
        );
        assert!(
            hub.pictures_for(user(CAROL), &known(&[1]), false, 2.0)
                .0
                .is_empty()
        );
        assert!(
            hub.pictures_for(guest(), &known(&[1]), true, 2.0)
                .0
                .is_empty()
        );

        // It registers again without Bob: Bob reads nothing, at once.
        hub.board_gone(10);
        hub.register(registration_v2(1, 11, &[ALICE], 3.0)).unwrap();
        assert!(
            hub.pictures_for(user(BOB), &known(&[1]), false, 4.0)
                .0
                .is_empty()
        );
        assert_eq!(
            hub.pictures_for(user(ALICE), &known(&[1]), false, 4.0)
                .0
                .len(),
            1
        );
    }

    #[test]
    fn the_cache_keeps_at_most_4096_boards_dropping_the_oldest_offline_first() {
        use super::super::picture_cache::MAX_CACHED_PICTURES;
        let mut hub = RelayHub::new();
        // Boards 0..4 stay online; every other board leaves after its
        // picture, so the account never holds more than a few at once.
        for n in 0..MAX_CACHED_PICTURES as u32 {
            let leg = u64::from(n) + 1;
            hub.register(registration_v2_wide(n, leg, f64::from(n)))
                .unwrap();
            hub.from_board(leg, RelayFrame::Picture(picture(1)), f64::from(n));
            if n >= 4 {
                hub.board_gone(leg);
            }
        }
        assert_eq!(hub.picture_count(), MAX_CACHED_PICTURES);
        let newcomer = MAX_CACHED_PICTURES as u32;
        hub.register(registration_v2_wide(newcomer, 1_000_000, 10_000.0))
            .unwrap();
        hub.from_board(1_000_000, RelayFrame::Picture(picture(1)), 10_000.0);
        assert_eq!(hub.picture_count(), MAX_CACHED_PICTURES);
        assert!(
            hub.pictures.get(wide_id(4)).is_none(),
            "the oldest offline went"
        );
        assert!(hub.pictures.get(wide_id(0)).is_some(), "older, but online");
        assert!(hub.pictures.get(wide_id(newcomer)).is_some());
    }

    #[test]
    fn shutdown_forgets_every_picture() {
        let mut hub = RelayHub::new();
        hub.register(registration_v2(1, 10, &[ALICE], 0.0)).unwrap();
        hub.from_board(10, RelayFrame::Picture(picture(1)), 1.0);
        hub.watch(user(ALICE), id(1), 1.0);
        hub.board_gone(10);
        assert_eq!(hub.picture_count(), 1);
        hub.shutdown();
        assert_eq!(hub.picture_count(), 0);
        assert!(
            hub.pictures_for(user(ALICE), &known(&[1]), false, 2.0)
                .0
                .is_empty()
        );
        assert_eq!(
            hub.register(registration_v2(1, 11, &[ALICE], 2.0)).unwrap(),
            [rate_for(11, 60, 500, 0)],
            "and every watch"
        );
    }

    #[test]
    fn a_read_asks_about_at_most_sixteen_boards() {
        let mut hub = RelayHub::new();
        for n in 0..17u8 {
            let leg = u64::from(n) + 10;
            // Bob is on all seventeen; each board's other account has room.
            hub.register(registration_v2(n, leg, &[BOB, 100 + n], 0.0))
                .unwrap();
            hub.from_board(leg, RelayFrame::Picture(picture(1)), 1.0);
        }
        let all: Vec<u8> = (0..17).collect();
        let (pictures, _) = hub.pictures_for(user(BOB), &known(&all), false, 2.0);
        assert_eq!(pictures.len(), MAX_LISTED_BOARDS);
        assert!(
            !pictures
                .iter()
                .any(|picture| picture.id == id(16).to_string())
        );
        let garbage = vec![KnownPicture {
            id: "not a board".into(),
            seq: None,
        }];
        assert!(
            hub.pictures_for(user(BOB), &garbage, true, 2.0)
                .0
                .is_empty()
        );
    }

    const ALICE: u8 = 1;
    const BOB: u8 = 2;
    const CAROL: u8 = 3;

    fn user(n: u8) -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::User, &[n; 16])
    }

    /// A guest's uid: never among a board's accounts (guests hold no key).
    fn guest() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::User, &[0xee; 16])
    }

    fn id(n: u8) -> RelayBoardId {
        RelayBoardId([0x10, 0xbd, 0, 0, 0, n])
    }

    fn wide_id(n: u32) -> RelayBoardId {
        let [a, b, c, d] = n.to_be_bytes();
        RelayBoardId([0x02, 0, a, b, c, d])
    }

    fn known(boards: &[u8]) -> Vec<KnownPicture> {
        boards
            .iter()
            .map(|n| KnownPicture {
                id: id(*n).to_string(),
                seq: None,
            })
            .collect()
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
            relay_proto: RELAY_PROTO_1,
            firmware: None,
            project: None,
        }
    }

    fn registration_v2(board: u8, leg: LegId, users: &[u8], since: f64) -> BoardRegistration {
        BoardRegistration {
            since,
            relay_proto: RELAY_PROTO_2,
            firmware: Some("2026.10.09-1".to_string()),
            ..registration(board, leg, users)
        }
    }

    fn registration_v2_wide(board: u32, leg: LegId, since: f64) -> BoardRegistration {
        BoardRegistration {
            id: wide_id(board),
            ..registration_v2(0, leg, &[ALICE], since)
        }
    }

    fn rate(idle_s: u16, watched_ms: u16, watched_for_s: u16) -> PictureRate {
        PictureRate {
            idle_s,
            watched_ms,
            watched_for_s,
        }
    }

    fn rate_for(leg: LegId, idle_s: u16, watched_ms: u16, watched_for_s: u16) -> HubAction {
        HubAction::Send {
            leg,
            bytes: RelayFrame::PictureRate(rate(idle_s, watched_ms, watched_for_s)).encode(),
        }
    }

    fn picture(shade: u8) -> RelayPicture {
        RelayPicture {
            outputs: vec![3],
            colors: vec![shade; 9],
        }
    }

    fn project(name: &str) -> RelayProject {
        RelayProject {
            name: name.to_string(),
            uid_tag: Some([0xa1; 16]),
            content_tag: None,
        }
    }

    /// Every message the hub sent to `leg` decodes to a protocol 1 frame.
    fn assert_protocol_1_only(actions: &[HubAction], leg: LegId) {
        for action in actions {
            if let HubAction::Send { leg: to, bytes } = action
                && *to == leg
            {
                let frame = RelayFrame::decode(bytes).expect("a relay frame");
                assert_eq!(
                    frame.protocol(),
                    RELAY_PROTO_1,
                    "sent to leg {leg}: {frame}"
                );
            }
        }
    }
}
