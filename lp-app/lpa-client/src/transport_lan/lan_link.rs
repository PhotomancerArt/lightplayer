//! One LAN link to a board: a secure lp-link initiator (lpc-wire's
//! [`WireLinkPort`] on [`LinkConfig::ws`]) over a [`LanSocket`], one frame
//! per WebSocket message ([`WireLinkPort::on_datagram`], never `on_bytes`).
//!
//! [`LanLink::open`] is the whole way in, and how it picks its key:
//!
//! 1. **Anonymous first.** The anonymous key (`KeyId::ANONYMOUS`,
//!    `Psk::ANONYMOUS`) encrypts and authenticates nobody; the board's hello
//!    on that session says what anyone nearby holds. An open board is done
//!    here (and so is any board when no password was given and it grants
//!    something).
//! 2. **A locked board, no password:** refused in words
//!    ([`LanError::Locked`]).
//! 3. **A password:** a `LoginBegin` on the anonymous session lists the
//!    board's offers; that session closes, and a NEW connection opens with
//!    the password's key against each stretched offer in turn
//!    ([`super::lan_keys`]): a wrong key is answered `WrongKey` by the board
//!    and the next offer is tried on the same connection
//!    ([`WireLinkPort::retry_with`]); when none fits, the password is wrong
//!    ([`LanError::WrongPassword`]). The key that verifies is the login: the
//!    board grants its entry's tier at `Up` (an HMAC `LoginAnswer` never
//!    grants on a keyed link). That session's tier is then asked with a
//!    `Hello` request, which the server answers after it has taken the
//!    session's grant — the hello the board writes at `Up` may not have it
//!    yet (see the module docs of `lpa-client`'s `transport_lan`).
//!
//! Sans-IO underneath: time is this link's own monotonic clock (µs since it
//! connected), handed to the port on every call.

use std::time::{Duration, Instant};

use lpc_access::Tier;
use lpc_wire::lp_link::secure_channel::{KeyId, Psk, RefusalReason, SecureEvent};
use lpc_wire::lp_link::{LinkConfig, Micros, SendError};
use lpc_wire::server::ServerMsgBody;
use lpc_wire::{
    ClientMessage, ClientRequest, PACK_OPT_IN_REQUEST_ID, PortRead, ServerHello, WireLinkPort,
};

use super::board_password::BoardPassword;
use super::lan_entropy::os_entropy;
use super::lan_error::LanError;
use super::lan_keys::password_keys;
use super::lan_socket::LanSocket;
use super::lan_target::LanTarget;
use crate::transport_serial::fresh_link_nonce;

/// How long a secure session, its hello, or one setup request may take.
pub const LAN_SETUP_BUDGET: Duration = Duration::from_secs(10);

/// The longest one [`LanLink::step`] waits for a frame during setup.
const SETUP_STEP: Duration = Duration::from_millis(10);

/// Request ids of the setup requests: far from any client's counter, below
/// the ones the port itself uses (the pack opt-in, the log level).
const LOGIN_BEGIN_ID: u64 = PACK_OPT_IN_REQUEST_ID - 2;
const HELLO_ID: u64 = PACK_OPT_IN_REQUEST_ID - 3;

/// What [`LanLink::open`] needs beyond the address.
#[derive(Debug, Clone, Default)]
pub struct LanOptions {
    /// The board's password, for a locked board (stdin or `LP_PASSWORD`).
    pub password: Option<BoardPassword>,
    /// Ask the board to pack its replies (`LP_WIRE_ENCODING`'s rule, the
    /// serial transports' own).
    pub want_packed: bool,
}

/// An open link whose secure session is up and whose tier is known.
pub struct LanSession {
    pub link: LanLink,
    /// The board's hello for this session (its tier the one that stands).
    pub hello: ServerHello,
    /// The tier the session holds.
    pub granted: Tier,
    /// Everything the session said before `open` returned (its `Up`, the
    /// hello, notes, any heartbeat), in order — minus the answers to
    /// `open`'s own requests.
    pub early: Vec<PortRead>,
}

/// One secure lp-link over one WebSocket.
pub struct LanLink {
    socket: LanSocket,
    port: WireLinkPort,
    started: Instant,
    target: LanTarget,
}

