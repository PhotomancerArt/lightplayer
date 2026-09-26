//! `lp-cli record timeline`: a recording as one readable line per event.
//!
//! Time is relative to the session's first line. Wire chunks are decoded
//! into messages ([`super::wire_streams`]) by default, or shown raw. A
//! stretch of more than [`SILENCE_GAP_SECS`] in which nothing at all
//! happened while a request was waiting on its answer gets its own line —
//! that is what a hang looks like in a recording.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Value};

use super::args::{TimelineArgs, WireView};
use super::wire_streams::{WireItem, WireStreamKey, WireStreams, describe_message};

/// Silence longer than this, with a request outstanding, is called out.
pub const SILENCE_GAP_SECS: f64 = 2.0;
/// How much of a free-text field a line shows.
const TEXT_PREVIEW: usize = 160;
/// How many bytes of a chunk `--wire raw` previews.
const RAW_PREVIEW: usize = 48;

/// What to show.
#[derive(Debug, Clone)]
pub struct TimelineOptions {
    pub wire: WireView,
    /// Seconds after the session start before which nothing is printed.
    pub since: Option<f64>,
    /// Record kinds to print (`None` = all). The silence lines always print.
    pub kinds: Option<Vec<String>>,
}

impl Default for TimelineOptions {
    fn default() -> Self {
        Self {
            wire: WireView::Frames,
            since: None,
            kinds: None,
        }
    }
}

/// Run `lp-cli record timeline`.
pub fn handle_timeline(args: &TimelineArgs) -> Result<()> {
    let options = TimelineOptions {
        wire: args.wire,
        since: args.since,
        kinds: args.kinds.as_ref().map(|kinds| {
            kinds
                .split(',')
                .map(|kind| kind.trim().to_string())
                .filter(|kind| !kind.is_empty())
                .collect()
        }),
    };
    let files = recording_files(&args.path, args.all)?;
    let many = files.len() > 1;
    for (n, file) in files.iter().enumerate() {
        let text =
            std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        if many {
            if n > 0 {
                println!();
            }
            println!("══ {}", file.display());
        }
        print!("{}", render_timeline(&text, &options));
    }
    Ok(())
}

/// The recording file(s) `path` names: itself, or — for a directory — its
/// newest `.jsonl` (every one, by name, with `all`).
pub fn recording_files(path: &Path, all: bool) -> Result<Vec<PathBuf>> {
    if !path.is_dir() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path).with_context(|| format!("reading {}", path.display()))? {
        let entry = entry?;
        let file = entry.path();
        if file.extension().is_some_and(|ext| ext == "jsonl") {
            let modified = entry.metadata()?.modified()?;
            files.push((modified, file));
        }
    }
    if files.is_empty() {
        bail!("no .jsonl recordings in {}", path.display());
    }
    if all {
        let mut files: Vec<_> = files.into_iter().map(|(_, file)| file).collect();
        files.sort();
        Ok(files)
    } else {
        files.sort();
        Ok(vec![files.pop().expect("not empty").1])
    }
}

/// Render one recording's JSONL text.
pub fn render_timeline(text: &str, options: &TimelineOptions) -> String {
    let mut records = Vec::new();
    let mut unreadable = 0usize;
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Object(map)) => records.push((n, map)),
            _ => unreadable += 1,
        }
    }
    // The sink's `seq` is the page's total order; arrival order breaks ties
    // (and orders a recording made before the seq existed).
    records.sort_by_key(|(n, map)| (map.get("seq").and_then(Value::as_u64), *n));

    let mut out = String::new();
    let start = records
        .iter()
        .find_map(|(_, map)| map.get("t").and_then(Value::as_f64))
        .unwrap_or(0.0);
    let mut renderer = Renderer {
        options,
        start,
        out: &mut out,
        streams: WireStreams::new(),
        outstanding: BTreeMap::new(),
        last_t: None,
    };
    for (_, record) in &records {
        renderer.record(record);
    }
    renderer.finish();
    if unreadable > 0 {
        let _ = writeln!(
            out,
            "({unreadable} line(s) were not JSON objects and are not shown)"
        );
    }
    out
}

