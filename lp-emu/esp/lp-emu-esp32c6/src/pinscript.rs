//! `--pin-script` — scripted host **input** on the pads, and `--wire`'s pad
//! policy.
//!
//! The determinism split this machine already lives by (README
//! "Determinism", plan RD6) applies to input exactly as it does to bytes:
//!
//! - **scripted** is the deterministic path. Every line carries its own
//!   guest time, so two runs of the same file against the same image drive
//!   the same pads at the same cycles. This module.
//! - **socket** is the auditable path. The `pin` / `pins` verbs on the
//!   control channel ([`crate::control`]) are applied at a slice boundary
//!   and stamped with the cycle they landed at, and a run that used them is
//!   not a transcript.
//!
//! The two never blur. A `--pin-script` run needs no socket and a socket
//! run makes no claim about repeating.
//!
//! # The grammar
//!
//! It is [`crate::control::parse_usb_script`]'s, with `pin <n> <0|1>` where
//! the bytes go — the same `after` / `then` forms, the same `#` comments,
//! the same file-order-is-wire-order rule — plus two **generators**, because
//! a payload should not have to hand-write forty lines of contact bounce.
//!
//! ```text
//! # a level at an absolute guest time
//! <us> pin <n> <0|1>
//! # once the device has said something
//! after "<needle>" [+<ms>] pin <n> <0|1>
//! # paced after the previous step
//! then +<ms> pin <n> <0|1>
//! # generators
//! button <n> press at <us> [bounce <k> edges over <us>] hold <ms>
//! encoder <a> <b> <steps> cw|ccw from <us> at <hz>
//! ```
//!
//! **The units differ from `--usb-script` on purpose, and only in the
//! absolute column.** A host byte lands on a millisecond scale; a contact
//! bounce is tens of microseconds, and a script that had to spell one as
//! `0.05` would not be an integer grammar at all. So the leading number of a
//! `--pin-script` line is **microseconds** (plan RD6's `<us> pin <n>
//! <0|1>`), while `after`/`then`'s `+<ms>` delay stays milliseconds, as it
//! is in `--usb-script`. Both accept an explicit `us` or `ms` suffix, and
//! spelling the unit is the readable thing to do: `1500us`, `+2ms`.
//!
//! # The pads a script may not name
//!
//! Four of the C6's pads are spoken for on this desk, and a script that
//! drove one would be describing a board that does not exist:
//!
//! | pad | why |
//! |---|---|
//! | GPIO9 | the BOOT strap — it decides what the chip boots as, and it is Yona's hands |
//! | GPIO12 / GPIO13 | USB D− / D+ |
//! | GPIO16 / GPIO17 | the UART0 tap the bridge board uses |
//! | GPIO18 | the LED strip |
//!
//! [`check_pad`] refuses each by name with the reason. `--wire` has one
//! exception and only one: **GPIO18 as the TX side of a loopback**
//! (`--wire 18:19`), which is how the chase reaches an RX pad without a
//! jumper and which M2 P3's RMT RX engine needs.

use std::collections::VecDeque;

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::ByteLog;
use lp_emu_esp_common::pins::PadId;

use crate::control::unescape;
use crate::memmap::CYCLES_PER_US;
use crate::periph::gpio::PAD_COUNT;

/// One emulated microsecond, in cycles.
const US: Cycles = CYCLES_PER_US;
/// One emulated millisecond, in cycles.
const MS: Cycles = 1_000 * CYCLES_PER_US;

/// A pad a script and a wire may not name, and why.
///
/// The list is `../notes.md` F17's, and it is a real check with a message
/// rather than a comment: the emulator is where a wiring mistake should be
/// caught, since on the desk it costs a board.
pub const FORBIDDEN_PADS: &[(u8, &str)] = &[
    (9, "the BOOT strap — it decides what the chip boots as"),
    (12, "USB D−"),
    (13, "USB D+"),
    (16, "the UART0 tap (RX)"),
    (17, "the UART0 tap (TX)"),
    (18, "the LED strip"),
];

