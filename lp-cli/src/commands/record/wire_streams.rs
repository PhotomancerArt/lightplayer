//! Reassembles a recording's raw `wire` chunks into the messages they carry.
//!
//! A recording holds each transport chunk as it crossed the page's byte
//! chokepoint: whatever the port delivered — half a frame, three frames, a
//! board log line torn in two. So the chunks are concatenated and decoded
//! the way `lp-cli wire unpack` decodes a capture:
//!
//! - **USB ports (`serial`, `emu-tab`) are lp-links** since
//!   `WIRE_PROTO_VERSION` 30 (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`,
//!   D10): both directions of one port go through one [`WireLinkSniffer`],
//!   messages (JSON or packed) come out as their `M!{json}` JSON, console
//!   text as text, and the link's own recoveries (a new session, a damaged
//!   frame it resent) as link notes.
//! - **Other transports (`ble`) still carry `M!` lines** (plan D3): each
//!   (transport, port, direction) stream is read with [`WireUnpacker`]:
//!   JSON Pack frames become their `M!{json}` line, JSON lines are read as
//!   they are, and everything else is the board's own text.
//!
//! A message that does not decode is reported, never dropped.

use std::collections::HashMap;

use lpc_wire::lp_link::sniffer::Direction;
use lpc_wire::{SniffedWire, UnpackEvent, WireLinkSniffer, WireUnpacker};
use serde_json::Value;

/// One (transport, port, direction) byte stream.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WireStreamKey {
    pub transport: String,
    pub port: String,
    /// `tx` (page → board) or `rx` (board → page).
    pub dir: String,
}

/// What a stream's bytes turned out to hold.
#[derive(Debug, Clone, PartialEq)]
pub enum WireItem {
    /// One protocol message. `packed` is its JSON Pack size on the wire
    /// (`None` for a plain `M!{json}` line); `line_len` is the size of its
    /// `M!{json}\n` line.
    Message {
        json: String,
        packed: Option<usize>,
        line_len: usize,
    },
    /// A line of anything else (the board's own log text), newline removed.
    Text(String),
    /// A frame that could not be decoded, and why.
    Undecodable(String),
    /// The link's own account (lp-link ports): a new session, a damaged
    /// frame it resent, frames the recording never saw.
    Link(String),
}

/// Every stream of one recording, each with its own reader.
#[derive(Default)]
pub struct WireStreams {
    /// `M!`-line streams, one per (transport, port, direction).
    streams: HashMap<WireStreamKey, StreamState>,
    /// lp-link ports, one per (transport, port), both directions.
    links: HashMap<(String, String), WireLinkSniffer>,
}

impl WireStreams {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one chunk; returns what it completed, in order.
    pub fn push(&mut self, key: &WireStreamKey, bytes: &[u8]) -> Vec<WireItem> {
        if !is_link_transport(&key.transport) {
            return self.streams.entry(key.clone()).or_default().push(bytes);
        }
        let dir = match key.dir.as_str() {
            "tx" => Direction::HostToBoard,
            _ => Direction::BoardToHost,
        };
        let sniffer = self
            .links
            .entry((key.transport.clone(), key.port.clone()))
            .or_default();
        let mut items = Vec::new();
        sniffer.push(dir, 0, bytes, |item| items.extend(link_item(item)));
        items
    }

    /// What the streams still hold when the recording ends: a torn packed
    /// frame, or a last line with no newline. Sorted for a stable order.
    pub fn finish(&mut self) -> Vec<(WireStreamKey, WireItem)> {
        let mut keys: Vec<_> = self.streams.keys().cloned().collect();
        keys.sort_by(|a, b| (&a.transport, &a.port, &a.dir).cmp(&(&b.transport, &b.port, &b.dir)));
        let mut items = Vec::new();
        for key in keys {
            let state = self.streams.get_mut(&key).expect("key from the map");
            for item in state.finish() {
                items.push((key.clone(), item));
            }
        }
        let mut ports: Vec<_> = self.links.keys().cloned().collect();
        ports.sort();
        for (transport, port) in ports {
            let sniffer = self
                .links
                .get_mut(&(transport.clone(), port.clone()))
                .expect("key from the map");
            sniffer.flush(|item| {
                let key = WireStreamKey {
                    transport: transport.clone(),
                    port: port.clone(),
                    dir: match item_direction(&item) {
                        Direction::HostToBoard => "tx",
                        Direction::BoardToHost => "rx",
                    }
                    .to_string(),
                };
                if let Some(item) = link_item(item) {
                    items.push((key, item));
                }
            });
        }
        items
    }
}

