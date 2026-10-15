//! The host board's device leg: `lpc-relay`'s own [`RelayClient`] — the
//! state machine the C6 runs — driven by a tokio task over a plain
//! WebSocket to `<relay>/relay/device`.
//!
//! The task owns the socket; the server loop owns the routes and the
//! server. They talk over two channels: [`LegEvent`]s out (a route opened,
//! a frame for a route, a route closed, the relay's state, a picture
//! wanted), [`LegCommand`]s in (a frame for the browser, close a route, the
//! picture taken, the project's facts).
//!
//! **Pictures** (relay protocol 2): the client's `TakePicture` becomes
//! [`LegEvent::TakePicture`]; the loop answers with the whole `Picture`
//! frame ([`LegCommand::Picture`]), which the leg holds and reports as
//! `PictureReady`; the client then says `SendPicture` (one binary message)
//! or `DropPicture`. A picture never answered is asked for again at the
//! client's next due time, so a repeated `TakePicture` is normal.

use std::time::{Duration, Instant};

use futures_util::{SinkExt as _, StreamExt as _};
use lpa_client::transport_lan::os_entropy;
use lpc_relay::{
    RELAY_DEVICE_PATH, RelayAccount, RelayAction, RelayClient, RelayClientConfig, RelayEvent,
    RelayProjectFacts, RelayState,
};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// What the leg tells the server loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegEvent {
    RouteOpened(u16),
    RouteFrame {
        route: u16,
        bytes: Vec<u8>,
    },
    RouteClosed(u16),
    State(RelayState),
    /// The hub wants a picture: answer with [`LegCommand::Picture`].
    TakePicture,
}

/// What the server loop asks of the leg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegCommand {
    RouteSend {
        route: u16,
        bytes: Vec<u8>,
    },
    RouteClose {
        route: u16,
    },
    /// The picture [`LegEvent::TakePicture`] asked for: one whole
    /// `RelayFrame::Picture` message, sent as it is if the client still
    /// wants it.
    Picture(Vec<u8>),
    /// The server's project as it now stands (`None`: nothing loaded).
    Project(Option<RelayProjectFacts>),
}

