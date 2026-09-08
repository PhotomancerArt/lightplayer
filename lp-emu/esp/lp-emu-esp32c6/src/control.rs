//! The host control channel (plan PD8) — the line protocol, its replies,
//! and the scripted form.
//!
//! A byte socket carries bytes and nothing else, because `lp-cli …
//! serial:tcp://` is a plain byte client and in-band control would be a
//! dialect every client would have to speak. So the host's *side* of the
//! USB link — the cable, the port, the DTR/RTS lines, the reset dances —
//! rides a **second** socket in its own line protocol, and this module is
//! that protocol: nothing here knows about sockets, the machine, or the
//! `USB_DEVICE` block. It parses strings and formats strings, which is why
//! all of it is unit-tested on strings.
//!
//! # The vocabulary is the scripted fake device's, deliberately
//!
//! `lpa-link/src/providers/fake_device/fake_device_core.rs` is the other
//! device a host program can talk to, and it already decides what a host
//! *did* from `set_signals(dtr, rts)` and `reopen(baud)`. The verbs here map
//! onto it one for one, so plan two's Web Serial shim is glue rather than a
//! translation:
//!
//! | Web Serial / esptool-js | here | the fake |
//! |---|---|---|
//! | `requestPort()` + `open()` | `attach` then `open` | a device appears, then `reopen` |
//! | `close()` | `close` | the stream is dropped |
//! | unplug | `detach` | the device goes away (scenario s7) |
//! | `setSignals({dataTerminalReady, requestToSend})` | `dtr 0\|1`, `rts 0\|1`, `signals dtr=… rts=…` | `set_signals` |
//! | esptool-js's reset dance | the same `dtr`/`rts` lines | the RTS falling edge decodes it |
//!
//! # Not the wire
//!
//! No line here is an `M!` frame and none ever will be: this socket is the
//! cable, not the protocol on it. Any `M!` line a host-side script sends is
//! built by `lpc_wire::json::to_serial_line` in lp-cli or the runner (plan
//! conventions, PR #538) and reaches the device as **bytes**, on the byte
//! socket or through a script — never assembled inside `lp-emu/*`, which
//! cannot depend on `lpc-wire` at all (`just lint-emu-fence`).
//!
//! # Time
//!
//! A command carries no time of its own. It is applied at the next slice
//! boundary, and the reply says at which guest cycle that was — so a socket
//! run's *ordering* is the host's while everything the machine then does is
//! still guest time. The scripted form ([`parse_usb_script`]) has the times
//! in the file and is the deterministic path.

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::ScriptedSource;

use crate::memmap;

