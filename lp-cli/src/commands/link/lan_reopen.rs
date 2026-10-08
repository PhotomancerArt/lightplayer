//! `link capture`'s LAN link across an over-the-air update's resets (OTA
//! Wi-Fi plan WD7).
//!
//! An update resets the board three times, and each time its Wi-Fi link goes
//! with it: the board rejoins its network and serves `ws://<board>/link`
//! again, in core-only between the pieces (no server, so no hello). This
//! dials it again:
//!
//! - **on a bounded backoff**: 250 ms, 1 s, 2 s, then every 2 s, for as long
//!   as an update may be away ([`REOPEN_GAP`], the update activity's own
//!   `UPDATE_GAP_MS`), counted from when the link was lost;
//! - **with the key the last session came up on** (`LanLink::key`): a locked
//!   board's core-only answers that key from its own store, where it has no
//!   `LoginBegin` to learn a password's key from; then anonymously, then by
//!   the password (`LanLink::open_for_update`);
//! - **at the address the user gave**, and when that stops answering, at
//!   the board's `lp-xxxx.local` name (learned from its hello's MAC, the
//!   name its mDNS answers): a board whose address moved across a reset is
//!   found again, and the run says which address it used;
//! - **"busy" is a refusal like any other** while reconnecting (one Wi-Fi
//!   client at a time: Studio may hold it for a moment); before the update
//!   starts it is one plain sentence and the end of the run
//!   ([`BUSY_WORDS`]).

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use lpa_client::HostSpecifier;
use lpa_client::transport_lan::{
    LanError, LanLink, LanOptions, LanSocket, LanTarget, LanUpdateSession,
};
use lpc_wire::lp_link::secure_channel::{KeyId, Psk};
use lpc_wire::{PortRead, ServerHello, WireLinkPort};

use super::args::CaptureArgs;

/// How long an update's link may be away before the run gives up on it:
/// the update activity's gap (`lpa_devices::activity::UPDATE_GAP_MS`).
pub const REOPEN_GAP: Duration = Duration::from_millis(lpa_devices::activity::UPDATE_GAP_MS);

/// The pauses before each try, in order; the last repeats.
const BACKOFF: [Duration; 3] = [
    Duration::from_millis(250),
    Duration::from_secs(1),
    Duration::from_secs(2),
];

/// Tries at the given address before its `.local` name is tried too.
const TRIES_BEFORE_NAME: u32 = 3;

/// A board busy with another client, in words.
pub const BUSY_WORDS: &str = "the board is busy with another connection (one Wi-Fi client at a \
                              time) — try again when Studio lets go of it";

/// A LAN link opened: its socket, its port, its clock, and what it said
/// first.
pub struct LanOpened {
    pub socket: LanSocket,
    pub link: WireLinkPort,
    pub clock: Instant,
    pub early: Vec<PortRead>,
}

/// See the module docs.
pub struct LanReopen {
    given: LanTarget,
    /// `lp-xxxx.local`, once a hello said the board's MAC.
    name: Option<LanTarget>,
    options: LanOptions,
    /// The key the last session came up on.
    held: Option<(KeyId, Psk)>,
    /// When the link was lost, and how many tries since.
    lost_at: Option<Instant>,
    tries: u32,
    next_try: Option<Instant>,
    /// Where the last open went.
    last: String,
}

impl LanReopen {
    /// For `args`' `lan:` target (its password, if any, from stdin or
    /// `LP_PASSWORD`).
    pub fn new(args: &CaptureArgs) -> Result<Self> {
        let spec = HostSpecifier::parse(&args.target)?;
        let given = LanTarget::from_specifier(&spec)
            .with_context(|| format!("{} is not a lan: address", args.target))?;
        let options = LanOptions {
            password: args.board_password.resolve(&spec)?,
            want_packed: !args.json_replies,
            held_keys: Vec::new(),
        };
        Ok(Self {
            last: given.url(),
            given,
            name: None,
            options,
            held: None,
            lost_at: None,
            tries: 0,
            next_try: None,
        })
    }

    /// The first open, at the given address.
    pub fn open_first(&mut self) -> Result<LanOpened, LanError> {
        let target = self.given.clone();
        self.open_at(&target)
    }

