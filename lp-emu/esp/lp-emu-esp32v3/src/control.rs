//! The host control channel — **the cable, not a peripheral**.
//!
//! A byte socket carries bytes and nothing else, because `lp-cli …
//! serial:tcp://` is a plain byte client and in-band control would be a
//! dialect every client would have to speak. So the host's *side* of the
//! serial link — the bridge chip, the port, the DTR/RTS lines, the reset
//! dances — rides a **second** socket in its own line protocol, and this
//! module is that protocol: nothing here knows about sockets, the machine,
//! or any register block. It parses strings and formats strings, which is
//! why all of it is unit-tested on strings.
//!
//! # The one thing that is different from the C6, and it is the whole file
//!
//! **The C6's port is a peripheral inside the SoC.** A client attaching
//! moves chip state, because `USB_DEVICE` can see the host: the bus reset,
//! the SOF, the IN endpoint draining are all things the *guest* can observe.
//!
//! **The classic's port is a CH340K on the carrier board.** The SoC has no
//! idea a cable exists. Nothing on the chip changes when a port is opened or
//! closed, and no register anywhere reports it. What resets the chip is the
//! **auto-reset circuit on the board**, driven by the two modem lines.
//!
//! So `attach` / `detach` / `open` / `close` still parse — the verb set is
//! the C6's so that one client library drives both machines — but on this
//! chip they move only the *host's* bookkeeping, which `state` reports and
//! nothing else reads. A byte client connecting to `--uart0 tcp:` is **not**
//! a port open here, and there is no coupling rule to write: a bridge chip
//! is not a port open.
//!
//! # The circuit
//!
//! esptool documents it as *Classic reset*: two transistors, wired so that
//! neither line **alone** can hold both strap lines.
//!
//! ```text
//!     EN  low  ⟺  RTS asserted AND DTR not asserted
//!     IO0 low  ⟺  DTR asserted AND RTS not asserted
//!     both asserted → both EN and IO0 stay high
//! ```
//!
//! | dtr | rts | EN | IO0 | effect | grade |
//! |-----|-----|----|-----|--------|-------|
//! | 0 | 0 | 1 | 1 | run | **measured** (L0: `TIOCMSET 0` released the board) |
//! | 0 | 1 | 0 | 1 | RESET held | **measured** (L0: `TIOCMSET 0x4`, RTS only, drove EN low and reset the board) |
//! | 1 | 0 | 1 | 0 | IO0 low — the download strap | **documented** |
//! | 1 | 1 | 1 | 1 | run (the circuit's whole point) | **documented** |
//!
//! `../bench.md` is L0's measurement and it covers **two rows and no more**.
//! Rows 3 and 4 are esptool's circuit plus this repo's own sequences
//! (`spikes/serial-lab/index.html:341-357`), and they stay `documented`
//! until L1 captures a download-mode entry — ruling **R8/R9** in
//! `m3/notes.md` §6. A model that happens to work is not a measurement, and
//! this file does not launder one into the other.
//!
//! # What a verb does
//!
//! ```text
//! reset          = {dtr:0,rts:1} → hold → {rts:0}
//! download-mode  = {dtr:0,rts:1} → {dtr:1,rts:0} → {dtr:0}
//! ```
//!
//! Both are shorthands for the line sequence, and both are the sequences the
//! repo's own serial lab uses. **The reboot happens on the release**, not on
//! the assert: EN low holds the chip in reset and EN going high is the chip
//! starting, latching IO0's level at that instant as the strap. So
//! `signals dtr=0 rts=1` followed by `signals dtr=0 rts=0` is exactly one
//! reboot into [`Strap::App`], and the download dance is exactly one reboot
//! into [`Strap::Download`] — which is what the circuit does and why the
//! model is written as edges rather than as verbs.
//!
//! # A host-side fact that belongs beside the cable
//!
//! The WCH macOS driver **silently ignores** the single-bit
//! `TIOCMBIS`/`TIOCMBIC` ioctls behind pyserial's and serialport's
//! `.dtr`/`.rts` setters, and honours only whole-status `TIOCMSET`. Verified
//! on hardware and recorded in product code at
//! `lp-app/lpa-client/src/stream/serialport_stream.rs:51-60`, which also
//! notes that espflash carries `UnixTightReset` for the same reason. That
//! constrains the lab script and `scripts/emu/`, **not** this emulator —
//! written down here so nobody re-derives it, and so that a script driving
//! the real board and one driving this socket are known to differ.
//!
//! # Time
//!
//! A command carries no time of its own. It is applied at the next slice
//! boundary, and the reply says at which guest cycle that was — so a socket
//! run's *ordering* is the host's while everything the machine then does is
//! still guest time. The scripted form ([`parse_control_script`]) has the
//! times in the file and is the deterministic path.

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::{ScriptedSource, Strap};