/// The transports whose bytes are an lp-link: a board's USB port, in the
/// browser (`serial`) and in the tab emulator (`emu-tab`).
fn is_link_transport(transport: &str) -> bool {
    matches!(transport, "serial" | "emu-tab")
}

/// What one thing read off a link is on the timeline.
fn link_item(item: SniffedWire) -> Option<WireItem> {
    Some(match item {
        SniffedWire::Server { payload, .. } => WireItem::Message {
            line_len: payload.json.len() + "M!\n".len(),
            packed: payload.packed.then_some(payload.wire_len),
            json: payload.json,
        },
        SniffedWire::Client { json, .. } => WireItem::Message {
            line_len: json.len() + "M!\n".len(),
            packed: None,
            json,
        },
        SniffedWire::Console { line, .. } => {
            let line = line.trim_end();
            if line.is_empty() {
                return None;
            }
            WireItem::Text(line.to_string())
        }
        SniffedWire::Unreadable { len, reason, .. } => {
            WireItem::Undecodable(format!("{len} B message: {reason}"))
        }
        SniffedWire::Session { nonce, .. } => WireItem::Link(format!(
            "link session {nonce:#010x} (a reboot, reload or reconnect)"
        )),
        SniffedWire::Damaged { .. } => {
            WireItem::Link("a damaged frame (the link resent it)".to_string())
        }
        SniffedWire::Gap { skipped, .. } => {
            WireItem::Link(format!("{skipped} frame(s) missing from the recording"))
        }
        SniffedWire::Sealed { chan, len, .. } => WireItem::Link(format!(
            "sealed frame: {len} B on channel {chan} (a secure link; no key to read it)"
        )),
    })
}

fn item_direction(item: &SniffedWire) -> Direction {
    match item {
        SniffedWire::Server { .. } => Direction::BoardToHost,
        SniffedWire::Client { .. } => Direction::HostToBoard,
        SniffedWire::Console { dir, .. }
        | SniffedWire::Unreadable { dir, .. }
        | SniffedWire::Session { dir, .. }
        | SniffedWire::Damaged { dir }
        | SniffedWire::Gap { dir, .. }
        | SniffedWire::Sealed { dir, .. } => *dir,
    }
}

#[derive(Default)]
struct StreamState {
    unpacker: WireUnpacker,
    /// Bytes of the current (not yet newline-terminated) text line.
    text: Vec<u8>,
}

/// Where the unpacker's `<learned frame: …>` marker line starts in `out`.
fn find_marker(out: &[u8]) -> Option<usize> {
    const MARKER: &[u8] = b"<learned frame:";
    out.windows(MARKER.len())
        .rposition(|window| window == MARKER)
}

impl StreamState {
    fn push(&mut self, bytes: &[u8]) -> Vec<WireItem> {
        let mut items = Vec::new();
        let mut out = Vec::new();
        // One byte at a time, so that a frame the unpacker delivers is known
        // to be exactly the line it appended in that push: the text around
        // it stays text, and the frame keeps its packed size.
        for byte in bytes {
            let mut frame = None;
            self.unpacker
                .push(core::slice::from_ref(byte), &mut out, |result| {
                    frame = Some(result)
                });
            match frame {
                Some(UnpackEvent::Unpacked(frame)) => {
                    let split = out.len().saturating_sub(frame.json_line_len);
                    self.take_text(&out[..split], &mut items);
                    let line = &out[split..];
                    items.push(message_item(line, Some(frame.wire_len)));
                }
                Some(UnpackEvent::Unreadable(frame)) => {
                    // The unpacker wrote a `<learned frame: …>` marker line in
                    // the frame's place; show the frame as undecodable instead.
                    let split = find_marker(&out).unwrap_or(out.len());
                    self.take_text(&out[..split], &mut items);
                    items.push(WireItem::Undecodable(format!(
                        "learned frame, table unknown ({} B; the recording starts \
                         mid-connection or an earlier frame was lost)",
                        frame.wire_len
                    )));
                }
                Some(UnpackEvent::Dropped(error)) => {
                    self.take_text(&out, &mut items);
                    items.push(WireItem::Undecodable(error));
                }
                None => self.take_text(&out, &mut items),
            }
            out.clear();
        }
        items
    }

