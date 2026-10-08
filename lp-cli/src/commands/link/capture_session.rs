//! [`CaptureSession`]: what `lp-cli link capture` does with a board's link,
//! whatever carries it — a serial port or socket ([`super::capture`]), or a
//! Bluetooth frame pipe ([`super::blepipe_capture`]).
//!
//! The transport owns the link ([`WireLinkPort`]) and the clock, and says
//! when a session came up or went down; this owns everything above it: the
//! console file and `--exit-on`, the `--request` queue, and the update host
//! (`--ota-offer`, [`OtaHost`]). One copy of each, so a Bluetooth capture
//! writes the same console a serial one does.

use std::io::Write;

use anyhow::{Context, Result, bail};
use lpc_wire::{PortRead, WireLinkPort};

use super::args::CaptureArgs;
use super::capture_requests::CaptureRequests;
use crate::commands::emu::link_host::console_lines;
use crate::commands::ota_host::OtaHost;

/// Everything a capture keeps above the link. See the module docs.
pub struct CaptureSession {
    console: std::io::LineWriter<std::fs::File>,
    exit_on: Option<String>,
    seconds: u64,
    requests: CaptureRequests,
    ota: Option<OtaHost>,
    lines: usize,
    matched: bool,
}

impl CaptureSession {
    /// The session `args` describes, with `ota` as its update host. Parses
    /// every `--request` and creates the console file before any port is
    /// opened, so a typo fails first.
    pub fn create(args: &CaptureArgs, ota: Option<OtaHost>) -> Result<Self> {
        let requests = CaptureRequests::parse(&args.request)?;
        let file = std::fs::File::create(&args.console)
            .with_context(|| format!("creating the console {}", args.console.display()))?;
        Ok(Self {
            console: std::io::LineWriter::new(file),
            exit_on: args.exit_on.clone(),
            seconds: args.seconds,
            requests,
            ota,
            lines: 0,
            matched: false,
        })
    }

    /// A console line contained `--exit-on`: the capture stops now.
    pub fn matched(&self) -> bool {
        self.matched
    }

    /// The update host, when the capture has one.
    pub fn ota(&self) -> Option<&OtaHost> {
        self.ota.as_ref()
    }

    /// One read off the link, in the order the link gave it: a session
    /// edge restarts the `--request` queue's wait for a hello, and every
    /// console line is written. Stops at the line `--exit-on` names.
    pub fn on_read(&mut self, read: &PortRead) -> Result<()> {
        if matches!(read, PortRead::Up { .. } | PortRead::Reset { .. }) {
            self.requests.on_session_change();
        }
        for line in console_lines(read) {
            self.requests.on_line(&line);
            self.line(&line)?;
            if self.matched {
                break;
            }
        }
        Ok(())
    }

    /// A line of the host's own, for the console (and checked against
    /// `--exit-on` like the board's).
    pub fn line(&mut self, line: &str) -> Result<()> {
        writeln!(self.console, "{line}")?;
        self.lines += 1;
        if self
            .exit_on
            .as_deref()
            .is_some_and(|needle| line.contains(needle))
        {
            self.matched = true;
        }
        Ok(())
    }

    /// The link's session went away with no `Reset` read to say so (the
    /// transport itself is gone): the `--request` queue's half of a reset.
    pub fn session_lost(&mut self) {
        self.requests.on_session_change();
    }

    /// A link session the update may run on came up.
    pub fn ota_up(&mut self, now_ms: u64) {
        if let Some(ota) = self.ota.as_mut() {
            ota.link_up(now_ms);
        }
    }

    /// The update's link session went away.
    pub fn ota_down(&mut self, now_ms: u64) {
        if let Some(ota) = self.ota.as_mut() {
            ota.link_down(now_ms);
        }
    }

    /// Run the update host against `link`: the board's channel-3 messages
    /// in, time, the host's messages out (as many as the link's send ring
    /// takes; the rest wait), and its lines onto the console and stderr.
    pub fn pump_ota(&mut self, link: &mut WireLinkPort, now_ms: u64) -> Result<()> {
        let Some(ota) = self.ota.as_mut() else {
            return Ok(());
        };
        while let Some(message) = link.poll_update() {
            ota.on_board(now_ms, &message);
        }
        ota.tick(now_ms);
        while let Some(message) = ota.next_outgoing() {
            match link.send_update(message) {
                Ok(()) => ota.sent(),
                // The send ring is full: it drains as frames go out.
                Err(lp_link::SendError::Full) => break,
                Err(error) => bail!("the link refused an update message: {error:?}"),
            }
        }
        for line in ota.take_lines() {
            eprintln!("link capture: {line}");
            self.line(&line)?;
            if self.matched {
                break;
            }
        }
        Ok(())
    }

    /// Time passes with no link to the board (a LAN link being dialled
    /// again after a reset): the update host's clock, and its lines.
    pub fn pump_ota_idle(&mut self, now_ms: u64) -> Result<()> {
        let Some(ota) = self.ota.as_mut() else {
            return Ok(());
        };
        ota.tick(now_ms);
        for line in ota.take_lines() {
            eprintln!("link capture: {line}");
            self.line(&line)?;
        }
        Ok(())
    }

    /// Send the next `--request` if one is due.
    pub fn pump_requests(&mut self, link: &mut WireLinkPort) -> Result<()> {
        if let Some(message) = self.requests.due() {
            match link.send_client(message) {
                Ok(()) => self.requests.sent(),
                // The send ring is full: it drains as frames go out.
                Err(lp_link::SendError::Full) => {}
                Err(error) => bail!("the link refused a --request: {error:?}"),
            }
        }
        Ok(())
    }

    /// End the run: flush the console, say how it went (`host_link` is the
    /// transport's account of its link, `notes` its own lines), and fail
    /// it when a `--request` never finished or `--exit-on` never matched.
    pub fn finish(mut self, elapsed_s: f64, host_link: &str, notes: &[String]) -> Result<()> {
        self.console.flush()?;
        eprintln!(
            "link capture: {} after {elapsed_s:.3} s, {} console lines; host link — {host_link}",
            if self.matched {
                "stopped on --exit-on"
            } else {
                "reached its deadline"
            },
            self.lines,
        );
        for note in notes {
            eprintln!("link capture: {note}");
        }
        if let Some(summary) = self.requests.describe() {
            eprintln!("link capture: {summary}");
        }
        if let Some(ota) = &self.ota {
            eprintln!("link capture: ota — {}", ota.summary());
        }
        if let Some(why) = self.requests.unfinished() {
            bail!("{why}");
        }
        if let Some(needle) = &self.exit_on
            && !self.matched
        {
            bail!(
                "no console line contained `{needle}` within {} s",
                self.seconds
            );
        }
        Ok(())
    }
}