/// One command on the control channel.
///
/// [`Wait`](ControlCommand::Wait) is the scripted form's own — a socket
/// client waits by waiting — and the parser consumes it rather than handing
/// it on. Host **bytes** never appear here either: a script's byte lines
/// become [`UsbScript::bytes`], and a socket client writes them to the byte
/// socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlCommand {
    /// The cable goes in: bus reset, SOF starts, the port is closed.
    Attach,
    /// The cable comes out: SOF stops.
    Detach,
    /// An application opened the port: the IN endpoint drains.
    Open,
    /// It closed the port: committed packets sit where they are.
    Close,
    /// One or both control lines, as the host last set them. `dtr`/`rts` on
    /// their own set one; `signals dtr=… rts=…` sets both in one write,
    /// which is what a `UartBridge`-style host does.
    Signals {
        dtr: Option<bool>,
        rts: Option<bool>,
    },
    /// The hard-reset dance's effect, without the dance.
    Reset,
    /// The download dance's effect, without the dance.
    DownloadMode,
    /// Report the host's side. Answered by [`ControlReply::State`].
    State,
    /// Host → device bytes with no byte socket in the picture (tests).
    UsbWrite(Vec<u8>),
    /// Scripted form only: shift every later line by this many milliseconds.
    Wait(u64),
    /// Hold a level on a pad from outside the chip — a bench driver, the
    /// **auditable** twin of a `--pin-script` line (plan RD6). Applied at a
    /// slice boundary and stamped with the guest cycle, like every verb
    /// here; a run that used it is not a transcript.
    Pin { pad: u8, level: bool },
    /// Report every pad the machine has anything to say about. Answered by
    /// [`ControlReply::Pins`].
    Pins,
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
            ControlCommand::UsbWrite(_) => "usb-write",
            ControlCommand::Wait(_) => "wait",
            ControlCommand::Pin { .. } => "pin",
            ControlCommand::Pins => "pins",
        }
    }

    /// Every verb the parser knows, in the order the protocol document
    /// lists them. Also what tells a script line's control word from a run
    /// of hex bytes.
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
        "usb-write",
        "wait",
        "pin",
        "pins",
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
            "pins" => no_args(ControlCommand::Pins),
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
            "usb-write" => {
                if rest.is_empty() {
                    return Err("`usb-write` needs at least one hex byte".to_string());
                }
                Ok(ControlCommand::UsbWrite(parse_hex(&rest)?))
            }
            "pin" => {
                let [pad, value] = rest[..] else {
                    return Err("`pin` takes a pad number and a level, 0 or 1".to_string());
                };
                let n: u8 = pad
                    .trim_start_matches("gpio")
                    .parse()
                    .map_err(|e| format!("`pin {pad}`: not a pad number: {e}"))?;
                // The same policy a `--pin-script` line is held to: the
                // socket is auditable, not permissive.
                crate::pinscript::check_pad(n, crate::pinscript::PadUse::Driven)?;
                Ok(ControlCommand::Pin {
                    pad: n,
                    level: parse_level("pin", value)?,
                })
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

/// One pad, as the `pins` command reports it.
///
/// Both sides of the fabric in one row: what the chip has routed to the pad,
/// whether its input buffer is on, what a bench driver is holding on it, and
/// the level the two of them resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadReport {
    pub pad: u8,
    /// What the pad's level follows, as the chip's routing view names it
    /// (`sig71`, `gpio-out`), or `None` when nothing has routed it.
    pub route: Option<String>,
    /// The pad's input enable (the chip's IO_MUX `fun_ie`).
    pub input_enable: bool,
    /// The level an outside driver is holding, if one is.
    pub driven: Option<bool>,
    /// The resolved level — what a scope on the pin header would read.
    pub level: bool,
    /// Every pad tied to this one by `--wire`, ascending, this one left out.
    pub wired_to: Vec<u8>,
}

impl std::fmt::Display for PadReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "gpio{}[route={} ie={} drv={} lvl={}",
            self.pad,
            self.route.as_deref().unwrap_or("-"),
            u8::from(self.input_enable),
            match self.driven {
                Some(level) => (u8::from(level) + b'0') as char,
                None => '-',
            },
            u8::from(self.level),
        )?;
        if !self.wired_to.is_empty() {
            let wired: Vec<String> = self.wired_to.iter().map(|p| format!("gpio{p}")).collect();
            write!(f, " wire={}", wired.join(","))?;
        }
        f.write_str("]")
    }
}

/// The host's side, as the `state` command reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostReport {
    pub attached: bool,
    pub draining: bool,
    pub sof: bool,
    /// Bytes sitting in the IN endpoint that no host has taken.
    pub in_pending: usize,
    /// Host bytes taken from the socket or a script that the guest has not
    /// read: the resident OUT packet plus what waits behind it.
    pub out_queued: usize,
}