/// What a pad is about to be used for. The only difference is GPIO18.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadUse {
    /// A `--pin-script` line or a `pin` verb drives it from outside.
    Driven,
    /// The **TX** side of a `--wire a:b` — the pad whose output is being
    /// carried somewhere. GPIO18 is allowed here and nowhere else: the strip
    /// pad feeding a loopback into an RX pad is the one shape this plan
    /// wants (M2 P3, `--wire 18:19`).
    LoopbackTx,
    /// The other side of a `--wire a:b`.
    LoopbackRx,
}

/// Refuse a pad that is spoken for, by name and with the reason.
pub fn check_pad(pad: u8, use_: PadUse) -> Result<(), String> {
    if u32::from(pad) >= PAD_COUNT {
        return Err(format!(
            "gpio{pad}: the ESP32-C6 has {PAD_COUNT} pads, gpio0 to gpio{}",
            PAD_COUNT - 1
        ));
    }
    if pad == 18 && use_ == PadUse::LoopbackTx {
        return Ok(());
    }
    for (forbidden, why) in FORBIDDEN_PADS {
        if *forbidden == pad {
            return Err(format!(
                "gpio{pad} is {why}; this run may not drive it{}",
                if *forbidden == 18 {
                    " (a `--wire 18:<n>` loopback is the one exception, \
                     with gpio18 on the left)"
                } else {
                    ""
                }
            ));
        }
    }
    Ok(())
}

/// A level on a pad, as the script asked for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinEvent {
    pub pad: PadId,
    pub level: bool,
}

/// One step of a [`PinScript`], the shape [`lp_emu_esp_common::ScriptedSource`]
/// already uses for bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PinStep {
    /// Drive at an absolute guest cycle.
    At { at: Cycles, events: Vec<PinEvent> },
    /// Drive once `needle` has appeared in the device's own output after the
    /// previous step, plus `delay` cycles. `resolved` latches the answer.
    After {
        needle: Vec<u8>,
        delay: Cycles,
        events: Vec<PinEvent>,
        resolved: Option<Cycles>,
    },
    /// Drive `delay` cycles after the previous step ran.
    Then {
        delay: Cycles,
        events: Vec<PinEvent>,
        resolved: Option<Cycles>,
    },
}

impl PinStep {
    fn events(&self) -> &[PinEvent] {
        match self {
            PinStep::At { events, .. }
            | PinStep::After { events, .. }
            | PinStep::Then { events, .. } => events,
        }
    }
}

/// A parsed `--pin-script`: pad levels at declared guest times.
///
/// The machine asks [`next_service`](Self::next_service) for the cycle it
/// must next look, bounds its slice by it, and calls
/// [`take_due`](Self::take_due) at the boundary. Nothing here reads a host
/// clock.
#[derive(Debug, Default)]
pub struct PinScript {
    steps: VecDeque<PinStep>,
    /// The device's consoles, when this script watches for lines in them.
    /// One anchor per console, the way the machine's own `--exit-on` search
    /// keeps one: a pin script is link-agnostic, so an `after` resolves off
    /// whichever console said it.
    watch: Vec<ByteLog>,
    /// How far into each console the current step has already searched.
    search_from: Vec<usize>,
}

impl PinScript {
    pub fn new() -> Self {
        Self::default()
    }

    /// Watch a console for `after`'s needles. A script with no console
    /// treats every `after` as unsatisfiable, and says so by never emptying
    /// rather than by pretending the wait passed.
    pub fn watching(mut self, log: ByteLog) -> Self {
        self.watch.push(log);
        self.search_from.push(0);
        self
    }

    /// Steps still to run.
    pub fn steps_left(&self) -> usize {
        self.steps.len()
    }