struct Renderer<'a> {
    options: &'a TimelineOptions,
    start: f64,
    out: &'a mut String,
    streams: WireStreams,
    /// Requests sent and not yet answered: (conversation, id) → (sent at, kind).
    outstanding: BTreeMap<(u64, u64), (f64, String)>,
    last_t: Option<f64>,
}

impl Renderer<'_> {
    fn record(&mut self, record: &Map<String, Value>) {
        let t = record
            .get("t")
            .and_then(Value::as_f64)
            .unwrap_or(self.start);
        let kind = str_field(record, "kind");
        self.silence_before(t);
        self.last_t = Some(self.last_t.map_or(t, |last| last.max(t)));
        self.track_request(record, t);

        if kind == "wire" {
            // Decoded even when not shown, so a later chunk still reassembles.
            let lines = self.wire(record);
            if self.shows("wire", t) {
                for line in lines {
                    self.line(t, &line);
                }
            }
            return;
        }
        if !self.shows(kind, t) {
            return;
        }
        let line = describe_record(kind, record);
        self.line(t, &line);
    }

    fn finish(&mut self) {
        let t = self.last_t.unwrap_or(self.start);
        let rest = self.streams.finish();
        if self.options.wire == WireView::Frames && self.shows("wire", t) {
            for (key, item) in rest {
                let line = wire_item_line(&key, &item);
                self.line(t, &format!("{line}   (at the end of the recording)"));
            }
        }
        for ((conversation, id), (sent, kind)) in std::mem::take(&mut self.outstanding) {
            let at = sent - self.start;
            let _ = writeln!(
                self.out,
                "{:>9}  REQ      c{conversation}#{id} {kind}: no outcome (sent at +{at:.3}s)",
                "end"
            );
        }
    }

    fn shows(&self, kind: &str, t: f64) -> bool {
        if let Some(since) = self.options.since
            && t - self.start < since
        {
            return false;
        }
        if kind == "wire" && self.options.wire == WireView::Off {
            return false;
        }
        match &self.options.kinds {
            Some(kinds) => kinds.iter().any(|k| k == kind),
            None => true,
        }
    }

    fn line(&mut self, t: f64, text: &str) {
        let _ = writeln!(self.out, "{:>9}  {text}", relative(t - self.start));
    }

    /// A `… N s silent` line when nothing happened for a while with a
    /// request still waiting.
    fn silence_before(&mut self, t: f64) {
        let Some(last) = self.last_t else { return };
        let gap = t - last;
        if gap <= SILENCE_GAP_SECS || self.outstanding.is_empty() {
            return;
        }
        if self
            .options
            .since
            .is_some_and(|since| t - self.start < since)
        {
            return;
        }
        // The one that has waited longest.
        let (&(conversation, id), (_, kind)) = self
            .outstanding
            .iter()
            .min_by(|a, b| a.1.0.total_cmp(&b.1.0))
            .expect("not empty");
        let more = match self.outstanding.len() - 1 {
            0 => String::new(),
            n => format!(", +{n} more"),
        };
        let _ = writeln!(
            self.out,
            "{:>9}  … {gap:.1} s silent (request c{conversation}#{id} {kind} outstanding{more})",
            relative(last - self.start)
        );
    }

    fn track_request(&mut self, record: &Map<String, Value>, t: f64) {
        if str_field(record, "kind") != "request" {
            return;
        }
        let key = (u64_field(record, "conversation"), u64_field(record, "id"));
        match str_field(record, "phase") {
            "sent" => {
                self.outstanding
                    .insert(key, (t, str_field(record, "request").to_string()));
            }
            "outcome" => {
                self.outstanding.remove(&key);
            }
            _ => {}
        }
    }

    /// The lines one `wire` chunk produces.
    fn wire(&mut self, record: &Map<String, Value>) -> Vec<String> {
        let key = WireStreamKey {
            transport: str_field(record, "transport").to_string(),
            port: str_field(record, "port").to_string(),
            dir: str_field(record, "dir").to_string(),
        };
        let bytes = match STANDARD.decode(str_field(record, "b64")) {
            Ok(bytes) => bytes,
            Err(error) => {
                return vec![format!(
                    "{}  !! chunk is not base64: {error}",
                    wire_head(&key)
                )];
            }
        };
        match self.options.wire {
            WireView::Off => {
                self.streams.push(&key, &bytes);
                Vec::new()
            }
            WireView::Raw => {
                self.streams.push(&key, &bytes);
                vec![format!(
                    "{}  {} B  {}",
                    wire_head(&key),
                    thousands(bytes.len()),
                    preview_bytes(&bytes)
                )]
            }
            WireView::Frames => self
                .streams
                .push(&key, &bytes)
                .iter()
                .map(|item| wire_item_line(&key, item))
                .collect(),
        }
    }
}

