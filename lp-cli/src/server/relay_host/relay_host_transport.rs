//! The host board's server transport with the relay in it: everything the
//! inner transport (`serve`'s WebSocket server, or nothing) carries, plus
//! one secure lp-link responder per relay route.
//!
//! The routes' server links are `LinkTrust::Relayed`, numbered from
//! [`RELAY_LINK_IDS`] so they never meet the inner transport's. Each server
//! tick pumps the routes (frames in from the device leg, requests and
//! handshake events out, frames back to the leg). After the tick,
//! [`RelayHostTransport::after_tick`] sends each link that came up its
//! hello, makes the picture the hub asked for (relay protocol 2: the
//! engine's picture of the first loaded project, written in place), and
//! once a second hands the leg the project's facts if they changed — the
//! same two answers a C6 gives its relay. Nothing here logs the project's
//! uid or a colour.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use lpa_server::LpServer;
use lpc_relay::{
    DEFAULT_PICTURE_SAMPLES, MAX_BOARD_PICTURE_FRAME, MAX_PICTURE_OUTPUTS, RelayProjectFacts,
    RelayState, picture_sample_count, write_picture_header,
};
use lpc_shared::transport::{Incoming, KeyAnswer, Link, LinkId, SecureLinkEvent, ServerTransport};
use lpc_wire::{TransportError, WireServerMessage, WireServerMsgBody};
use tokio::sync::mpsc;

use super::relay_device_leg::{LegCommand, LegEvent};
use super::relay_route_link::RelayRouteLink;

/// The first server link id a relay session gets.
pub const RELAY_LINK_IDS: u32 = 1_000_000;

/// How often the project's facts are compared with what the leg was told.
const PROJECT_CHECK_EVERY: Duration = Duration::from_secs(1);

/// See the module doc.
pub struct RelayHostTransport<T> {
    inner: T,
    events: mpsc::UnboundedReceiver<LegEvent>,
    commands: mpsc::UnboundedSender<LegCommand>,
    routes: BTreeMap<u16, RelayRouteLink>,
    next_id: u32,
    nonce: u32,
    inbox: Vec<Incoming>,
    secure_events: Vec<(LinkId, SecureLinkEvent)>,
    closed: Vec<LinkId>,
    came_up: Vec<LinkId>,
    state: RelayState,
    /// The hub asked for a picture the loop has not made yet.
    picture_wanted: bool,
    /// The picture frame, written in place and kept across pictures.
    picture: Vec<u8>,
    /// Lamps per output of the picture being made.
    lamps: Vec<u32>,
    /// The project's name and uid as the leg was last told; `None` before
    /// the first report.
    project_told: Option<Option<(String, Option<String>)>>,
    /// When the project's facts were last compared.
    project_checked: Option<Instant>,
}

impl<T: ServerTransport> RelayHostTransport<T> {
    /// `inner`'s links plus the routes the device leg reports on `events`;
    /// frames for the browsers go back on `commands`.
    pub fn new(
        inner: T,
        events: mpsc::UnboundedReceiver<LegEvent>,
        commands: mpsc::UnboundedSender<LegCommand>,
    ) -> Self {
        Self {
            inner,
            events,
            commands,
            routes: BTreeMap::new(),
            next_id: RELAY_LINK_IDS,
            nonce: lpa_client::transport_serial::fresh_link_nonce(),
            inbox: Vec::new(),
            secure_events: Vec::new(),
            closed: Vec::new(),
            came_up: Vec::new(),
            state: RelayState::Off,
            picture_wanted: false,
            picture: Vec::with_capacity(MAX_BOARD_PICTURE_FRAME),
            lamps: Vec::with_capacity(MAX_PICTURE_OUTPUTS),
            project_told: None,
            project_checked: None,
        }
    }

    /// The server loop's per-tick hook: each relay session that came up
    /// its hello first, then the picture the hub asked for (at most one a
    /// tick), then the project's facts if a second has passed since they
    /// were last compared.
    pub fn after_tick(&mut self, server: &LpServer) {
        self.send_hellos(server);
        if std::mem::take(&mut self.picture_wanted) {
            self.send_picture(server);
        }
        if self
            .project_checked
            .is_none_or(|at| at.elapsed() >= PROJECT_CHECK_EVERY)
        {
            self.report_project(server);
        }
    }

    /// Tell the leg the server's project now, if it changed since the leg
    /// was last told (always, the first time). Call it once before the
    /// server loop starts, so the client holds the facts before its first
    /// registration; [`Self::after_tick`] keeps them current. Allocates
    /// only on a change.
    pub fn report_project(&mut self, server: &LpServer) {
        self.project_checked = Some(Instant::now());
        let facts = server.loaded_project_facts();
        let same = match (&self.project_told, facts) {
            (Some(None), None) => true,
            (Some(Some((name, uid))), Some(facts)) => {
                name == facts.name && uid.as_deref() == facts.uid
            }
            _ => false,
        };
        if same {
            return;
        }
        let told = facts.map(|facts| (facts.name.to_string(), facts.uid.map(str::to_string)));
        let report = told.as_ref().map(|(name, uid)| RelayProjectFacts {
            name: name.clone(),
            uid: uid.clone(),
            content_hash: None,
        });
        self.project_told = Some(told);
        let _ = self.commands.send(LegCommand::Project(report));
    }