impl LanLink {
    /// Connect to `target` and come up with the right key (module docs).
    pub fn open(target: &LanTarget, options: &LanOptions) -> Result<LanSession, LanError> {
        let mut link = Self::connect(
            target,
            KeyId::ANONYMOUS,
            Psk::ANONYMOUS,
            options.want_packed,
        )?;
        let (hello, mut early) = link.come_up(&mut std::iter::empty())?;
        let anyone = hello.auth.granted;
        let password = match &options.password {
            // Nothing more a password could give.
            Some(_) if anyone == Some(Tier::Edit) => None,
            other => other.as_ref(),
        };
        let Some(password) = password else {
            return match anyone {
                Some(granted) => Ok(LanSession {
                    link,
                    hello,
                    granted,
                    early,
                }),
                None => {
                    link.close();
                    Err(LanError::Locked)
                }
            };
        };

        let offers = match link.request(LOGIN_BEGIN_ID, ClientRequest::LoginBegin, &mut early)? {
            ServerMsgBody::LoginChallenge { offers, .. } => offers,
            other => return Err(LanError::Login(reply_words(&other))),
        };
        link.close();
        let mut keys = password_keys(password, &offers).into_iter();
        let Some((key_id, psk)) = keys.next() else {
            return Err(LanError::NoPasswordEntry);
        };
        let mut link = Self::connect(target, key_id, psk, options.want_packed)?;
        let (_, mut early) = link.come_up(&mut keys).map_err(|error| match error {
            LanError::Refused(RefusalReason::WrongKey | RefusalReason::UnknownKey) => {
                LanError::WrongPassword
            }
            other => other,
        })?;
        let hello = match link.request(HELLO_ID, ClientRequest::Hello, &mut early)? {
            ServerMsgBody::Hello(hello) => hello,
            other => return Err(LanError::Login(reply_words(&other))),
        };
        match hello.auth.granted {
            Some(granted) => Ok(LanSession {
                link,
                hello,
                granted,
                early,
            }),
            None => {
                link.close();
                Err(LanError::WrongPassword)
            }
        }
    }

    fn connect(
        target: &LanTarget,
        key_id: KeyId,
        psk: Psk,
        want_packed: bool,
    ) -> Result<Self, LanError> {
        let socket = LanSocket::connect(target)?;
        let port = WireLinkPort::new_secure(
            LinkConfig::ws(),
            fresh_link_nonce(),
            want_packed,
            key_id,
            psk,
            os_entropy,
        );
        Ok(Self {
            socket,
            port,
            started: Instant::now(),
            target: target.clone(),
        })
    }