use crate::memmap;

/// Which way the two modem lines are being driven, as the host last set
/// them. `true` is **asserted** — the logical sense pyserial's `.dtr` and
/// Web Serial's `dataTerminalReady` use, not the RS-232 voltage.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Cable {
    pub dtr: bool,
    pub rts: bool,
}

impl Cable {
    /// The chip-enable pin's level. Low means "held in reset".
    pub fn en(self) -> bool {
        !(self.rts && !self.dtr)
    }

    /// GPIO0's level. Low at the moment EN is released means "boot into the
    /// download console".
    pub fn io0(self) -> bool {
        !(self.dtr && !self.rts)
    }

    /// What the chip would strap to if EN were released right now.
    pub fn strap(self) -> Strap {
        if self.io0() {
            Strap::App
        } else {
            Strap::Download
        }
    }
}

/// One command on the control channel.
///
/// [`Wait`](ControlCommand::Wait) is the scripted form's own — a socket
/// client waits by waiting — and the parser consumes it rather than handing
/// it on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlCommand {
    /// The cable goes in. **On this chip that moves no chip state**: it is
    /// the host's bookkeeping and nothing else.
    Attach,
    /// The cable comes out. Also host-side only; the lines go slack, which
    /// on this circuit means both deasserted, which means "run".
    Detach,
    /// An application opened the port. Host-side only — see the module docs.
    Open,
    /// It closed the port. Host-side only.
    Close,
    /// One or both modem lines, as the host last set them. `dtr`/`rts` on
    /// their own set one; `signals dtr=… rts=…` sets both in one write,
    /// which is what whole-status `TIOCMSET` does and the only thing the WCH
    /// macOS driver honours.
    Signals {
        dtr: Option<bool>,
        rts: Option<bool>,
    },
    /// The classic-reset dance, in one line.
    Reset,
    /// The download dance, in one line.
    DownloadMode,
    /// Report the cable's side. Answered by [`ControlReply::State`].
    State,
    /// Scripted form only: shift every later line by this many milliseconds.
    Wait(u64),
}