    /// Make the picture the hub asked for, in place, and hand it to the
    /// leg: the first loaded project's outputs, at most sixteen, sampled to
    /// at most [`DEFAULT_PICTURE_SAMPLES`] colours (the empty picture when
    /// nothing is loaded).
    fn send_picture(&mut self, server: &LpServer) {
        server.output_picture_lamps(MAX_PICTURE_OUTPUTS, &mut self.lamps);
        let total = self.lamps.iter().map(|&lamps| u64::from(lamps)).sum();
        let count = picture_sample_count(total, DEFAULT_PICTURE_SAMPLES);
        if write_picture_header(&mut self.picture, &self.lamps, count).is_err() {
            // A shape the hub would refuse (a lamp sum past u32): no
            // picture; the client asks again at its next due time.
            return;
        }
        server.append_output_picture(&self.lamps, u32::from(count), &mut self.picture);
        let _ = self
            .commands
            .send(LegCommand::Picture(self.picture.clone()));
    }

    /// Send each relay session that came up since the last call its hello
    /// (the board's first message on every `Up`), built for that link.
    fn send_hellos(&mut self, server: &LpServer) {
        for id in std::mem::take(&mut self.came_up) {
            let Some(route) = self.routes.values_mut().find(|route| route.owns(id)) else {
                continue;
            };
            let Some(link) = route.link() else {
                continue;
            };
            let hello = server.hello_for_link(link);
            route.send(
                id,
                &WireServerMessage::new(0, WireServerMsgBody::Hello(hello)),
            );
        }
        self.pump();
    }

    /// Take the device leg's news, then run every route.
    fn pump(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                LegEvent::RouteOpened(route) => {
                    let first = self.mint_id();
                    self.nonce = self.nonce.wrapping_add(0x9e37_79b9) | 1;
                    self.routes
                        .insert(route, RelayRouteLink::new(first, self.nonce));
                }
                LegEvent::RouteFrame { route, bytes } => {
                    if let Some(link) = self.routes.get_mut(&route) {
                        link.on_frame(&bytes);
                    }
                }
                LegEvent::RouteClosed(route) => {
                    if let Some(link) = self.routes.remove(&route)
                        && let Some(id) = link.close()
                    {
                        self.closed.push(id);
                    }
                }
                LegEvent::State(state) => {
                    if state != self.state {
                        log::info!("relay: {state}");
                    }
                    self.state = state;
                }
                LegEvent::TakePicture => self.picture_wanted = true,
            }
        }
        let mut next_id = self.next_id;
        let mut finished = Vec::new();
        for (route, link) in &mut self.routes {
            let out = link.pump(&mut || {
                let id = LinkId::new(next_id);
                next_id += 1;
                id
            });
            for bytes in out.frames {
                let _ = self.commands.send(LegCommand::RouteSend {
                    route: *route,
                    bytes,
                });
            }
            self.inbox.extend(out.inbox);
            self.secure_events.extend(out.events);
            self.closed.extend(out.closed);
            self.came_up.extend(out.came_up);
            if out.reset {
                finished.push(*route);
            }
        }
        self.next_id = next_id;
        for route in finished {
            // A session that reset ends its route, as a board ends a keyed
            // link whose session resets; the browser reconnects.
            self.routes.remove(&route);
            let _ = self.commands.send(LegCommand::RouteClose { route });
        }
    }

    fn mint_id(&mut self) -> LinkId {
        let id = LinkId::new(self.next_id);
        self.next_id += 1;
        id
    }

    fn is_relay_link(link: LinkId) -> bool {
        link.raw() >= RELAY_LINK_IDS
    }
}

impl<T: ServerTransport> ServerTransport for RelayHostTransport<T> {
    async fn send(&mut self, link: LinkId, msg: WireServerMessage) -> Result<(), TransportError> {
        if !Self::is_relay_link(link) {
            return self.inner.send(link, msg).await;
        }
        if let Some(route) = self.routes.values_mut().find(|route| route.owns(link)) {
            route.send(link, &msg);
        }
        self.pump();
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        if let Some(incoming) = self.inner.receive().await? {
            return Ok(Some(incoming));
        }
        self.pump();
        Ok(if self.inbox.is_empty() {
            None
        } else {
            Some(self.inbox.remove(0))
        })
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        let mut all = self.inner.receive_all().await?;
        self.pump();
        all.append(&mut self.inbox);
        Ok(all)
    }

    fn links(&self) -> Vec<Link> {
        let mut links = self.inner.links();
        links.extend(self.routes.values().filter_map(RelayRouteLink::link));
        links
    }

    fn take_closed_links(&mut self) -> Vec<LinkId> {
        let mut closed = self.inner.take_closed_links();
        closed.append(&mut self.closed);
        closed
    }

    fn take_secure_events(&mut self) -> Vec<(LinkId, SecureLinkEvent)> {
        let mut events = self.inner.take_secure_events();
        events.append(&mut self.secure_events);
        events
    }

    fn answer_key_lookup(&mut self, link: LinkId, answer: KeyAnswer) {
        if !Self::is_relay_link(link) {
            self.inner.answer_key_lookup(link, answer);
            return;
        }
        if let Some(route) = self.routes.values_mut().find(|route| route.owns(link)) {
            route.answer_key_lookup(link, answer);
        }
        self.pump();
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.inner.close().await
    }
}

/// A transport with no links of its own: the relay alone (tests, and a host
/// board that serves nothing locally).
#[derive(Debug, Default)]
#[allow(
    dead_code,
    reason = "the lp-cli binary always serves locally; lp-cli's relay tests use it"
)]
pub struct NoLocalLinks;

impl ServerTransport for NoLocalLinks {
    async fn send(&mut self, _link: LinkId, _msg: WireServerMessage) -> Result<(), TransportError> {
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        Ok(Vec::new())
    }

    fn links(&self) -> Vec<Link> {
        Vec::new()
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}