/// One reply line. Exactly one per command — that is the whole contract,
/// and a client may read it as "the line that starts with `ok` or `err`".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlReply {
    /// `ok <verb> cyc=<cycle> us=<micros>` — applied, at that guest cycle.
    Ok { verb: &'static str, cycle: Cycles },
    /// `ok state cyc=… us=… host=… draining=… sof=… in_pending=… out_queued=…`
    State { cycle: Cycles, host: HostReport },
    /// `ok pins cyc=… us=… pads=<n> gpio18[…] gpio20[…]` — one line, one
    /// bracket per pad, so it can be selected and pasted whole.
    Pins { cycle: Cycles, pads: Vec<PadReport> },
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
            ControlReply::State { cycle, host } => write!(
                f,
                "ok state cyc={cycle} us={} host={} draining={} sof={} in_pending={} \
                 out_queued={}",
                cycle / memmap::CYCLES_PER_US,
                if host.attached { "attached" } else { "absent" },
                host.draining,
                if host.sof { "on" } else { "off" },
                host.in_pending,
                host.out_queued,
            ),
            ControlReply::Pins { cycle, pads } => {
                write!(
                    f,
                    "ok pins cyc={cycle} us={} pads={}",
                    cycle / memmap::CYCLES_PER_US,
                    pads.len()
                )?;
                for pad in pads {
                    write!(f, " {pad}")?;
                }
                Ok(())
            }
            ControlReply::Err(reason) => write!(f, "err {reason}"),
        }
    }
}

/// A parsed `--usb-script`: the host's bytes on one side, its control
/// commands on the other, both in file order.
#[derive(Debug, Default)]
pub struct UsbScript {
    /// Host → device bytes, as the OUT path's source.
    pub bytes: ScriptedSource,
    /// `(guest cycle, command)`, in file order.
    pub commands: Vec<(Cycles, ControlCommand)>,
}

/// One line of the script grammar, parsed but not yet placed on a timeline.
///
/// The three byte forms are [`ScriptedSource`]'s own three steps; the fourth
/// is the control channel's, and only [`parse_usb_script`] accepts it.
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
    // A comment may follow a command or a quoted string on the same line.
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

/// The byte half of the grammar on its own: `--uart0-script`.
///
/// A link with no control channel has no cable to talk about, so a control
/// word here is an error naming the flag that does take one rather than a
/// line silently ignored.
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
                    "`{}` is a control command and this link has no control channel \
                     (--usb-script takes those)",
                    command.verb()
                )));
            }
        }
    }
    Ok(source)
}

/// `--usb-script`: `--uart0-script`'s grammar with the control words added.
///
/// One entry per line. A line is either **bytes** — a double-quoted string
/// with `\n \r \t \\ \" \0 \xNN` escapes, or whitespace-separated hex bytes —
/// or one of the control words. `#` starts a comment; blank lines are
/// skipped. File order is wire order.
///
/// A leading `<ms>` is absolute emulated time from cycle zero. A `wait <ms>`
/// line adds to an offset applied to every **later** line, which is how a
/// relative script is written without recomputing every timestamp after an
/// edit.
///
/// `after "<line>" [+<ms>] <bytes>` and `then +<ms> <bytes>` are the walk
/// forms, and they are what makes the upload walk runnable over this link at
/// all: a client sends its next request when the answer to the last one
/// arrives, not at a wall-clock offset. The needle is matched against the
/// device's **USB** output, so the same file replays on either link — the
/// conversation is the payload's, the link is the configuration's (M6 P5).
/// The wait resolves in guest cycles, so two runs deliver the same bytes at
/// the same cycles.
///
/// A `wait` offset applies to the absolute lines only; a wait-for step has
/// no absolute time to shift.
pub fn parse_usb_script(text: &str) -> Result<UsbScript, String> {
    let mut script = UsbScript::default();
    let mut offset_ms = 0u64;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |e: String| format!("line {}: {e}", n + 1);
        match parse_script_line(line).map_err(&at)? {
            ScriptLine::At { ms, bytes } => script.bytes.push((ms + offset_ms) * MS, bytes),
            ScriptLine::After {
                needle,
                delay_ms,
                bytes,
            } => script.bytes.push_after(needle, delay_ms * MS, bytes),
            ScriptLine::Then { delay_ms, bytes } => script.bytes.push_then(delay_ms * MS, bytes),
            ScriptLine::Command {
                command: ControlCommand::Wait(by),
                ..
            } => offset_ms += by,
            ScriptLine::Command { ms, command } => {
                script.commands.push(((ms + offset_ms) * MS, command));
            }
        }
    }
    Ok(script)
}