/// Run the leg until the server loop goes away (its command channel
/// closes). `config.host`/`config.port` is the relay's plain-HTTP address.
pub async fn run_device_leg(
    config: RelayClientConfig,
    accounts: Vec<RelayAccount>,
    events: mpsc::UnboundedSender<LegEvent>,
    mut commands: mpsc::UnboundedReceiver<LegCommand>,
) {
    let started = Instant::now();
    let now = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let host = config.host.clone();
    let mut client = RelayClient::new(config, os_entropy);
    let mut ws: Option<Ws> = None;
    // The picture the loop made, until the client says send or drop it.
    let mut picture: Option<Vec<u8>> = None;
    let mut pending = Vec::new();
    pending.extend(client.handle(now(), RelayEvent::Network { joined: true }));
    pending.extend(client.handle(now(), RelayEvent::CloudRelay(true)));
    pending.extend(client.handle(now(), RelayEvent::Accounts(accounts)));
    let mut reported = None;
    loop {
        while !pending.is_empty() {
            for action in std::mem::take(&mut pending) {
                match action {
                    RelayAction::Resolve { host } => {
                        let addr = resolve_v4(&host).await;
                        pending.extend(client.handle(now(), RelayEvent::Resolved(addr)));
                    }
                    RelayAction::Connect { addr, port } => match connect(&host, addr, port).await {
                        Some(socket) => {
                            ws = Some(socket);
                            pending.extend(client.handle(now(), RelayEvent::Connected));
                        }
                        None => pending
                            .extend(client.handle(now(), RelayEvent::Closed { going_away: false })),
                    },
                    RelayAction::Send(bytes) => {
                        if let Some(socket) = ws.as_mut()
                            && socket.send(Message::Binary(bytes)).await.is_err()
                        {
                            ws = None;
                            pending.extend(
                                client.handle(now(), RelayEvent::Closed { going_away: false }),
                            );
                        }
                    }
                    RelayAction::Close => {
                        if let Some(mut socket) = ws.take() {
                            let _ = socket.close(None).await;
                        }
                    }
                    RelayAction::RouteOpened(route) => {
                        let _ = events.send(LegEvent::RouteOpened(route));
                    }
                    RelayAction::RouteFrame { route, bytes } => {
                        let _ = events.send(LegEvent::RouteFrame { route, bytes });
                    }
                    RelayAction::RouteClosed(route) => {
                        let _ = events.send(LegEvent::RouteClosed(route));
                    }
                    RelayAction::TakePicture => {
                        let _ = events.send(LegEvent::TakePicture);
                    }
                    RelayAction::SendPicture => {
                        if let (Some(bytes), Some(socket)) = (picture.take(), ws.as_mut())
                            && socket.send(Message::Binary(bytes)).await.is_err()
                        {
                            ws = None;
                            pending.extend(
                                client.handle(now(), RelayEvent::Closed { going_away: false }),
                            );
                        }
                    }
                    RelayAction::DropPicture => picture = None,
                }
            }
        }
        let state = client.state();
        if reported != Some(state) {
            reported = Some(state);
            if events.send(LegEvent::State(state)).is_err() {
                return;
            }
        }
        let wake = client.next_wake().map_or(Duration::from_secs(3600), |at| {
            Duration::from_millis(at.saturating_sub(now()))
        });
        let message = async {
            match ws.as_mut() {
                Some(socket) => socket.next().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            message = message => match message {
                Some(Ok(Message::Binary(bytes))) => {
                    pending.extend(client.handle(now(), RelayEvent::Message(&bytes)));
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {
                    pending.extend(client.handle(now(), RelayEvent::Heard));
                }
                Some(Ok(Message::Close(frame))) => {
                    let going_away = frame.is_some_and(|frame| u16::from(frame.code) == 1001);
                    ws = None;
                    pending.extend(client.handle(now(), RelayEvent::Closed { going_away }));
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => {
                    ws = None;
                    pending.extend(client.handle(now(), RelayEvent::Closed { going_away: false }));
                }
            },
            command = commands.recv() => match command {
                Some(LegCommand::RouteSend { route, bytes }) => pending.extend(
                    client.handle(now(), RelayEvent::RouteSend { route, bytes: &bytes }),
                ),
                Some(LegCommand::Picture(bytes)) => {
                    picture = Some(bytes);
                    pending.extend(client.handle(now(), RelayEvent::PictureReady));
                }
                Some(LegCommand::Project(facts)) => {
                    pending.extend(client.handle(now(), RelayEvent::Project(facts)));
                }
                Some(LegCommand::RouteClose { route }) => {
                    pending.extend(client.handle(
                        now(),
                        RelayEvent::RouteClose {
                            route,
                            reason: lpc_relay::RouteCloseReason::Normal,
                        },
                    ));
                }
                None => {
                    if let Some(mut socket) = ws.take() {
                        let _ = socket.close(None).await;
                    }
                    return;
                }
            },
            () = tokio::time::sleep(wake) => {
                pending.extend(client.handle(now(), RelayEvent::Tick));
            }
        }
    }
}

/// The first IPv4 address `host` resolves to (the board's resolver answers
/// one A record; the host's does the same here).
async fn resolve_v4(host: &str) -> Option<[u8; 4]> {
    let addrs = tokio::net::lookup_host((host, 0)).await.ok()?;
    addrs.into_iter().find_map(|addr| match addr.ip() {
        std::net::IpAddr::V4(ip) => Some(ip.octets()),
        std::net::IpAddr::V6(_) => None,
    })
}

/// TCP to `addr:port`, then the upgrade on the device path with `host` as
/// the `Host` — plain HTTP, as a board dials.
async fn connect(host: &str, addr: [u8; 4], port: u16) -> Option<Ws> {
    let tcp = tokio::time::timeout(
        Duration::from_secs(10),
        TcpStream::connect((std::net::Ipv4Addr::from(addr), port)),
    )
    .await
    .ok()?
    .ok()?;
    let _ = tcp.set_nodelay(true);
    let request = format!("ws://{host}:{port}{RELAY_DEVICE_PATH}")
        .into_client_request()
        .ok()?;
    let (socket, _) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::client_async(request, MaybeTlsStream::Plain(tcp)),
    )
    .await
    .ok()?
    .ok()?;
    Some(socket)
}