/// One non-wire record, after the time column.
fn describe_record(kind: &str, record: &Map<String, Value>) -> String {
    let s = |field: &str| str_field(record, field);
    match kind {
        "session" => {
            let mut parts = Vec::new();
            for field in ["version", "sha", "channel", "branch"] {
                if !s(field).is_empty() {
                    parts.push(s(field).to_string());
                }
            }
            let build = if parts.is_empty() {
                "(no build stamp)".to_string()
            } else {
                parts.join(" ")
            };
            format!(
                "SESSION  {} · {build} · {} · {}",
                s("recording"),
                browser(s("user_agent")),
                s("href")
            )
        }
        "command" => format!("CMD      {} {}", s("name"), preview(s("detail"))),
        "action" => {
            let mut line = format!(
                "ACTION   {} {} ({})",
                s("name"),
                s("outcome"),
                millis(record.get("elapsed_ms"))
            );
            if !s("error").is_empty() {
                let _ = write!(line, ": {}", preview(s("error")));
            }
            line
        }
        "route" => {
            let mut line = format!("ROUTE    {} → {}", s("from"), s("to"));
            if !s("reason").is_empty() {
                let _ = write!(line, "   ({})", preview(s("reason")));
            }
            line
        }
        "open" => format!("OPEN     {}", compact(record.get("stage"))),
        "error" => format!(
            "ERROR    [{}/{}] {}",
            s("level"),
            s("source"),
            preview(s("message"))
        ),
        "toast" => format!("TOAST    [{}] {}", s("level"), preview(s("message"))),
        "journal" => format!("JOURNAL  {}  {}", s("scope"), preview(s("entry"))),
        "request" => describe_request(record),
        "state" => {
            let from = compact(record.get("from"));
            let to = compact(record.get("to"));
            if record.contains_key("from") {
                format!("STATE    {}{from} → {to}", context(record))
            } else {
                format!("STATE    {}→ {to}", context(record))
            }
        }
        "flow" => format!(
            "FLOW     {}{} → {}",
            context(record),
            compact(record.get("from")),
            compact(record.get("to"))
        ),
        "pool" => format!(
            "POOL     {}{} {}",
            context(record),
            s("action"),
            preview(s("detail"))
        ),
        "mgmt" => format!(
            "MGMT     {}{} {}",
            context(record),
            s("phase"),
            preview(s("label"))
        ),
        "sweep" => format!(
            "SWEEP    {}{}",
            context(record),
            compact(record.get("disposition"))
        ),
        "sync" => format!(
            "SYNC     {}{}",
            context(record),
            compact(record.get("content"))
        ),
        "anomaly" => format!("ANOMALY  {}{}", context(record), preview(s("detail"))),
        "rx" => format!("RX       {}{}", context(record), preview(s("line"))),
        "tx" => format!("TX       {}{}", context(record), preview(s("frame"))),
        other => {
            let mut rest = record.clone();
            for field in ["seq", "t", "kind"] {
                rest.remove(field);
            }
            format!(
                "{:<8} {}",
                other.to_uppercase(),
                preview(&Value::Object(rest).to_string())
            )
        }
    }
}