impl ControlCommand {
    /// The word the `ok` reply echoes. `signals` when a line sets both,
    /// `dtr` / `rts` when it sets one — the client sees its own command
    /// named back.
    pub fn verb(&self) -> &'static str {
        match self {
            ControlCommand::Attach => "attach",
            ControlCommand::Detach => "detach",
            ControlCommand::Open => "open",
            ControlCommand::Close => "close",
            ControlCommand::Signals {
                dtr: Some(_),
                rts: None,
            } => "dtr",
            ControlCommand::Signals {
                dtr: None,
                rts: Some(_),
            } => "rts",
            ControlCommand::Signals { .. } => "signals",
            ControlCommand::Reset => "reset",
            ControlCommand::DownloadMode => "download-mode",
            ControlCommand::State => "state",
            ControlCommand::Wait(_) => "wait",
        }
    }

    /// Every verb the parser knows, in the order the protocol document lists
    /// them. Also what tells a script line's control word from a run of hex
    /// bytes.
    ///
    /// ⚠️ The C6's set minus `usb-write` (there is no USB endpoint to write
    /// into: a host on this board sends bytes down the wire, which is
    /// `--uart0-script` or the byte socket) and minus `pin`/`pins` (the pad
    /// fabric is **P8**'s; a `pin` verb here would answer for a GPIO block
    /// that has no behaviour yet).
    pub const VERBS: &'static [&'static str] = &[
        "attach",
        "detach",
        "open",
        "close",
        "dtr",
        "rts",
        "signals",
        "reset",
        "download-mode",
        "state",
        "wait",
    ];

    /// Parse one line. The caller has already dropped comments and blanks.
    pub fn parse(line: &str) -> Result<Self, String> {
        let line = line.trim();
        let mut words = line.split_whitespace();
        let verb = words.next().ok_or_else(|| "empty command".to_string())?;
        let rest: Vec<&str> = words.collect();

        let no_args = |cmd: ControlCommand| -> Result<ControlCommand, String> {
            if rest.is_empty() {
                Ok(cmd)
            } else {
                Err(format!(
                    "`{verb}` takes no arguments, got `{}`",
                    rest.join(" ")
                ))
            }
        };

        match verb {
            "attach" => no_args(ControlCommand::Attach),
            "detach" => no_args(ControlCommand::Detach),
            "open" => no_args(ControlCommand::Open),
            "close" => no_args(ControlCommand::Close),
            "reset" => no_args(ControlCommand::Reset),
            "download-mode" => no_args(ControlCommand::DownloadMode),
            "state" => no_args(ControlCommand::State),
            "dtr" | "rts" => {
                let [value] = rest[..] else {
                    return Err(format!("`{verb}` takes one argument, 0 or 1"));
                };
                let level = parse_level(verb, value)?;
                Ok(if verb == "dtr" {
                    ControlCommand::Signals {
                        dtr: Some(level),
                        rts: None,
                    }
                } else {
                    ControlCommand::Signals {
                        dtr: None,
                        rts: Some(level),
                    }
                })
            }
            "signals" => {
                let (mut dtr, mut rts) = (None, None);
                for word in &rest {
                    let (name, value) = word.split_once('=').ok_or_else(|| {
                        format!("`signals` takes dtr=<0|1> and/or rts=<0|1>, got `{word}`")
                    })?;
                    match name {
                        "dtr" => dtr = Some(parse_level("dtr", value)?),
                        "rts" => rts = Some(parse_level("rts", value)?),
                        other => {
                            return Err(format!("`signals` has no line `{other}` (dtr, rts)"));
                        }
                    }
                }
                if dtr.is_none() && rts.is_none() {
                    return Err("`signals` needs dtr=<0|1> and/or rts=<0|1>".to_string());
                }
                Ok(ControlCommand::Signals { dtr, rts })
            }
            "wait" => {
                let [value] = rest[..] else {
                    return Err("`wait` takes one argument, a millisecond count".to_string());
                };
                let ms: u64 = value
                    .trim_end_matches("ms")
                    .parse()
                    .map_err(|e| format!("`wait {value}`: {e}"))?;
                Ok(ControlCommand::Wait(ms))
            }
            other => Err(format!(
                "unknown command `{other}` (expected one of: {})",
                Self::VERBS.join(", ")
            )),
        }
    }
}

fn parse_level(name: &str, value: &str) -> Result<bool, String> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        other => Err(format!("`{name} {other}`: expected 0 or 1")),
    }
}

fn parse_hex(words: &[&str]) -> Result<Vec<u8>, String> {
    words
        .iter()
        .map(|h| {
            u8::from_str_radix(h.trim_start_matches("0x"), 16)
                .map_err(|e| format!("`{h}` is not a hex byte: {e}"))
        })
        .collect()
}

/// The cable's side, as the `state` command reports it.
///
/// Both halves in one row: what the host says it has done, and what the
/// **circuit** makes of it. `en` and `io0` are the two pins a scope on the
/// board would read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CableReport {
    /// Is a cable plugged in, as far as the host is concerned? Host-side
    /// bookkeeping: no chip register reports this.
    pub attached: bool,
    /// Has an application opened the port? Also host-side only.
    pub port_open: bool,
    pub cable: Cable,
    /// How many times this run has actually rebooted the chip.
    pub reboots: u64,
}

/// One reply line. Exactly one per command — that is the whole contract, and
/// a client may read it as "the line that starts with `ok` or `err`".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlReply {
    /// `ok <verb> cyc=<cycle> us=<micros>` — applied, at that guest cycle.
    Ok { verb: &'static str, cycle: Cycles },
    /// `ok state cyc=… us=… cable=… port=… dtr=… rts=… en=… io0=… strap=… reboots=…`
    State {
        cycle: Cycles,
        report: CableReport,
    },
    /// `err <reason>` — nothing was applied, and the reason says why.
    Err(String),
}

