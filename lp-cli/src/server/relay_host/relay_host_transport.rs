//! The host board's server transport with the relay in it: everything the
//! inner transport (`serve`'s WebSocket server, or nothing) carries, plus
//! one secure lp-link responder per relay route.
//!
//! The routes' server links are `LinkTrust::Relayed`, numbered from
//! [`RELAY_LINK_IDS`] so they never meet the inner transport's. Each server
//! tick pumps the routes (frames in from the device leg, requests and
//! handshake events out, frames back to the leg); a link that came up is
//! owed its hello, which [`RelayHostTransport::send_hellos`] sends after
//! the tick.

use std::collections::BTreeMap;

use lpa_server::LpServer;
use lpc_relay::RelayState;
use lpc_shared::transport::{Incoming, KeyAnswer, Link, LinkId, SecureLinkEvent, ServerTransport};
use lpc_wire::{TransportError, WireServerMessage, WireServerMsgBody};
use tokio::sync::mpsc;

use super::relay_device_leg::{LegCommand, LegEvent};
use super::relay_route_link::RelayRouteLink;

/// The first server link id a relay session gets.
pub const RELAY_LINK_IDS: u32 = 1_000_000;

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
        }
    }

    /// Send each relay session that came up since the last call its hello
    /// (the board's first message on every `Up`), built for that link.
    pub fn send_hellos(&mut self, server: &LpServer) {
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