    /// Pad levels still to drive.
    pub fn remaining(&self) -> usize {
        self.steps.iter().map(|s| s.events().len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Append another script's steps after this one's — file order is wire
    /// order, and two files on one run are no different.
    pub fn extend(&mut self, other: Self) {
        self.steps.extend(other.steps);
    }

    /// Every pad this script ever drives, ascending — what a load report
    /// names and what the machine checks against the pad policy once, at
    /// build time, rather than line by line at run time.
    pub fn pads(&self) -> Vec<u8> {
        let mut pads: Vec<u8> = self
            .steps
            .iter()
            .flat_map(|s| s.events().iter().map(|e| e.pad.0))
            .collect();
        pads.sort_unstable();
        pads.dedup();
        pads
    }

    /// The cycle the machine must next call [`take_due`](Self::take_due) at,
    /// or `None` when the script is finished.
    ///
    /// A step still waiting on a needle has no time of its own, so the
    /// answer is the poll cadence — in **guest** cycles, so the resolve
    /// lands on the same grid in every run, exactly as
    /// [`lp_emu_esp_common::ScriptedSource`]'s waits do.
    pub fn next_service(&self, now: Cycles, poll: Cycles) -> Option<Cycles> {
        match self.steps.front()? {
            PinStep::At { at, .. } => Some(*at),
            PinStep::After {
                resolved: Some(at), ..
            }
            | PinStep::Then {
                resolved: Some(at), ..
            } => Some(*at),
            _ => Some(now.saturating_add(poll)),
        }
    }

    /// Every event due by `now`, in file order, each with **its own** cycle.
    ///
    /// The step's cycle, not the boundary's: a machine that stamped a
    /// scripted edge with the slice boundary it noticed it at would put the
    /// edge a cycle or two late in the pin log and make the file's own times
    /// unverifiable. An `after` or `then` step's cycle is the one its wait
    /// resolved to.
    pub fn take_due(&mut self, now: Cycles) -> Vec<(Cycles, PinEvent)> {
        let mut out = Vec::new();
        while let Some(at) = self.ready_at(now) {
            if at > now {
                break;
            }
            let step = self.steps.pop_front().expect("checked");
            out.extend(step.events().iter().map(|e| (at, *e)));
        }
        out
    }

    /// Resolve the front step's wait if the device has said its needle.
    fn ready_at(&mut self, now: Cycles) -> Option<Cycles> {
        let seen: Vec<Vec<u8>> = self.watch.iter().map(|log| log.bytes()).collect();
        let anchors = self.search_from.clone();
        let (needle, delay) = match self.steps.front_mut()? {
            PinStep::At { at, .. } => return Some(*at),
            PinStep::After {
                resolved: Some(at), ..
            }
            | PinStep::Then {
                resolved: Some(at), ..
            } => return Some(*at),
            PinStep::Then {
                delay, resolved, ..
            } => {
                let at = now.saturating_add(*delay);
                *resolved = Some(at);
                return Some(at);
            }
            PinStep::After { needle, delay, .. } => (needle.clone(), *delay),
        };
        if needle.is_empty() {
            return None;
        }
        // Whichever console said it first in this pass; the others' anchors
        // move to their current end, so a later needle is not matched
        // against output that was already on screen.
        let mut found = None;
        for (i, bytes) in seen.iter().enumerate() {
            let from = anchors[i].min(bytes.len());
            let tail = &bytes[from..];
            if tail.len() < needle.len() {
                continue;
            }
            if let Some(hit) = tail
                .windows(needle.len())
                .position(|w| w == needle.as_slice())
            {
                found = Some((i, from + hit + needle.len()));
                break;
            }
        }
        let (log, past) = found?;
        for (i, bytes) in seen.iter().enumerate() {
            self.search_from[i] = if i == log { past } else { bytes.len() };
        }
        let at = now.saturating_add(delay);
        if let Some(PinStep::After { resolved, .. }) = self.steps.front_mut() {
            *resolved = Some(at);
        }
        Some(at)
    }

    fn push_at(&mut self, at: Cycles, events: Vec<PinEvent>) {
        if !events.is_empty() {
            self.steps.push_back(PinStep::At { at, events });
        }
    }
}

/// Parse a `--pin-script` file. See the module docs for the grammar.
pub fn parse_pin_script(text: &str) -> Result<PinScript, String> {
    let mut script = PinScript::new();
    // Generators produce absolute steps out of file order (a `button` line's
    // rest level is at cycle 0), so absolute steps are collected and sorted
    // by time; `after`/`then` keep their place in the queue between them.
    for (n, raw) in text.lines().enumerate() {
        let line = strip_comment(raw.trim());
        if line.is_empty() {
            continue;
        }
        let at = |e: String| format!("line {}: {e}", n + 1);
        if let Some(rest) = line.strip_prefix("after ") {
            let (needle, rest) = take_quoted(rest.trim()).map_err(&at)?;
            let (delay_ms, rest) = parse_delay(rest.trim()).map_err(&at)?;
            let events = parse_pin_clause(rest.trim()).map_err(&at)?;
            script.steps.push_back(PinStep::After {
                needle,
                delay: delay_ms,
                events,
                resolved: None,
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("then ") {
            let (delay_ms, rest) = parse_delay(rest.trim()).map_err(&at)?;
            let events = parse_pin_clause(rest.trim()).map_err(&at)?;
            script.steps.push_back(PinStep::Then {
                delay: delay_ms,
                events,
                resolved: None,
            });
            continue;
        }
        if line.starts_with("button ") {
            for (cycle, event) in parse_button(line).map_err(&at)? {
                script.push_at(cycle, vec![event]);
            }
            continue;
        }
        if line.starts_with("encoder ") {
            for (cycle, event) in parse_encoder(line).map_err(&at)? {
                script.push_at(cycle, vec![event]);
            }
            continue;
        }
        let (stamp, rest) = line.split_once(char::is_whitespace).ok_or_else(|| {
            at("expected `<us> pin <n> <0|1>`, `after \"<line>\" pin …`, \
                `then +<ms> pin …`, a `button` or an `encoder`"
                .to_string())
        })?;
        let cycle = parse_stamp(stamp).map_err(&at)?;
        script.push_at(cycle, parse_pin_clause(rest.trim()).map_err(&at)?);
    }
    Ok(script)
}

/// `pin <n> <0|1>`.
fn parse_pin_clause(text: &str) -> Result<Vec<PinEvent>, String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let ["pin", pad, level] = words[..] else {
        return Err(format!(
            "expected `pin <n> <0|1>`, got `{text}` (a pin script carries \
             levels, never bytes — `--usb-script` takes those)"
        ));
    };
    Ok(vec![PinEvent {
        pad: parse_pad(pad, PadUse::Driven)?,
        level: parse_level(level)?,
    }])
}

fn parse_pad(text: &str, use_: PadUse) -> Result<PadId, String> {
    let n: u8 = text
        .trim_start_matches("gpio")
        .parse()
        .map_err(|e| format!("`{text}` is not a pad number: {e}"))?;
    check_pad(n, use_)?;
    Ok(PadId(n))
}

fn parse_level(text: &str) -> Result<bool, String> {
    match text {
        "0" => Ok(false),
        "1" => Ok(true),
        other => Err(format!("`{other}`: a pad level is 0 or 1")),
    }
}

/// An absolute stamp: bare or `us`-suffixed microseconds, or `ms`.
fn parse_stamp(text: &str) -> Result<Cycles, String> {
    parse_scaled(text, US)
}

/// A `+<ms>` delay, and the rest — `--usb-script`'s form, milliseconds by
/// default, `us` accepted.
fn parse_delay(text: &str) -> Result<(Cycles, &str), String> {
    let Some(after_plus) = text.strip_prefix('+') else {
        return Ok((0, text));
    };
    let (num, tail) = after_plus
        .split_once(char::is_whitespace)
        .ok_or_else(|| "`+<ms>` needs a `pin` clause after it".to_string())?;
    Ok((parse_scaled(num, MS)?, tail.trim()))
}

/// A number with an optional `us` / `ms` suffix; `default` is the scale a
/// bare number carries.
fn parse_scaled(text: &str, default: Cycles) -> Result<Cycles, String> {
    let (digits, scale) = if let Some(d) = text.strip_suffix("us") {
        (d, US)
    } else if let Some(d) = text.strip_suffix("ms") {
        (d, MS)
    } else {
        (text, default)
    };
    let n: u64 = digits
        .parse()
        .map_err(|e| format!("`{text}` is not a time: {e}"))?;
    Ok(n.saturating_mul(scale))
}

/// `button <n> press at <us> [bounce <k> edges over <us>] hold <ms>`
///
/// The shape the desk's `test_button` describes: a normally-open button to
/// ground with a pull-up, so **pressed is low** and the pad **rests high**.
/// The rest level is emitted at cycle 0, because a pull-up's *value* is not
/// modelled — an undriven pad reads low, and a script that did not say so
/// would have the firmware see the button held down from boot.
///
/// The expansion, exactly:
///
/// - cycle 0: `pin n 1` — resting, not pressed.
/// - `at`: the press. With `bounce <k> edges over <us>`, **k** edges evenly
///   spaced across `over`, alternating and starting low, so edge *i* is at
///   `at + i * over / (k - 1)` and carries `i % 2 == 0 ? 0 : 1`. **k must be
///   odd**, so the burst settles pressed; an even count is an error rather
///   than a button that bounced itself open.
/// - `at + over + hold`: `pin n 1` — released.
fn parse_button(line: &str) -> Result<Vec<(Cycles, PinEvent)>, String> {
    let w: Vec<&str> = line.split_whitespace().collect();
    let usage = "expected `button <n> press at <us> [bounce <k> edges over <us>] hold <ms>`";
    if w.len() < 6 || w[0] != "button" || w[2] != "press" || w[3] != "at" {
        return Err(format!("{usage}, got `{line}`"));
    }
    let pad = parse_pad(w[1], PadUse::Driven)?;
    let at = parse_stamp(w[4])?;
    let (edges, over, hold_at) = match &w[5..] {
        ["hold", hold] => (1u64, 0, parse_scaled(hold, MS)?),
        ["bounce", k, "edges", "over", over, "hold", hold] => {
            let k: u64 = k.parse().map_err(|e| format!("`bounce {k} edges`: {e}"))?;
            if k == 0 || k % 2 == 0 {
                return Err(format!(
                    "`bounce {k} edges`: a bounce burst must settle pressed, so the \
                     edge count is odd (1, 3, 5 …)"
                ));
            }
            (k, parse_scaled(over, US)?, parse_scaled(hold, MS)?)
        }
        _ => return Err(format!("{usage}, got `{line}`")),
    };
    let mut out = vec![(0, PinEvent { pad, level: true })];
    for i in 0..edges {
        let offset = if edges > 1 { over * i / (edges - 1) } else { 0 };
        out.push((
            at + offset,
            PinEvent {
                pad,
                level: i % 2 == 1,
            },
        ));
    }
    out.push((at + over + hold_at, PinEvent { pad, level: true }));
    Ok(out)
}

/// `encoder <a> <b> <steps> cw|ccw from <us> at <hz>`
///
/// A quadrature pair, both channels resting low. `<steps>` is the number of
/// **state transitions** (the 4×-decoded count), each changing exactly one
/// channel, and `at <hz>` is the transition rate — so transition *j* is at
/// `from + j * 1_000_000 / hz` microseconds, by integer division, which is
/// what makes the same line expand to the same cycles in every run.
///
/// The Gray sequence, from `(a, b) = (0, 0)`:
///
/// | | transition | state after |
/// |---|---|---|
/// | **cw** | a↑, b↑, a↓, b↓ | (1,0) (1,1) (0,1) (0,0) |
/// | **ccw** | b↑, a↑, b↓, a↓ | (0,1) (1,1) (1,0) (0,0) |
fn parse_encoder(line: &str) -> Result<Vec<(Cycles, PinEvent)>, String> {
    let w: Vec<&str> = line.split_whitespace().collect();
    let usage = "expected `encoder <a> <b> <steps> cw|ccw from <us> at <hz>`";
    let ["encoder", a, b, steps, dir, "from", from, "at", hz] = w[..] else {
        return Err(format!("{usage}, got `{line}`"));
    };
    let pad_a = parse_pad(a, PadUse::Driven)?;
    let pad_b = parse_pad(b, PadUse::Driven)?;
    if pad_a == pad_b {
        return Err(format!(
            "encoder {a} {b}: a quadrature pair is two different pads"
        ));
    }
    let steps: u64 = steps
        .parse()
        .map_err(|e| format!("`{steps}` is not a step count: {e}"))?;
    let cw = match dir {
        "cw" => true,
        "ccw" => false,
        other => return Err(format!("`{other}`: a direction is cw or ccw")),
    };
    let from = parse_stamp(from)?;
    let hz: u64 = hz
        .trim_end_matches("hz")
        .parse()
        .map_err(|e| format!("`{hz}` is not a rate in hz: {e}"))?;
    if hz == 0 {
        return Err("`at 0hz`: an encoder that never turns has no edges".to_string());
    }
    let period = 1_000_000u128 * u128::from(US) / u128::from(hz);
    let mut out = Vec::with_capacity(steps as usize);
    for j in 0..steps {
        let at = from + (u128::from(j) * period) as Cycles;
        // Phase within the four-transition cycle.
        let phase = (j % 4) as usize;
        let (pad, level) = if cw {
            match phase {
                0 => (pad_a, true),
                1 => (pad_b, true),
                2 => (pad_a, false),
                _ => (pad_b, false),
            }
        } else {
            match phase {
                0 => (pad_b, true),
                1 => (pad_a, true),
                2 => (pad_b, false),
                _ => (pad_a, false),
            }
        };
        out.push((at, PinEvent { pad, level }));
    }
    Ok(out)
}

/// Drop a trailing `#` comment, unless it is inside the quoted needle.
fn strip_comment(line: &str) -> &str {
    let Some(open) = line.find('"') else {
        return match line.split_once('#') {
            Some((head, _)) => head.trim_end(),
            None => line,
        };
    };
    let mut escaped = false;
    for (i, c) in line.char_indices().skip(open + 1) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => {
                let tail = &line[i + 1..];
                return match tail.split_once('#') {
                    Some(_) => line[..=i].trim_end(),
                    None => line,
                };
            }
            _ => {}
        }
    }
    line
}

/// A double-quoted, escaped string at the start of `text`, and the rest —
/// `--usb-script`'s `after` needle, byte for byte.
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

/// `--wire a:b`, parsed and checked against the pad policy.
///
/// `a` is the **TX** side — the pad whose level is being carried — which is
/// the only place GPIO18 is allowed.
pub fn parse_wire(text: &str) -> Result<(PadId, PadId), String> {
    let (a, b) = text
        .split_once(':')
        .ok_or_else(|| format!("`{text}`: a wire is `<tx pad>:<rx pad>`, for example `18:19`"))?;
    let a = parse_pad(a.trim(), PadUse::LoopbackTx)?;
    let b = parse_pad(b.trim(), PadUse::LoopbackRx)?;
    if a == b {
        return Err(format!(
            "wire {a}:{b}: a pad cannot be wired to itself (a wire ties two different pads)"
        ));
    }
    Ok((a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events_at(script: &mut PinScript, us: u64) -> Vec<(u8, bool)> {
        script
            .take_due(us * US)
            .into_iter()
            .map(|(_, e)| (e.pad.0, e.level))
            .collect()
    }

    /// The whole expansion of one script, as `(microsecond, pad, level)`.
    fn expand(text: &str) -> Vec<(u64, u8, bool)> {
        let mut script = parse_pin_script(text).unwrap();
        let mut out = Vec::new();
        // Absolute steps only: walk the queue by the cycle each asks for.
        while let Some(at) = script.next_service(0, 1) {
            let due = script.take_due(at);
            if due.is_empty() {
                break;
            }
            for (cycle, e) in due {
                assert_eq!(cycle, at, "an absolute step carries its own cycle");
                out.push((at / US, e.pad.0, e.level));
            }
        }
        out
    }

    #[test]
    fn an_absolute_line_drives_its_pad_at_its_microsecond() {
        let mut script =
            parse_pin_script("1500 pin 20 0\n2500us pin 20 1\n3ms pin 21 1\n").unwrap();
        assert_eq!(script.steps_left(), 3);
        assert_eq!(script.remaining(), 3);
        assert_eq!(script.pads(), [20, 21]);
        assert!(events_at(&mut script, 1_499).is_empty());
        assert_eq!(events_at(&mut script, 1_500), [(20, false)]);
        assert_eq!(events_at(&mut script, 2_500), [(20, true)]);
        assert_eq!(events_at(&mut script, 3_000), [(21, true)]);
        assert!(script.is_empty());
    }

    #[test]
    fn the_walk_forms_are_the_usb_scripts_and_resolve_off_the_device_log() {
        let log = ByteLog::new();
        let mut script = parse_pin_script(
            "after \"[INIT] ready\" pin 20 0\n\
             then +2ms pin 20 1\n",
        )
        .unwrap()
        .watching(log.clone());

        // Nothing said yet: the wait does not resolve and the poll cadence
        // is what the machine is told to come back on.
        assert!(events_at(&mut script, 1_000).is_empty());
        assert_eq!(script.next_service(1_000 * US, 64), Some(1_000 * US + 64));

        log.append(b"boot\n[INIT] ready\n");
        assert_eq!(events_at(&mut script, 5_000), [(20, false)]);
        // `then` paces itself off the step before: 2 ms later.
        assert!(events_at(&mut script, 6_000).is_empty());
        assert_eq!(events_at(&mut script, 7_000), [(20, true)]);
    }

    #[test]
    fn a_needle_with_no_log_to_watch_never_resolves_and_says_so_by_staying() {
        let mut script = parse_pin_script("after \"never\" pin 20 1\n").unwrap();
        assert!(events_at(&mut script, 1_000_000).is_empty());
        assert_eq!(script.remaining(), 1, "not pretended away");
    }

    /// **G1-4.** One button with bounce, expanded to the exact edge list.
    #[test]
    fn g1_4_a_button_with_bounce_expands_to_the_documented_edge_list() {
        // 5 edges over 200 us: 0, 50, 100, 150, 200 us after the press,
        // alternating from low; then released 10 ms after the burst settles.
        let got = expand("button 20 press at 1000 bounce 5 edges over 200 hold 10ms\n");
        assert_eq!(
            got,
            [
                (0, 20, true),      // resting: a pull-up, not pressed
                (1_000, 20, false), // the press
                (1_050, 20, true),  // bounce
                (1_100, 20, false),
                (1_150, 20, true),
                (1_200, 20, false), // settled pressed
                (11_200, 20, true), // released, 10 ms after the burst
            ]
        );

        // Without a bounce clause it is two edges and a rest level.
        assert_eq!(
            expand("button 20 press at 1000 hold 5ms\n"),
            [(0, 20, true), (1_000, 20, false), (6_000, 20, true)]
        );
    }

    #[test]
    fn an_even_bounce_count_would_leave_the_button_open_and_is_refused() {
        let err = parse_pin_script("button 20 press at 10 bounce 4 edges over 100 hold 1ms\n")
            .unwrap_err();
        assert!(err.contains("settle pressed"), "{err}");
        let err = parse_pin_script("button 20 press at 10\n").unwrap_err();
        assert!(err.contains("button <n> press at"), "{err}");
    }

    /// **G1-4.** One encoder run, expanded to the exact edge list.
    #[test]
    fn g1_4_an_encoder_expands_to_the_gray_sequence_at_the_stated_rate() {
        // 1 kHz: one transition every 1000 us.
        let got = expand("encoder 20 21 6 cw from 1000 at 1000hz\n");
        assert_eq!(
            got,
            [
                (1_000, 20, true),  // a^ -> (1,0)
                (2_000, 21, true),  // b^ -> (1,1)
                (3_000, 20, false), // av -> (0,1)
                (4_000, 21, false), // bv -> (0,0)
                (5_000, 20, true),  // and round again
                (6_000, 21, true),
            ]
        );
        // Counter-clockwise is the same sequence with the channels swapped.
        assert_eq!(
            expand("encoder 20 21 4 ccw from 0 at 2000hz\n"),
            [
                (0, 21, true),
                (500, 20, true),
                (1_000, 21, false),
                (1_500, 20, false),
            ]
        );
    }

    #[test]
    fn an_encoder_that_makes_no_sense_says_which_part() {
        for (text, needle) in [
            (
                "encoder 20 20 4 cw from 0 at 1000hz\n",
                "two different pads",
            ),
            ("encoder 20 21 4 sideways from 0 at 1000hz\n", "cw or ccw"),
            ("encoder 20 21 4 cw from 0 at 0hz\n", "never turns"),
            ("encoder 20 21 4 cw from 0\n", "expected `encoder"),
        ] {
            let err = parse_pin_script(text).unwrap_err();
            assert!(err.contains(needle), "{text:?} gave {err:?}");
        }
    }

    /// **G1-7.** Every forbidden pad, refused by name and with the reason,
    /// for a script line and for a wire.
    #[test]
    fn g1_7_every_forbidden_pad_is_refused_by_name_with_the_reason() {
        for (pad, why) in FORBIDDEN_PADS {
            let err = parse_pin_script(&format!("0 pin {pad} 1\n")).unwrap_err();
            assert!(err.contains(&format!("gpio{pad}")), "pad {pad}: {err}");
            assert!(err.contains(why), "pad {pad}: {err}");

            // As the RX side of a wire, every one of them including gpio18.
            let err = parse_wire(&format!("20:{pad}")).unwrap_err();
            assert!(err.contains(&format!("gpio{pad}")), "pad {pad}: {err}");

            // As the TX side, all but gpio18.
            let wire = parse_wire(&format!("{pad}:20"));
            if *pad == 18 {
                assert_eq!(wire.unwrap(), (PadId(18), PadId(20)), "the loopback");
            } else {
                let err = wire.unwrap_err();
                assert!(err.contains(&format!("gpio{pad}")), "pad {pad}: {err}");
            }
        }
        // The one exception, spelled the way M2 P3 needs it.
        assert_eq!(parse_wire("18:19").unwrap(), (PadId(18), PadId(19)));
        // And a generator is checked the same way as a plain line.
        let err = parse_pin_script("button 9 press at 0 hold 1ms\n").unwrap_err();
        assert!(err.contains("BOOT strap"), "{err}");
        let err = parse_pin_script("encoder 20 12 4 cw from 0 at 1000hz\n").unwrap_err();
        assert!(err.contains("USB D"), "{err}");
    }

    #[test]
    fn a_pad_the_chip_does_not_have_is_refused_before_the_policy_is_consulted() {
        let err = parse_pin_script("0 pin 31 1\n").unwrap_err();
        assert!(err.contains("31 pads"), "{err}");
        let err = parse_wire("20:200").unwrap_err();
        assert!(err.contains("31 pads"), "{err}");
    }

    #[test]
    fn a_wire_that_makes_no_sense_says_so() {
        let err = parse_wire("20").unwrap_err();
        assert!(err.contains("<tx pad>:<rx pad>"), "{err}");
        let err = parse_wire("20:20").unwrap_err();
        assert!(err.contains("itself"), "{err}");
    }

    #[test]
    fn a_line_that_cannot_be_read_names_its_line_number() {
        for text in [
            "pin 20 1\n",
            "abc pin 20 1\n",
            "10 pin 20 2\n",
            "10 pin twenty 1\n",
            "10 \"bytes\"\n",
            "after \"unterminated pin 20 1\n",
        ] {
            let err = parse_pin_script(text).unwrap_err();
            assert!(err.starts_with("line 1:"), "{text:?} gave {err:?}");
        }
        // A byte line is refused with the flag that does take one.
        let err = parse_pin_script("10 \"hi\"\n").unwrap_err();
        assert!(err.contains("--usb-script"), "{err}");
    }

    #[test]
    fn comments_and_blank_lines_are_skipped_and_a_hash_in_a_needle_is_not_a_comment() {
        let log = ByteLog::new();
        let mut script = parse_pin_script(
            "# a whole-line comment\n\
             \n\
             10 pin 20 1   # trailing\n\
             after \"a#b\" pin 21 1\n",
        )
        .unwrap()
        .watching(log.clone());
        assert_eq!(events_at(&mut script, 10), [(20, true)]);
        log.append(b"xxa#byy");
        assert_eq!(events_at(&mut script, 20), [(21, true)]);
    }

    #[test]
    fn two_script_files_queue_in_the_order_they_were_given() {
        let mut a = parse_pin_script("10 pin 20 1\n").unwrap();
        let b = parse_pin_script("20 pin 21 1\n").unwrap();
        a.extend(b);
        assert_eq!(a.steps_left(), 2);
        assert_eq!(a.pads(), [20, 21]);
    }
}