impl std::fmt::Display for ControlReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControlReply::Ok { verb, cycle } => write!(
                f,
                "ok {verb} cyc={cycle} us={}",
                cycle / memmap::CYCLES_PER_US
            ),
            ControlReply::State { cycle, report } => write!(
                f,
                "ok state cyc={cycle} us={} cable={} port={} dtr={} rts={} en={} io0={} \
                 strap={} reboots={}",
                cycle / memmap::CYCLES_PER_US,
                if report.attached { "attached" } else { "absent" },
                if report.port_open { "open" } else { "closed" },
                u8::from(report.cable.dtr),
                u8::from(report.cable.rts),
                u8::from(report.cable.en()),
                u8::from(report.cable.io0()),
                report.cable.strap(),
                report.reboots,
            ),
            ControlReply::Err(reason) => write!(f, "err {reason}"),
        }
    }
}

/// One emulated millisecond, in cycles.
const MS: Cycles = 1_000 * memmap::CYCLES_PER_US;

/// One line of the script grammar, parsed but not yet placed on a timeline.
#[derive(Debug, PartialEq, Eq)]
enum ScriptLine {
    /// `<ms> <bytes>` — absolute emulated time from cycle zero.
    At { ms: u64, bytes: Vec<u8> },
    /// `after "<line>" [+<ms>] <bytes>` — once the device has said that.
    After {
        needle: Vec<u8>,
        delay_ms: u64,
        bytes: Vec<u8>,
    },
    /// `then +<ms> <bytes>` — a host pacing itself after its own last chunk.
    Then { delay_ms: u64, bytes: Vec<u8> },
    /// `<ms> <verb …>` — the cable, not the wire.
    Command { ms: u64, command: ControlCommand },
}

/// Parse one non-blank, non-comment line of the script grammar.
///
/// Bytes and control words are told apart by the first token: a quoted
/// string is bytes, a token in [`ControlCommand::VERBS`] is a command, and
/// anything else is hex. No verb is a pair of hex digits, so the rule never
/// has to guess.
fn parse_script_line(line: &str) -> Result<ScriptLine, String> {
    if let Some(rest) = line.strip_prefix("after ") {
        let (needle, rest) = take_quoted(rest.trim())?;
        let (delay_ms, rest) = parse_delay(rest.trim())?;
        return Ok(ScriptLine::After {
            needle,
            delay_ms,
            bytes: parse_script_bytes(strip_comment(rest.trim()))?,
        });
    }
    if let Some(rest) = line.strip_prefix("then ") {
        let (delay_ms, rest) = parse_delay(rest.trim())?;
        return Ok(ScriptLine::Then {
            delay_ms,
            bytes: parse_script_bytes(strip_comment(rest.trim()))?,
        });
    }
    let (ms, rest) = line.split_once(char::is_whitespace).ok_or_else(|| {
        "expected `<ms> <bytes or command>`, `after \"<line>\" <bytes>` or `then +<ms> <bytes>`"
            .to_string()
    })?;
    let ms: u64 = ms
        .trim_end_matches("ms")
        .parse()
        .map_err(|e| format!("`{ms}` is not a millisecond count: {e}"))?;
    let rest = strip_comment(rest.trim());
    let first = rest.split_whitespace().next().unwrap_or("");
    if !rest.starts_with('"') && ControlCommand::VERBS.contains(&first) {
        return Ok(ScriptLine::Command {
            ms,
            command: ControlCommand::parse(rest)?,
        });
    }
    Ok(ScriptLine::At {
        ms,
        bytes: parse_script_bytes(rest)?,
    })
}

/// `--uart0-script`: the **byte** half of the grammar on its own.
///
/// One entry per line, and file order is wire order. A line is a
/// double-quoted string with `\n \r \t \\ \" \0 \xNN` escapes, or
/// whitespace-separated hex bytes. `#` starts a comment; blanks are skipped.
///
/// A leading `<ms>` is absolute emulated time from cycle zero.
/// `after "<line>" [+<ms>] <bytes>` sends once the device has printed that
/// line — the form a walk needs, because a host client answers what it hears
/// rather than a wall-clock offset — and `then +<ms> <bytes>` is a host
/// pacing itself. Every wait resolves in **guest** cycles, so two runs of
/// one script deliver the same bytes at the same cycles.
///
/// A control word here is an error naming the flag that does take one, never
/// a line quietly dropped: the wire and the cable are two different sockets
/// on this chip and confusing them is exactly the mistake worth catching.
pub fn parse_byte_script(text: &str) -> Result<ScriptedSource, String> {
    let mut source = ScriptedSource::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |e: String| format!("line {}: {e}", n + 1);
        match parse_script_line(line).map_err(&at)? {
            ScriptLine::At { ms, bytes } => source.push(ms * MS, bytes),
            ScriptLine::After {
                needle,
                delay_ms,
                bytes,
            } => source.push_after(needle, delay_ms * MS, bytes),
            ScriptLine::Then { delay_ms, bytes } => source.push_then(delay_ms * MS, bytes),
            ScriptLine::Command { command, .. } => {
                return Err(at(format!(
                    "`{}` is a cable command and this is the byte script \
                     (--control-script takes those)",
                    command.verb()
                )));
            }
        }
    }
    Ok(source)
}