/// One emulated millisecond, in cycles.
const MS: Cycles = 1_000 * memmap::CYCLES_PER_US;

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
    // The closing quote is the first unescaped one.
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
        // The string runs to its closing quote; anything after it is a
        // comment (the grammar has one value per line).
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

    const MS: Cycles = 1_000 * memmap::CYCLES_PER_US;

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
            (
                "usb-write 4d 21 0a",
                ControlCommand::UsbWrite(vec![0x4d, 0x21, 0x0a]),
            ),
            ("wait 250", ControlCommand::Wait(250)),
            (
                "pin 20 1",
                ControlCommand::Pin {
                    pad: 20,
                    level: true,
                },
            ),
            (
                "pin gpio21 0",
                ControlCommand::Pin {
                    pad: 21,
                    level: false,
                },
            ),
            ("pins", ControlCommand::Pins),
        ];
        for (line, want) in cases {
            assert_eq!(&ControlCommand::parse(line).unwrap(), want, "`{line}`");
        }
        // Leading and trailing space is a client's, not a protocol error.
        assert_eq!(
            ControlCommand::parse("  open  ").unwrap(),
            ControlCommand::Open
        );
        // The verb the reply echoes is the one the client typed.
        assert_eq!(ControlCommand::parse("dtr 1").unwrap().verb(), "dtr");
        assert_eq!(ControlCommand::parse("rts 1").unwrap().verb(), "rts");
        assert_eq!(
            ControlCommand::parse("signals dtr=1 rts=1").unwrap().verb(),
            "signals"
        );
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
            "usb-write",
            "usb-write zz",
            "wait",
            "wait soon",
            "pin",
            "pin 20",
            "pin 20 2",
            "pin twenty 1",
            "pins now",
            // The pad policy is the script's, on the socket too.
            "pin 9 1",
            "pin 18 0",
            "pin 31 1",
        ];
        for line in bad {
            let err = ControlCommand::parse(line).unwrap_err();
            assert!(!err.is_empty(), "`{line}` must say why");
        }
        assert!(
            ControlCommand::parse("nope")
                .unwrap_err()
                .contains("attach"),
            "an unknown command lists the ones that exist"
        );
    }

    #[test]
    fn a_reply_names_the_command_and_the_guest_cycle_it_took_effect_at() {
        assert_eq!(
            ControlReply::Ok {
                verb: "attach",
                cycle: 12_345,
            }
            .to_string(),
            "ok attach cyc=12345 us=77"
        );
        assert_eq!(
            ControlReply::State {
                cycle: 160_000,
                host: HostReport {
                    attached: true,
                    draining: true,
                    sof: true,
                    in_pending: 64,
                    out_queued: 0,
                },
            }
            .to_string(),
            "ok state cyc=160000 us=1000 host=attached draining=true sof=on in_pending=64 \
             out_queued=0"
        );
        assert_eq!(
            ControlReply::State {
                cycle: 0,
                host: HostReport {
                    attached: false,
                    draining: false,
                    sof: false,
                    in_pending: 0,
                    out_queued: 7,
                },
            }
            .to_string(),
            "ok state cyc=0 us=0 host=absent draining=false sof=off in_pending=0 out_queued=7"
        );
        assert_eq!(
            ControlReply::Err("open: no host attached".to_string()).to_string(),
            "err open: no host attached"
        );
    }

    /// **G1-5.** `pin` and `pins` round-trip: they parse, they render, and
    /// the reply names the cycle the machine applied them at.
    #[test]
    fn the_pin_verbs_render_both_sides_of_every_pad_on_one_line() {
        assert_eq!(
            ControlReply::Ok {
                verb: "pin",
                cycle: 320,
            }
            .to_string(),
            "ok pin cyc=320 us=2"
        );
        assert_eq!(
            ControlReply::Pins {
                cycle: 160_000,
                pads: vec![
                    PadReport {
                        pad: 18,
                        route: Some("sig71".to_string()),
                        input_enable: false,
                        driven: None,
                        level: true,
                        wired_to: vec![19],
                    },
                    PadReport {
                        pad: 19,
                        route: None,
                        input_enable: true,
                        driven: None,
                        level: true,
                        wired_to: vec![18],
                    },
                    PadReport {
                        pad: 20,
                        route: Some("gpio-out".to_string()),
                        input_enable: true,
                        driven: Some(false),
                        level: false,
                        wired_to: vec![],
                    },
                ],
            }
            .to_string(),
            "ok pins cyc=160000 us=1000 pads=3 \
             gpio18[route=sig71 ie=0 drv=- lvl=1 wire=gpio19] \
             gpio19[route=- ie=1 drv=- lvl=1 wire=gpio18] \
             gpio20[route=gpio-out ie=1 drv=0 lvl=0]"
        );
        assert_eq!(
            ControlReply::Pins {
                cycle: 0,
                pads: vec![],
            }
            .to_string(),
            "ok pins cyc=0 us=0 pads=0"
        );
    }

    /// A pin verb is the socket's, not the script's: it carries no time of
    /// its own, so a script file that used one is a script that cannot be
    /// replayed. `--pin-script` is the deterministic path and its grammar is
    /// `crate::pinscript`'s.
    #[test]
    fn a_pin_verb_in_a_usb_script_is_a_command_like_any_other() {
        let script = parse_usb_script("0 attach\n100 pin 20 1\n").unwrap();
        assert_eq!(
            script.commands,
            vec![
                (0, ControlCommand::Attach),
                (
                    100 * MS,
                    ControlCommand::Pin {
                        pad: 20,
                        level: true
                    }
                ),
            ]
        );
    }

    #[test]
    fn a_script_carries_bytes_on_one_side_and_commands_on_the_other_in_file_order() {
        // The s7 shape from the brief, with an `M!` line built elsewhere and
        // pasted in — the emulator parses bytes, it never builds a frame.
        let mut script = parse_usb_script(
            "# the s7 shape\n\
             0      attach\n\
             0      open\n\
             1500   \"M!{\\\"id\\\":1,\\\"msg\\\":\\\"hello\\\"}\\n\"   # from to_serial_line\n\
             6000   detach\n\
             9000   attach\n\
             9500   open\n",
        )
        .unwrap();
        assert_eq!(
            script.commands,
            vec![
                (0, ControlCommand::Attach),
                (0, ControlCommand::Open),
                (6_000 * MS, ControlCommand::Detach),
                (9_000 * MS, ControlCommand::Attach),
                (9_500 * MS, ControlCommand::Open),
            ]
        );
        assert_eq!(script.bytes.next_ready(), Some(1_500 * MS));
        // `M!` + the 22-byte object + the newline: bytes, not a frame this
        // crate could have built.
        assert_eq!(script.bytes.remaining(), 25);
        assert_eq!(script.bytes.next_byte(1_500 * MS), Some(b'M'));
    }

    #[test]
    fn hex_bytes_and_control_words_are_told_apart_by_the_first_token() {
        let mut script = parse_usb_script("10 4d 21 0a\n20 open\n30 0xde 0xad\n").unwrap();
        assert_eq!(script.commands, vec![(20 * MS, ControlCommand::Open)]);
        assert_eq!(script.bytes.remaining(), 5);
        assert_eq!(script.bytes.next_byte(10 * MS), Some(0x4d));
    }

    #[test]
    fn wait_shifts_every_later_line_and_emits_nothing_itself() {
        let script = parse_usb_script("0 attach\n0 wait 500\n0 open\n100 detach\n").unwrap();
        assert_eq!(
            script.commands,
            vec![
                (0, ControlCommand::Attach),
                (500 * MS, ControlCommand::Open),
                (600 * MS, ControlCommand::Detach),
            ],
            "`wait` is relative time in a file whose leading numbers are absolute"
        );
    }

    #[test]
    fn a_script_line_that_cannot_be_read_names_its_line_number() {
        for text in [
            "attach\n",
            "abc open\n",
            "10 \"unterminated\n",
            "10 zz\n",
            "10 \"\\q\"\n",
            "10 dtr 2\n",
        ] {
            let err = parse_usb_script(text).unwrap_err();
            assert!(err.starts_with("line 1:"), "{text:?} gave {err:?}");
        }
    }

    #[test]
    fn a_comment_after_a_value_is_a_comment_and_a_hash_inside_a_string_is_not() {
        let mut script = parse_usb_script("10 \"a#b\" # trailing\n20 open # why\n").unwrap();
        assert_eq!(script.commands, vec![(20 * MS, ControlCommand::Open)]);
        assert_eq!(script.bytes.remaining(), 3);
        assert_eq!(script.bytes.next_byte(10 * MS), Some(b'a'));
        assert_eq!(script.bytes.next_byte(10 * MS), Some(b'#'));
        assert_eq!(script.bytes.next_byte(10 * MS), Some(b'b'));
    }

    /// The walk forms are the whole point of P5: M4's `examples-basic.script`
    /// is a file of `after` and `then` lines, and until this parser took them
    /// the shipped walk could only be replayed over the spike's UART0 link.
    #[test]
    fn the_walk_forms_parse_on_the_usb_link_too() {
        let mut script = parse_usb_script(
            "0 attach\n\
             after \"[RECOVERY] boot complete (first frame served)\" \"M!{\\\"id\\\":1}\\n\"\n\
             then +2ms \"more\"\n\
             after \"\\\"id\\\":1,\" +5ms 4d 21 0a\n",
        )
        .unwrap();
        assert_eq!(script.commands, vec![(0, ControlCommand::Attach)]);
        // 11 + 4 + 3 bytes, in three steps.
        assert_eq!(script.bytes.remaining(), 18);
        assert_eq!(script.bytes.steps_left(), 3);
        // Nothing is ready: every step waits on something the device has not
        // said, and a source with no log to watch never resolves one.
        assert_eq!(script.bytes.next_byte(1_000 * MS), None);
    }

    /// A `wait` shifts the absolute lines. A wait-*for* line has no absolute
    /// time to shift, which is a distinction worth pinning: an offset that
    /// silently moved an `after` would be moving a wait that is already
    /// relative to the device.
    #[test]
    fn a_wait_offsets_the_absolute_lines_and_leaves_the_waits_alone() {
        let script =
            parse_usb_script("0 wait 500\n10 open\nafter \"x\" \"y\"\n600 detach\n").unwrap();
        assert_eq!(
            script.commands,
            vec![
                (510 * MS, ControlCommand::Open),
                (1_100 * MS, ControlCommand::Detach),
            ]
        );
        assert_eq!(script.bytes.steps_left(), 1);
    }

    /// UART0 has no cable to talk about, so a control word there is an error
    /// naming the flag that does take one — never a line quietly dropped.
    #[test]
    fn the_byte_only_grammar_refuses_a_control_word() {
        let source = parse_byte_script("1500 \"hi\"\nthen +2ms \"there\"\n").unwrap();
        assert_eq!(source.remaining(), 7);
        let err = parse_byte_script("0 attach\n").unwrap_err();
        assert!(err.starts_with("line 1:"), "{err}");
        assert!(err.contains("--usb-script"), "{err}");
    }

    #[test]
    fn escapes_are_the_uart0_scripts_escapes() {
        assert_eq!(unescape("a\\nb\\x00c\\\\\\\"").unwrap(), b"a\nb\0c\\\"");
        assert!(unescape("\\q").is_err());
        assert!(unescape("trailing\\").is_err());
    }
}