    /// A hello arrived on the link: its MAC names the board on the LAN.
    pub fn saw_hello(&mut self, hello: &ServerHello) {
        if let Some(mac) = &hello.hardware.base_mac
            && let Some(name) = mdns_name(mac)
        {
            self.name = Some(LanTarget::new(name, self.given.port));
        }
    }

    /// The link went away.
    pub fn lost(&mut self) {
        if self.lost_at.is_none() {
            self.lost_at = Some(Instant::now());
            self.tries = 0;
            self.next_try = Some(Instant::now() + BACKOFF[0]);
        }
    }

    /// One try, if one is due: the link opened, `None` while it is not back
    /// yet (nothing waits long here), or an error once [`REOPEN_GAP`] is
    /// spent.
    pub fn try_again(&mut self) -> Result<Option<LanOpened>> {
        let Some(lost_at) = self.lost_at else {
            return Ok(None);
        };
        let now = Instant::now();
        if self.next_try.is_some_and(|at| now < at) {
            std::thread::sleep(Duration::from_millis(10));
            return Ok(None);
        }
        if now.duration_since(lost_at) > REOPEN_GAP {
            bail!(
                "the board did not come back on {} within {} s",
                self.given.url(),
                REOPEN_GAP.as_secs()
            );
        }
        self.tries += 1;
        let pause = BACKOFF[(self.tries as usize).min(BACKOFF.len() - 1)];
        self.next_try = Some(now + pause);
        let mut targets = vec![self.given.clone()];
        if self.tries > TRIES_BEFORE_NAME
            && let Some(name) = &self.name
            && name.url() != self.given.url()
        {
            targets.push(name.clone());
        }
        for target in targets {
            match self.open_at(&target) {
                Ok(opened) => return Ok(Some(opened)),
                Err(error) => {
                    eprintln!(
                        "link capture: {} not back yet (try {}): {error}",
                        target.url(),
                        self.tries
                    );
                }
            }
        }
        Ok(None)
    }

    /// The line for a link that came back: where, and how long it was away.
    pub fn back_line(&mut self, run: Instant) -> String {
        let away = self
            .lost_at
            .take()
            .map_or(0.0, |at| at.elapsed().as_secs_f64());
        let moved = if self.last == self.given.url() {
            String::new()
        } else {
            format!(" (its address moved: found it as {})", self.last)
        };
        format!(
            "LAN link back at {:.3} s after {away:.1} s away, {} tries{moved}",
            run.elapsed().as_secs_f64(),
            self.tries
        )
    }

    fn open_at(&mut self, target: &LanTarget) -> Result<LanOpened, LanError> {
        let mut options = self.options.clone();
        options.held_keys = self.held.iter().cloned().collect();
        let LanUpdateSession { link, hello, early } =
            LanLink::open_for_update(&target.endpoint(), &options)?;
        if let Some(key) = link.key() {
            self.held = Some(key);
        }
        if let Some(hello) = &hello {
            self.saw_hello(hello);
        }
        self.last = target.url();
        // The session's `Up` is in `early` (core-only says nothing else).
        let (socket, link, clock) = link.into_parts();
        Ok(LanOpened {
            socket,
            link,
            clock,
            early,
        })
    }
}

/// `lp-xxxx.local` from a MAC as the hello spells it (`A0:F2:…:B4:8C`, any
/// separator, any case): `lp-` and its last two bytes, lowercase — the name
/// the board's mDNS answers (`fw_esp32_common::net::mdns::mdns_host`).
#[must_use]
pub fn mdns_name(mac: &str) -> Option<String> {
    let hex: String = mac.chars().filter(char::is_ascii_hexdigit).collect();
    if hex.len() != 12 {
        return None;
    }
    Some(format!("lp-{}.local", hex[8..].to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_is_the_macs_last_two_bytes_as_the_board_answers_it() {
        assert_eq!(
            mdns_name("A0:F2:62:87:B4:8C").as_deref(),
            Some("lp-b48c.local")
        );
        assert_eq!(mdns_name("10bda3b08e30").as_deref(), Some("lp-8e30.local"));
        assert_eq!(mdns_name("A0:F2:62"), None);
    }

    #[test]
    fn the_backoff_is_bounded_by_the_updates_gap() {
        assert_eq!(REOPEN_GAP, Duration::from_secs(90));
        assert_eq!(BACKOFF.last(), Some(&Duration::from_secs(2)));
        assert!(BUSY_WORDS.contains("one Wi-Fi client at a time"));
    }
}