/// `--control-script`: the **cable** half on its own, `(guest cycle,
/// command)` in file order.
///
/// The deterministic twin of a client on `--control tcp:`. A `wait <ms>`
/// line adds to an offset applied to every **later** line, so a relative
/// script can be written without recomputing every timestamp after an edit.
///
/// A byte line here is an error for the same reason a cable verb is an error
/// in the byte script.
pub fn parse_control_script(text: &str) -> Result<Vec<(Cycles, ControlCommand)>, String> {
    let mut out = Vec::new();
    let mut offset_ms = 0u64;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |e: String| format!("line {}: {e}", n + 1);
        match parse_script_line(line).map_err(&at)? {
            ScriptLine::Command {
                command: ControlCommand::Wait(by),
                ..
            } => offset_ms += by,
            ScriptLine::Command { ms, command } => out.push(((ms + offset_ms) * MS, command)),
            ScriptLine::At { .. } | ScriptLine::After { .. } | ScriptLine::Then { .. } => {
                return Err(at(
                    "this is the cable script and that line is bytes (--uart0-script takes those)"
                        .to_string(),
                ));
            }
        }
    }
    Ok(out)
}

/// An optional leading `+<ms>` delay, and the rest.
fn parse_delay(text: &str) -> Result<(u64, &str), String> {
    let Some(after_plus) = text.strip_prefix('+') else {
        return Ok((0, text));
    };
    let (num, tail) = after_plus
        .split_once(char::is_whitespace)
        .ok_or_else(|| "`+<ms>` needs bytes after it".to_string())?;
    let ms: u64 = num
        .trim_end_matches("ms")
        .parse()
        .map_err(|e| format!("`{num}` is not a millisecond count: {e}"))?;
    Ok((ms, tail.trim()))
}

/// A double-quoted, escaped string at the start of `text`, and the rest.
fn take_quoted(text: &str) -> Result<(Vec<u8>, &str), String> {
    let body = text
        .strip_prefix('"')
        .ok_or_else(|| "expected a double-quoted string".to_string())?;
    let mut end = None;
    let mut escaped = false;
    for (i, c) in body.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => {
                end = Some(i);
                break;
            }
            _ => {}
        }
    }
    let end = end.ok_or_else(|| "unterminated string".to_string())?;
    Ok((unescape(&body[..end])?, &body[end + 1..]))
}

/// A quoted string, or whitespace-separated hex bytes.
fn parse_script_bytes(rest: &str) -> Result<Vec<u8>, String> {
    if rest.starts_with('"') {
        let (bytes, tail) = take_quoted(rest)?;
        if !tail.trim().is_empty() {
            return Err(format!("trailing `{}` after the string", tail.trim()));
        }
        return Ok(bytes);
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    if words.is_empty() {
        return Err("expected bytes or a command".to_string());
    }
    parse_hex(&words)
}

/// Drop a trailing `#` comment, unless it is inside the quoted string.
fn strip_comment(rest: &str) -> &str {
    if rest.starts_with('"') {
        let mut escaped = false;
        for (i, c) in rest.char_indices().skip(1) {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => return &rest[..=i],
                _ => {}
            }
        }
        return rest;
    }
    match rest.split_once('#') {
        Some((head, _)) => head.trim_end(),
        None => rest,
    }
}