    /// Append passed-through bytes to the text line, closing each line at
    /// its newline.
    fn take_text(&mut self, bytes: &[u8], items: &mut Vec<WireItem>) {
        for &byte in bytes {
            if byte == b'\n' {
                let line = std::mem::take(&mut self.text);
                if let Some(item) = text_item(&line) {
                    items.push(item);
                }
            } else {
                self.text.push(byte);
            }
        }
    }

    fn finish(&mut self) -> Vec<WireItem> {
        let mut items = Vec::new();
        if self.unpacker.in_frame() {
            items.push(WireItem::Undecodable(
                "the recording ended inside a packed frame".to_string(),
            ));
        }
        let line = std::mem::take(&mut self.text);
        if let Some(item) = text_item(&line) {
            items.push(item);
        }
        items
    }
}

/// A complete line (no newline): a JSON message if it is `M!{…}`, text
/// otherwise, nothing if blank.
fn text_item(line: &[u8]) -> Option<WireItem> {
    let trimmed = line.strip_suffix(b"\r").unwrap_or(line);
    if trimmed.starts_with(b"M!") {
        let mut with_newline = trimmed.to_vec();
        with_newline.push(b'\n');
        return Some(message_item(&with_newline, None));
    }
    let text = String::from_utf8_lossy(trimmed);
    let text = text.trim_end();
    if text.is_empty() {
        None
    } else {
        Some(WireItem::Text(text.to_string()))
    }
}

/// An `M!{json}\n` line as a message.
fn message_item(line: &[u8], packed: Option<usize>) -> WireItem {
    let body = line.strip_prefix(b"M!").unwrap_or(line);
    let body = body.strip_suffix(b"\n").unwrap_or(body);
    WireItem::Message {
        json: String::from_utf8_lossy(body).into_owned(),
        packed,
        line_len: line.len(),
    }
}

/// `kind id=… seq=… fin=…` for a message's JSON, or why it is not one.
///
/// The kind is the message's `msg` tag (`"hello"`, `{"projectRead":…}`),
/// one level deeper when that is itself a single tag
/// (`{"response":{"projectRead":…}}` → `response.projectRead`).
pub fn describe_message(json: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return format!("unparsed JSON ({} B)", json.len());
    };
    let mut parts = vec![message_kind(value.get("msg"))];
    if let Some(id) = value.get("id") {
        parts.push(format!("id={id}"));
    }
    if let Some(seq) = value.get("seq") {
        parts.push(format!("seq={seq}"));
    }
    if let Some(fin) = value.get("fin") {
        parts.push(format!("fin={fin}"));
    }
    parts.join(" ")
}