    /// This link's clock: µs since it connected.
    pub fn now(&self) -> Micros {
        u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX)
    }

    /// When this link's clock started.
    pub fn started(&self) -> Instant {
        self.started
    }

    /// The port: requests in, reads out.
    pub fn port(&mut self) -> &mut WireLinkPort {
        &mut self.port
    }

    /// Where the link goes.
    pub fn target(&self) -> &LanTarget {
        &self.target
    }

    /// Send every frame the port has.
    pub fn flush(&mut self) -> Result<(), LanError> {
        let now = self.now();
        let Self { socket, port, .. } = self;
        while let Some(frame) = port.poll_transmit(now) {
            socket.send(frame)?;
        }
        Ok(())
    }

    /// One pass: frames out, frames in (waiting at most `max_wait`, and no
    /// longer than the link's next timer), frames out again.
    pub fn step(&mut self, max_wait: Duration) -> Result<(), LanError> {
        self.flush()?;
        let now = self.now();
        let wait = self.port.poll_timeout().map_or(max_wait, |at| {
            Duration::from_micros(at.saturating_sub(now)).min(max_wait)
        });
        let mut next = self.socket.recv(wait)?;
        while let Some(frame) = next {
            let now = self.now();
            self.port.on_datagram(now, &frame);
            next = self.socket.recv(Duration::ZERO)?;
        }
        self.flush()
    }

    /// The socket and the port, apart, with the port's clock origin: for a
    /// caller that runs its own loop (`lp-cli link rtt|capture`).
    pub fn into_parts(self) -> (LanSocket, WireLinkPort, Instant) {
        (self.socket, self.port, self.started)
    }

    /// Close the WebSocket.
    pub fn close(self) {
        self.socket.close();
    }

    /// Step until the session is up and its hello has arrived; on a refusal
    /// of a wrong or unknown key, try the next of `more_keys`.
    fn come_up(
        &mut self,
        more_keys: &mut dyn Iterator<Item = (KeyId, Psk)>,
    ) -> Result<(ServerHello, Vec<PortRead>), LanError> {
        let deadline = Instant::now() + LAN_SETUP_BUDGET;
        let mut early = Vec::new();
        while Instant::now() < deadline {
            self.step(SETUP_STEP).map_err(|error| self.busy_or(error))?;
            while let Some(event) = self.port.poll_secure_event() {
                match event {
                    SecureEvent::Refused {
                        reason: reason @ (RefusalReason::WrongKey | RefusalReason::UnknownKey),
                        ..
                    } => match more_keys.next() {
                        Some((key_id, psk)) => self.port.retry_with(key_id, psk),
                        None => return Err(self.closing(LanError::Refused(reason))),
                    },
                    SecureEvent::Refused {
                        reason: RefusalReason::Backoff,
                        retry_after_ms,
                    } => return Err(self.closing(LanError::Backoff { retry_after_ms })),
                    SecureEvent::Refused { reason, .. } => {
                        return Err(self.closing(LanError::Refused(reason)));
                    }
                    SecureEvent::PeerNotSecure => return Err(self.closing(LanError::NotSecure)),
                    // Responder events never reach an initiator.
                    SecureEvent::KeyLookup { .. } | SecureEvent::WrongKey { .. } => {}
                }
            }
            while let Some(read) = self.port.poll_read() {
                if let PortRead::Message(payload) = &read
                    && let Ok(message) = &payload.message
                    && let ServerMsgBody::Hello(hello) = &message.msg
                {
                    let hello = hello.clone();
                    early.push(read);
                    return Ok((hello, early));
                }
                early.push(read);
            }
        }
        Err(self.closing(LanError::NoHello {
            target: self.target.to_string(),
            secs: LAN_SETUP_BUDGET.as_secs(),
        }))
    }

    /// Send one setup request and wait for its answer; everything else read
    /// meanwhile goes to `early`.
    fn request(
        &mut self,
        id: u64,
        msg: ClientRequest,
        early: &mut Vec<PortRead>,
    ) -> Result<ServerMsgBody, LanError> {
        self.port
            .send_client(&ClientMessage { id, msg })
            .map_err(|error| LanError::Lost(send_error_words(error).to_string()))?;
        let deadline = Instant::now() + LAN_SETUP_BUDGET;
        while Instant::now() < deadline {
            self.step(SETUP_STEP)?;
            while let Some(read) = self.port.poll_read() {
                match read {
                    PortRead::Message(payload)
                        if payload.message.as_ref().is_ok_and(|m| m.id == id) =>
                    {
                        if let Ok(message) = payload.message {
                            return Ok(message.msg);
                        }
                    }
                    PortRead::Reset { .. } => {
                        return Err(LanError::Lost("the secure session reset".to_string()));
                    }
                    other => early.push(other),
                }
            }
        }
        Err(LanError::Lost(format!(
            "no answer to a setup request within {} s",
            LAN_SETUP_BUDGET.as_secs()
        )))
    }

    /// A socket error during setup: close 1013 is the board saying both its
    /// LAN links are taken.
    fn busy_or(&self, error: LanError) -> LanError {
        match error {
            LanError::Closed { code: Some(1013) } => LanError::Busy {
                target: self.target.to_string(),
            },
            other => other,
        }
    }

    /// `error`, after closing the socket (the board frees the slot).
    fn closing(&mut self, error: LanError) -> LanError {
        // A refusal or a timeout leaves nothing to say on this connection;
        // `close` consumes the link, so the socket is shut here by hand.
        self.socket.shutdown();
        error
    }
}

/// A setup request's unexpected answer, in words.
fn reply_words(body: &ServerMsgBody) -> String {
    match body {
        ServerMsgBody::Error { error } => error.clone(),
        ServerMsgBody::NotPermitted { needs } => {
            format!("it needs the {} tier", tier_words(*needs))
        }
        _ => "an unexpected reply".to_string(),
    }
}

/// A tier as the wire spells it.
pub fn tier_words(tier: Tier) -> &'static str {
    match tier {
        Tier::Play => "play",
        Tier::Edit => "edit",
    }
}

fn send_error_words(error: SendError) -> &'static str {
    match error {
        SendError::Full => "the link's send budget is full",
        SendError::TooBig => "the request is larger than the link carries",
        SendError::BadChannel => "the link has no such channel",
    }
}