/// `\n \r \t \0 \\ \" \xNN` in a double-quoted script string.
pub fn unescape(body: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        match chars.next() {
            Some('n') => out.push(b'\n'),
            Some('r') => out.push(b'\r'),
            Some('t') => out.push(b'\t'),
            Some('0') => out.push(0),
            Some('\\') => out.push(b'\\'),
            Some('"') => out.push(b'"'),
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                out.push(
                    u8::from_str_radix(&hex, 16)
                        .map_err(|e| format!("`\\x{hex}` is not a hex byte: {e}"))?,
                );
            }
            Some(other) => return Err(format!("unknown escape `\\{other}`")),
            None => return Err("trailing backslash".to_string()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::ByteSource;

    /// The truth table, as a table. Two rows are L0's measurement and two are
    /// esptool's circuit; the code does not know the difference and the
    /// README says which is which.
    #[test]
    fn the_auto_reset_circuit_is_the_boards_two_transistors() {
        let cases = [
            //  dtr,   rts,    en,   io0
            (false, false, true, true),
            (false, true, false, true),
            (true, false, true, false),
            (true, true, true, true),
        ];
        for (dtr, rts, en, io0) in cases {
            let cable = Cable { dtr, rts };
            assert_eq!(cable.en(), en, "EN for dtr={dtr} rts={rts}");
            assert_eq!(cable.io0(), io0, "IO0 for dtr={dtr} rts={rts}");
        }
        // Neither line alone can hold both strap lines — the point of the
        // circuit, and what makes `dtr 1` on its own a download strap rather
        // than a reset.
        assert_eq!(Cable { dtr: false, rts: true }.strap(), Strap::App);
        assert_eq!(Cable { dtr: true, rts: false }.strap(), Strap::Download);
        assert_eq!(Cable { dtr: true, rts: true }.strap(), Strap::App);
    }

    #[test]
    fn every_verb_in_the_protocol_table_parses_to_its_command() {
        let cases: &[(&str, ControlCommand)] = &[
            ("attach", ControlCommand::Attach),
            ("detach", ControlCommand::Detach),
            ("open", ControlCommand::Open),
            ("close", ControlCommand::Close),
            ("reset", ControlCommand::Reset),
            ("download-mode", ControlCommand::DownloadMode),
            ("state", ControlCommand::State),
            (
                "dtr 1",
                ControlCommand::Signals {
                    dtr: Some(true),
                    rts: None,
                },
            ),
            (
                "rts 0",
                ControlCommand::Signals {
                    dtr: None,
                    rts: Some(false),
                },
            ),
            (
                "signals dtr=0 rts=1",
                ControlCommand::Signals {
                    dtr: Some(false),
                    rts: Some(true),
                },
            ),
            ("wait 250", ControlCommand::Wait(250)),
        ];
        for (line, want) in cases {
            assert_eq!(&ControlCommand::parse(line).unwrap(), want, "`{line}`");
        }
        assert_eq!(
            ControlCommand::parse("  open  ").unwrap(),
            ControlCommand::Open
        );
        assert_eq!(ControlCommand::parse("dtr 1").unwrap().verb(), "dtr");
        assert_eq!(ControlCommand::parse("rts 1").unwrap().verb(), "rts");
        assert_eq!(
            ControlCommand::parse("signals dtr=1 rts=1").unwrap().verb(),
            "signals"
        );
    }

    /// The C6's `usb-write`, `pin` and `pins` are **not** this chip's verbs,
    /// and the error says so rather than half-accepting one.
    #[test]
    fn the_verbs_this_chip_does_not_have_are_refused_by_name() {
        for line in ["usb-write 4d 21", "pin 20 1", "pins"] {
            let err = ControlCommand::parse(line).unwrap_err();
            assert!(err.contains("unknown command"), "`{line}` gave {err}");
            assert!(err.contains("download-mode"), "and lists what does exist");
        }
    }

    #[test]
    fn a_bad_command_says_what_was_wrong_with_it_and_applies_nothing() {
        let bad = [
            "nope",
            "open now",
            "dtr",
            "dtr 2",
            "dtr high",
            "signals",
            "signals dtr",
            "signals dsr=1",
            "wait",
            "wait soon",
        ];
        for line in bad {
            let err = ControlCommand::parse(line).unwrap_err();
            assert!(!err.is_empty(), "`{line}` must say why");
        }
    }

    #[test]
    fn a_reply_names_the_command_and_the_guest_cycle_it_took_effect_at() {
        assert_eq!(
            ControlReply::Ok {
                verb: "reset",
                cycle: 12_345,
            }
            .to_string(),
            "ok reset cyc=12345 us=51"
        );
        assert_eq!(
            ControlReply::State {
                cycle: 240_000,
                report: CableReport {
                    attached: true,
                    port_open: true,
                    cable: Cable {
                        dtr: false,
                        rts: true
                    },
                    reboots: 2,
                },
            }
            .to_string(),
            "ok state cyc=240000 us=1000 cable=attached port=open dtr=0 rts=1 en=0 io0=1 \
             strap=app reboots=2"
        );
        assert_eq!(
            ControlReply::Err("reset: no cable attached".to_string()).to_string(),
            "err reset: no cable attached"
        );
    }

    #[test]
    fn the_cable_script_carries_commands_and_the_byte_script_carries_bytes() {
        let script = parse_control_script(
            "# the acceptance sequence\n\
             0    attach\n\
             0    open\n\
             100  signals dtr=0 rts=1   # EN low: the board is held in reset\n\
             150  signals dtr=0 rts=0   # released, IO0 high: it boots the app\n",
        )
        .unwrap();
        assert_eq!(
            script,
            vec![
                (0, ControlCommand::Attach),
                (0, ControlCommand::Open),
                (
                    100 * MS,
                    ControlCommand::Signals {
                        dtr: Some(false),
                        rts: Some(true)
                    }
                ),
                (
                    150 * MS,
                    ControlCommand::Signals {
                        dtr: Some(false),
                        rts: Some(false)
                    }
                ),
            ]
        );

        let mut bytes = parse_byte_script("1500 \"M!{}\\n\"\nthen +2ms 4d 21 0a\n").unwrap();
        assert_eq!(bytes.remaining(), 8);
        assert_eq!(bytes.next_ready(), Some(1_500 * MS));
        assert_eq!(bytes.next_byte(1_500 * MS), Some(b'M'));
    }

    #[test]
    fn wait_shifts_every_later_line_and_emits_nothing_itself() {
        let script =
            parse_control_script("0 attach\n0 wait 500\n0 open\n100 reset\n").unwrap();
        assert_eq!(
            script,
            vec![
                (0, ControlCommand::Attach),
                (500 * MS, ControlCommand::Open),
                (600 * MS, ControlCommand::Reset),
            ],
            "`wait` is relative time in a file whose leading numbers are absolute"
        );
    }

    /// The two sockets are two different things and mixing them is an error
    /// that names the other flag.
    #[test]
    fn each_script_refuses_the_other_ones_lines() {
        let err = parse_byte_script("0 reset\n").unwrap_err();
        assert!(err.starts_with("line 1:"), "{err}");
        assert!(err.contains("--control-script"), "{err}");

        let err = parse_control_script("0 \"hello\"\n").unwrap_err();
        assert!(err.starts_with("line 1:"), "{err}");
        assert!(err.contains("--uart0-script"), "{err}");
    }

    #[test]
    fn a_script_line_that_cannot_be_read_names_its_line_number() {
        for text in ["attach\n", "abc open\n", "10 dtr 2\n"] {
            let err = parse_control_script(text).unwrap_err();
            assert!(err.starts_with("line 1:"), "{text:?} gave {err:?}");
        }
        for text in ["10 \"unterminated\n", "10 zz\n", "10 \"\\q\"\n"] {
            let err = parse_byte_script(text).unwrap_err();
            assert!(err.starts_with("line 1:"), "{text:?} gave {err:?}");
        }
    }

    #[test]
    fn a_comment_after_a_value_is_a_comment_and_a_hash_inside_a_string_is_not() {
        let mut bytes = parse_byte_script("10 \"a#b\" # trailing\n").unwrap();
        assert_eq!(bytes.remaining(), 3);
        assert_eq!(bytes.next_byte(10 * MS), Some(b'a'));
        assert_eq!(bytes.next_byte(10 * MS), Some(b'#'));
        assert_eq!(bytes.next_byte(10 * MS), Some(b'b'));
    }

    #[test]
    fn escapes_are_the_c6s_escapes() {
        assert_eq!(unescape("a\\nb\\x00c\\\\\\\"").unwrap(), b"a\nb\0c\\\"");
        assert!(unescape("\\q").is_err());
        assert!(unescape("trailing\\").is_err());
    }
}