fn describe_request(record: &Map<String, Value>) -> String {
    let s = |field: &str| str_field(record, field);
    let name = format!(
        "c{}#{}",
        u64_field(record, "conversation"),
        u64_field(record, "id")
    );
    match s("phase") {
        "sent" => format!("REQ      {name} {} sent", s("request")),
        "frame" => {
            let mut line = format!(
                "REQ      {name} frame ← id={} seq={} fin={} {}",
                compact(record.get("response_id")),
                compact(record.get("frame_seq")),
                compact(record.get("fin")),
                s("disposition")
            );
            if let Some(since) = record.get("since_sent_ms") {
                let _ = write!(line, " +{}", millis(Some(since)));
            }
            line
        }
        "outcome" => {
            let mut line = format!("REQ      {name}");
            if !s("request").is_empty() {
                let _ = write!(line, " {}", s("request"));
            }
            let _ = write!(line, " {}", s("outcome"));
            if let Some(latency) = record.get("latency_ms") {
                let _ = write!(line, " in {}", millis(Some(latency)));
            }
            if let Some(budget) = record.get("budget_ms") {
                let _ = write!(line, " (budget {})", millis(Some(budget)));
            }
            if !s("error").is_empty() {
                let _ = write!(line, ": {}", preview(s("error")));
            }
            line
        }
        other => format!("REQ      {name} {other}"),
    }
}

fn wire_head(key: &WireStreamKey) -> String {
    let arrow = match key.dir.as_str() {
        "tx" => "→",
        "rx" => "←",
        _ => "?",
    };
    format!("WIRE  {arrow}  {}:{}", key.transport, key.port)
}

fn wire_item_line(key: &WireStreamKey, item: &WireItem) -> String {
    let head = wire_head(key);
    match item {
        WireItem::Message {
            json,
            packed,
            line_len,
        } => match packed {
            Some(wire_len) => format!(
                "{head}  {} {} B packed",
                describe_message(json),
                thousands(*wire_len)
            ),
            None => format!(
                "{head}  {} {} B",
                describe_message(json),
                thousands(*line_len)
            ),
        },
        WireItem::Text(text) => format!("{head}  | {}", preview(text)),
        WireItem::Undecodable(why) => format!("{head}  !! undecodable frame: {why}"),
    }
}

/// `session`/`endpoint` prefix of a device-event record.
fn context(record: &Map<String, Value>) -> String {
    let mut parts = Vec::new();
    for field in ["endpoint", "session"] {
        if let Some(value) = record.get(field) {
            parts.push(compact(Some(value)));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("[{}] ", parts.join(" "))
    }
}

/// `Chrome 152` out of a user agent, or the agent itself, shortened.
fn browser(user_agent: &str) -> String {
    for name in ["Edg/", "HeadlessChrome/", "Chrome/", "Firefox/", "Version/"] {
        if let Some(rest) = user_agent.split(name).nth(1) {
            let major = rest.split(['.', ' ']).next().unwrap_or("");
            let name = match name {
                "Edg/" => "Edge",
                "Version/" => "Safari",
                other => other.trim_end_matches('/'),
            };
            return format!("{name} {major}");
        }
    }
    preview(user_agent)
}

fn str_field<'a>(record: &'a Map<String, Value>, field: &str) -> &'a str {
    record.get(field).and_then(Value::as_str).unwrap_or("")
}

fn u64_field(record: &Map<String, Value>, field: &str) -> u64 {
    record.get(field).and_then(Value::as_u64).unwrap_or(0)
}

/// A value as short text: strings bare, everything else as compact JSON.
fn compact(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "-".to_string(),
        Some(Value::String(s)) => preview(s),
        Some(other) => preview(&other.to_string()),
    }
}

fn millis(value: Option<&Value>) -> String {
    match value.and_then(Value::as_f64) {
        Some(ms) if ms >= 1000.0 => format!("{:.2} s", ms / 1000.0),
        Some(ms) => format!("{ms:.1} ms"),
        None => "? ms".to_string(),
    }
}

fn relative(secs: f64) -> String {
    format!("{secs:+.3}s")
}

/// One line of at most [`TEXT_PREVIEW`] characters.
fn preview(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    if flat.chars().count() <= TEXT_PREVIEW {
        flat
    } else {
        let cut: String = flat.chars().take(TEXT_PREVIEW).collect();
        format!("{cut}…")
    }
}