fn message_kind(msg: Option<&Value>) -> String {
    match msg {
        Some(Value::String(tag)) => tag.clone(),
        Some(Value::Object(map)) if map.len() == 1 => {
            let (tag, inner) = map.iter().next().expect("one entry");
            match inner {
                Value::Object(inner) if inner.len() == 1 => {
                    let (inner_tag, _) = inner.iter().next().expect("one entry");
                    format!("{tag}.{inner_tag}")
                }
                Value::String(inner_tag) => format!("{tag}.{inner_tag}"),
                _ => tag.clone(),
            }
        }
        Some(_) => "message".to_string(),
        None => "no msg".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::wire::line_unpack::tests::packed_and_json;

    /// An `M!`-line stream (BLE).
    fn key(dir: &str) -> WireStreamKey {
        WireStreamKey {
            transport: "ble".into(),
            port: "3".into(),
            dir: dir.into(),
        }
    }

    /// A USB port's stream (an lp-link).
    fn serial(dir: &str) -> WireStreamKey {
        WireStreamKey {
            transport: "serial".into(),
            port: "3".into(),
            dir: dir.into(),
        }
    }

    /// A USB port's recording, both ways, chunk by chunk: the session, the
    /// hello, the opt-in, a request and its packed answer, all read as the
    /// messages they are, however the chunks split the frames.
    #[test]
    fn a_usb_recording_reads_as_the_link_messages_both_ways() {
        use crate::commands::wire::test_capture::{capture, log_reply};
        let session = capture("boot ok\n", &[log_reply(4)], true);
        let mut streams = WireStreams::new();
        let mut items = Vec::new();
        for (dir, bytes) in &session.chunks {
            let key = match dir {
                Direction::BoardToHost => serial("rx"),
                Direction::HostToBoard => serial("tx"),
            };
            for half in bytes.chunks(bytes.len().div_ceil(2).max(1)) {
                for item in streams.push(&key, half) {
                    items.push((key.dir.clone(), item));
                }
            }
        }
        items.extend(
            streams
                .finish()
                .into_iter()
                .map(|(key, item)| (key.dir, item)),
        );

        assert_eq!(
            items[0],
            ("rx".to_string(), WireItem::Text("boot ok".into()))
        );
        let described: Vec<(String, String)> = items
            .iter()
            .filter_map(|(dir, item)| match item {
                WireItem::Message { json, .. } => Some((dir.clone(), describe_message(json))),
                _ => None,
            })
            .collect();
        assert!(
            described.contains(&("rx".to_string(), "hello id=0".to_string())),
            "{described:?}"
        );
        assert!(
            described.contains(&("tx".to_string(), "hello id=1".to_string())),
            "{described:?}"
        );
        assert!(
            described.contains(&("rx".to_string(), "log id=4".to_string())),
            "{described:?}"
        );
        assert!(
            items.iter().any(|(_, item)| matches!(
                item,
                WireItem::Message { json, packed: Some(_), .. } if json.contains("number 4")
            )),
            "the reply went packed and reads as its JSON: {items:?}"
        );
        assert!(
            !items
                .iter()
                .any(|(_, item)| matches!(item, WireItem::Undecodable(_))),
            "{items:?}"
        );
    }

    #[test]
    fn a_packed_frame_split_across_chunks_decodes_once_with_its_size() {
        let (packed, json_line) = packed_and_json(4);
        let mut streams = WireStreams::new();
        let (a, b) = packed.split_at(5);
        let mut chunk = b"boot ok\n".to_vec();
        chunk.extend_from_slice(a);
        let first = streams.push(&key("rx"), &chunk);
        assert_eq!(first, vec![WireItem::Text("boot ok".into())]);
        let second = streams.push(&key("rx"), b);
        let [
            WireItem::Message {
                json,
                packed: Some(wire),
                line_len,
            },
        ] = second.as_slice()
        else {
            panic!("one packed message, got {second:?}");
        };
        assert_eq!(
            *wire,
            packed.len() - 1,
            "the firmware's leading newline is not the frame's"
        );
        assert_eq!(*line_len, json_line.len() - 1);
        assert!(json_line.contains(json.as_str()));
        assert_eq!(describe_message(json), "log id=4");
    }

    #[test]
    fn json_lines_and_text_are_read_as_lines_and_a_tail_is_kept() {
        let mut streams = WireStreams::new();
        let items = streams.push(
            &key("tx"),
            b"M!{\"id\":7,\"msg\":{\"projectRead\":{}}}\nhalf",
        );
        assert_eq!(items.len(), 1);
        let WireItem::Message {
            json, packed: None, ..
        } = &items[0]
        else {
            panic!("a JSON line, got {items:?}");
        };
        assert_eq!(describe_message(json), "projectRead id=7");
        assert_eq!(
            streams.finish(),
            vec![(key("tx"), WireItem::Text("half".into()))]
        );
    }

    #[test]
    fn a_torn_frame_at_the_end_is_named() {
        let (packed, _) = packed_and_json(4);
        let mut streams = WireStreams::new();
        assert!(streams.push(&key("rx"), &packed[..6]).is_empty());
        let rest = streams.finish();
        assert!(
            matches!(rest.as_slice(), [(_, WireItem::Undecodable(why))] if why.contains("ended inside"))
        );
    }

    #[test]
    fn a_nested_tag_names_both_levels() {
        assert_eq!(
            describe_message(
                r#"{"id":3,"seq":1,"fin":false,"msg":{"response":{"projectRead":{"x":1}}}}"#
            ),
            "response.projectRead id=3 seq=1 fin=false"
        );
        assert_eq!(describe_message(r#"{"id":0,"msg":"hello"}"#), "hello id=0");
    }
}