/// A chunk's first bytes: escaped text when it reads as text, hex when not.
fn preview_bytes(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(RAW_PREVIEW)];
    let printable = head
        .iter()
        .filter(|&&b| b.is_ascii_graphic() || b == b' ' || b == b'\n' || b == b'\r')
        .count();
    let tail = if bytes.len() > head.len() { "…" } else { "" };
    if printable * 10 >= head.len() * 9 {
        format!("\"{}\"{tail}", head.escape_ascii())
    } else {
        let hex: Vec<String> = head.iter().map(|b| format!("{b:02x}")).collect();
        format!("{}{tail}", hex.join(" "))
    }
}

fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hang_reads_as_a_silence_with_the_request_named() {
        let text = [
            r#"{"seq":0,"t":100.0,"kind":"session","recording":"abc","user_agent":"Mozilla/5.0 Chrome/152.0.1 Safari/537.36","href":"http://127.0.0.1/"}"#,
            r#"{"seq":1,"t":100.5,"kind":"request","phase":"sent","conversation":1,"id":41,"request":"project.read"}"#,
            r#"{"seq":2,"t":105.5,"kind":"route","from":"/p/x","to":"/devices","reason":"editor-ended"}"#,
        ]
        .join("\n");
        let out = render_timeline(&text, &TimelineOptions::default());
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines[0],
            "  +0.000s  SESSION  abc · (no build stamp) · Chrome 152 · http://127.0.0.1/"
        );
        assert_eq!(lines[1], "  +0.500s  REQ      c1#41 project.read sent");
        assert_eq!(
            lines[2],
            "  +0.500s  … 5.0 s silent (request c1#41 project.read outstanding)"
        );
        assert_eq!(
            lines[3],
            "  +5.500s  ROUTE    /p/x → /devices   (editor-ended)"
        );
        assert_eq!(
            lines[4],
            "      end  REQ      c1#41 project.read: no outcome (sent at +0.500s)"
        );
    }

    #[test]
    fn kinds_and_since_filter_but_wire_still_reassembles() {
        // `M!{"id":1,"msg":"hello"}\n` split across two chunks.
        let a = STANDARD.encode(br#"M!{"id":1,"#);
        let b = STANDARD.encode(b"\"msg\":\"hello\"}\n");
        let text = format!(
            "{}\n{}\n{}\n",
            r#"{"seq":0,"t":0.0,"kind":"session","recording":"r"}"#,
            format_args!(
                r#"{{"seq":1,"t":1.0,"kind":"wire","dir":"rx","transport":"serial","port":"3","len":10,"b64":"{a}"}}"#
            ),
            format_args!(
                r#"{{"seq":2,"t":3.0,"kind":"wire","dir":"rx","transport":"serial","port":"3","len":15,"b64":"{b}"}}"#
            ),
        );
        let options = TimelineOptions {
            since: Some(2.0),
            kinds: Some(vec!["wire".into()]),
            ..TimelineOptions::default()
        };
        assert_eq!(
            render_timeline(&text, &options),
            "  +3.000s  WIRE  ←  serial:3  hello id=1 25 B\n"
        );
        let raw = TimelineOptions {
            wire: WireView::Raw,
            kinds: Some(vec!["wire".into()]),
            ..TimelineOptions::default()
        };
        assert_eq!(
            render_timeline(&text, &raw).lines().next().unwrap(),
            r#"  +1.000s  WIRE  ←  serial:3  10 B  "M!{\"id\":1,""#
        );
    }

    #[test]
    fn numbers_read_naturally() {
        assert_eq!(thousands(1203), "1,203");
        assert_eq!(thousands(12), "12");
        assert_eq!(thousands(1_000_000), "1,000,000");
        assert_eq!(millis(Some(&Value::from(31.25))), "31.2 ms");
        assert_eq!(millis(Some(&Value::from(2500.0))), "2.50 s");
        assert_eq!(preview_bytes(&[0, 0x50, 1, 2]), "00 50 01 02");
    }
}
