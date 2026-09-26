//! One port's reading half: bytes in, lines and wire messages out, and the
//! packed-reply opt-in kept honest along the way.
//!
//! [`WireReader`] is [`lpc_wire::WireStream`] (the frame-aware splitter every
//! host reader shares) plus [`lpc_wire::PackOptIn`] (when to ask a board to
//! pack its replies, and when to ask again). It exists so a port with **more
//! than one drainer** keeps one of each: the browser's Web Serial port is
//! drained by the model's link pump and, while the editor lens or a coarse
//! effect borrows the wire, by an `lpa-client` conversation. Two splitters
//! would each tear the frame that straddles the handover; two opt-in states
//! would disagree about what the board was asked.
//!
//! Sans-IO: time is the caller's (`now_ms`), and the opt-in request is handed
//! back as [`WireRead::Send`] for the caller to write.
//!
//! It also says, once per change, what the link's encoding is and why
//! ([`WireRead::Note`]) — a board that stays on JSON because its pack format
//! is not this build's is otherwise invisible, since every reader decodes
//! both forms. And when the stream's learned table loses step with the
//! board's (a packed frame lost in flight), it drops frames until the board
//! resets, asks for that reset through the opt-in, and says so once.
//!
//! One dev-only rider ([`WireReader::with_device_log_level`], Studio's
//! `?device-log=<level>`): once the board has said hello and the opt-in is
//! settled, the reader asks it once for that log level, and swallows the
//! answer into a note — the same shape as the opt-in, so no drainer sees a
//! reply to a request it did not make.

use std::cell::Cell;

use lpc_wire::server::api::LogLevel;
use lpc_wire::{
    ClientMessage, ClientRequest, PACK_FORMAT_VERSION, PACK_OPT_IN_REQUEST_ID, PackOptIn,
    ServerMsgBody, WIRE_PROTO_VERSION, WIRE_STREAM_MAX_FRAME, WireChunk, WireEncoding,
    WireServerMessage, WireStream,
};

thread_local! {
    /// Whether this page's browser readers ask boards to pack. See
    /// [`set_packed_replies_wanted`].
    static PACKED_REPLIES_WANTED: Cell<bool> = const { Cell::new(true) };
    /// The dev log level this page's browser readers ask boards for. See
    /// [`set_device_log_level`].
    static DEVICE_LOG_LEVEL: Cell<Option<LogLevel>> = const { Cell::new(None) };
}

/// Dev-only (Studio's `?device-log=<level>`): the log level the browser's
/// Web Serial readers ask each board for, once per link, after its hello and
/// the opt-in. `None` (the default) never asks. Readers built after the call
/// take it.
pub fn set_device_log_level(level: Option<LogLevel>) {
    DEVICE_LOG_LEVEL.with(|cell| cell.set(level));
}

/// See [`set_device_log_level`].
pub fn device_log_level() -> Option<LogLevel> {
    DEVICE_LOG_LEVEL.with(Cell::get)
}

/// Whether the browser's readers (Web Serial and the tab emulator) ask a
/// board to pack its replies. On by default; Studio's dev-only `?wire=json`
/// turns it off for a page, so the same build can be measured both ways.
/// Readers built after the call take it; one already reading keeps what it
/// had.
pub fn set_packed_replies_wanted(wanted: bool) {
    PACKED_REPLIES_WANTED.with(|cell| cell.set(wanted));
}

/// See [`set_packed_replies_wanted`].
pub fn packed_replies_wanted() -> bool {
    PACKED_REPLIES_WANTED.with(Cell::get)
}

/// The id the dev log-level request goes out with: one below the opt-in's,
/// so it is as far from any request counter and still exact in JavaScript.
pub const DEVICE_LOG_LEVEL_REQUEST_ID: u64 = PACK_OPT_IN_REQUEST_ID - 1;

/// One thing a [`WireReader`] produced, in stream order.
#[derive(Debug)]
pub enum WireRead {
    /// A console line (not a wire message).
    Line(String),
    /// One wire message, decoded once here so no reader decodes it twice.
    Frame(ReadFrame),
    /// A packed frame that could not be delivered (torn, too long, not
    /// decodable). Never silence.
    Error(String),
    /// Write this request to the board now, as an `M!{json}` line.
    Send(ClientMessage),
    /// The link's encoding changed, or was settled, and this says how. At
    /// most one per change; never per frame.
    Note(String),
}

/// One wire message and the form it came in.
#[derive(Debug)]
pub struct ReadFrame {
    /// The JSON its `M!` line carries (or, for a packed frame, would have).
    pub json: String,
    /// Whether it came as a packed frame.
    pub packed: bool,
    /// The message, or why the JSON did not decode (a JSON line with console
    /// text spliced into it — the demux resyncs those).
    pub message: Result<WireServerMessage, String>,
}

impl ReadFrame {
    /// The `M!{json}` line this message is, or stands for.
    pub fn to_line(&self) -> String {
        format!("M!{}", self.json)
    }
}

/// What the reader last said about the link, so it says each thing once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Told {
    Nothing,
    StaysJson,
    Packed,
    FellBack,
}

/// One port's reader. See the module docs.
pub struct WireReader {
    stream: WireStream,
    opt_in: PackOptIn,
    wanted: bool,
    told: Told,
    /// Whether a fallback has been noted on this link (only the first is).
    fallback_noted: bool,
    /// Packed frames since the link last (re)opened, for the notes.
    packed_frames: u64,
    /// Packed frames dropped since the link last (re)opened because the
    /// learned table was out of step.
    desynced_frames: u64,
    /// Whether the current out-of-step episode has been noted (once each).
    desync_noted: bool,
    /// The dev log level to ask for, once per link (`None`: never ask).
    device_log: Option<LogLevel>,
    /// Where the dev log-level request stands on this link.
    device_log_step: DeviceLogStep,
}

/// The dev log-level request's progress on one link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceLogStep {
    /// No hello yet.
    WaitingForHello,
    /// Hello seen; the opt-in went out and its answer has not come.
    WaitingForOptIn,
    /// Hello seen and the opt-in settled (or never asked): ask now.
    Due,
    /// Asked.
    Sent,
}

impl WireReader {
    /// A reader that asks for packed replies when `wanted`.
    pub fn new(wanted: bool) -> Self {
        Self {
            stream: WireStream::new(),
            opt_in: PackOptIn::new(wanted),
            wanted,
            told: Told::Nothing,
            fallback_noted: false,
            packed_frames: 0,
            desynced_frames: 0,
            desync_noted: false,
            device_log: None,
            device_log_step: DeviceLogStep::WaitingForHello,
        }
    }

    /// Dev-only: also ask the board for `level` once per link, after its
    /// hello and the opt-in. `None` never asks.
    pub fn with_device_log_level(mut self, level: Option<LogLevel>) -> Self {
        self.device_log = level;
        self
    }

    /// The encoding the board is writing this link's replies in, as far as
    /// this side knows.
    pub fn encoding(&self) -> WireEncoding {
        self.opt_in.encoding()
    }

    /// Packed frames dropped on this link because the learned table was out
    /// of step with the board's.
    pub fn desynced_frames(&self) -> u64 {
        self.desynced_frames
    }

    /// Feed bytes as they arrive (any split) at `now_ms`, calling `on` for
    /// everything they complete, in stream order.
    pub fn push(&mut self, bytes: &[u8], now_ms: u64, mut on: impl FnMut(WireRead)) {
        let Self {
            stream,
            opt_in,
            wanted,
            told,
            fallback_noted,
            packed_frames,
            desynced_frames,
            desync_noted,
            device_log,
            device_log_step,
        } = self;
        stream.push(bytes, |chunk| match chunk {
            WireChunk::Line(line) => on(WireRead::Line(line)),
            WireChunk::Error(error) => on(WireRead::Error(error)),
            WireChunk::Desync(dropped) => {
                *desynced_frames += 1;
                if !*desync_noted {
                    *desync_noted = true;
                    on(WireRead::Note(format!(
                        "wire: packed reply dropped ({} B) — {}; asking the board to reset its \
                         table",
                        dropped.wire_len, dropped.reason
                    )));
                }
                // Every time: the opt-in's rate limit decides when it goes.
                if let Some(request) = opt_in.desynced(now_ms) {
                    on(WireRead::Send(request));
                }
            }
            WireChunk::Frame(frame) => {
                let packed = frame.is_packed();
                if packed && *desync_noted {
                    *desync_noted = false;
                    on(WireRead::Note(format!(
                        "wire: back in step after {desynced_frames} dropped packed reply(ies)"
                    )));
                }
                let message = lpc_wire::json::from_str::<WireServerMessage>(&frame.json)
                    .map_err(|error| format!("malformed M! frame: {error}"));
                let Ok(decoded) = &message else {
                    on(WireRead::Frame(ReadFrame {
                        json: frame.json,
                        packed,
                        message,
                    }));
                    return;
                };
                if packed {
                    *packed_frames += 1;
                }
                if decoded.id == DEVICE_LOG_LEVEL_REQUEST_ID
                    && let Some(note) = device_log_answer(decoded)
                {
                    on(WireRead::Note(note));
                    return;
                }
                let before = opt_in.encoding();
                let step = opt_in.observe(decoded, packed, now_ms);
                let after = opt_in.encoding();
                let seen = Seen {
                    before,
                    after,
                    packed_frames: *packed_frames,
                };
                if let Some(note) = note_for(*wanted, told, fallback_noted, decoded, seen) {
                    on(WireRead::Note(note));
                }
                let opt_in_sent = step.send.is_some();
                if let Some(request) = step.send {
                    on(WireRead::Send(request));
                }
                if let Some(level) = *device_log {
                    *device_log_step = next_device_log_step(*device_log_step, decoded, opt_in_sent);
                    if *device_log_step == DeviceLogStep::Due {
                        *device_log_step = DeviceLogStep::Sent;
                        on(WireRead::Send(ClientMessage {
                            id: DEVICE_LOG_LEVEL_REQUEST_ID,
                            msg: ClientRequest::SetLogLevel { level },
                        }));
                    }
                }
                if step.deliver {
                    on(WireRead::Frame(ReadFrame {
                        json: frame.json,
                        packed,
                        message,
                    }));
                }
            }
        });
    }

    /// Forget a partial line or frame and the opt-in: the port (re)opened or
    /// the board reset, and the board has forgotten it was asked.
    pub fn clear(&mut self) {
        *self = Self::new(self.wanted).with_device_log_level(self.device_log);
    }
}

/// Where the dev log-level request stands after `message`, `opt_in_sent` when
/// the reader just wrote the opt-in because of it.
fn next_device_log_step(
    step: DeviceLogStep,
    message: &WireServerMessage,
    opt_in_sent: bool,
) -> DeviceLogStep {
    let hello = matches!(message.msg, ServerMsgBody::Hello(_));
    let opt_in_answer = message.id == PACK_OPT_IN_REQUEST_ID;
    match step {
        DeviceLogStep::Sent => DeviceLogStep::Sent,
        DeviceLogStep::WaitingForHello if !hello => DeviceLogStep::WaitingForHello,
        _ if opt_in_sent => DeviceLogStep::WaitingForOptIn,
        DeviceLogStep::WaitingForOptIn if !opt_in_answer => DeviceLogStep::WaitingForOptIn,
        _ => DeviceLogStep::Due,
    }
}

/// The note the board's answer to the dev log-level request earns.
fn device_log_answer(message: &WireServerMessage) -> Option<String> {
    Some(match &message.msg {
        ServerMsgBody::SetLogLevel => "dev: the board applied the requested log level".to_string(),
        ServerMsgBody::Error { error } => {
            format!("dev: the board refused the requested log level: {error}")
        }
        _ => return None,
    })
}

/// What one message moved: the encoding before and after it, and the packed
/// frames so far.
struct Seen {
    before: WireEncoding,
    after: WireEncoding,
    packed_frames: u64,
}

/// The one note (if any) this message earns. See [`Told`].
fn note_for(
    wanted: bool,
    told: &mut Told,
    fallback_noted: &mut bool,
    message: &WireServerMessage,
    seen: Seen,
) -> Option<String> {
    let Seen {
        before,
        after,
        packed_frames,
    } = seen;
    if !wanted {
        return None;
    }
    let own_answer = message.id == PACK_OPT_IN_REQUEST_ID;
    let (next, text) = match &message.msg {
        ServerMsgBody::Hello(hello) if hello.proto != WIRE_PROTO_VERSION => (
            Told::StaysJson,
            format!(
                "wire: replies stay JSON — the board speaks proto {}, this build {}",
                hello.proto, WIRE_PROTO_VERSION
            ),
        ),
        ServerMsgBody::Hello(hello) if hello.pack_format == 0 => (
            Told::StaysJson,
            "wire: replies stay JSON — the board does not pack (its hello offers no pack format)"
                .to_string(),
        ),
        ServerMsgBody::Hello(hello) if hello.pack_format != PACK_FORMAT_VERSION => (
            Told::StaysJson,
            format!(
                "wire: replies stay JSON — the board packs JSON Pack v{}, this build \
                 v{PACK_FORMAT_VERSION}",
                hello.pack_format
            ),
        ),
        ServerMsgBody::SetEncoding {
            encoding: WireEncoding::Json,
        } if own_answer => (
            Told::StaysJson,
            "wire: replies stay JSON — the board answered the opt-in with `json`".to_string(),
        ),
        _ if before == WireEncoding::Json && after == WireEncoding::Packed => (
            Told::Packed,
            format!("wire: replies packed (JSON Pack v{PACK_FORMAT_VERSION}, learned)"),
        ),
        // A board that drops the opt-in (a de-enumerate, a reboot, a host
        // that stopped draining) is asked again by `PackOptIn`; the first
        // fallback is worth a line, a board that keeps doing it is not
        // worth one each time.
        _ if before == WireEncoding::Packed && after == WireEncoding::Json && !*fallback_noted => {
            *fallback_noted = true;
            (
                Told::FellBack,
                format!(
                    "wire: the board fell back to JSON after {packed_frames} packed frame(s); \
                     asking again"
                ),
            )
        }
        _ => return None,
    };
    // A repeat of what was last said is not a change. The one exception is
    // `Packed`: after a fallback, packing again IS news.
    if next == *told {
        return None;
    }
    *told = next;
    Some(text)
}

/// The length of the longest prefix of `bytes` that ends on a boundary: after
/// a newline outside any packed frame, or after a packed frame's closing
/// `0x00`. For a drainer that hands the rest back for the next one to read
/// (the tab emulator's conversation io), so a packed frame is never cut —
/// which a plain "up to the last newline" cut does, since a frame may hold
/// any byte, `\n` included.
///
/// `bytes` must start on a boundary (the previous call's cut). A frame body
/// longer than [`WIRE_STREAM_MAX_FRAME`] is treated as the torn frame it is
/// and the whole buffer is released, so a stray `0x00` cannot hold bytes back
/// forever.
pub fn complete_prefix_len(bytes: &[u8]) -> usize {
    let mut cut = 0;
    let mut frame_start: Option<usize> = None;
    for (at, &byte) in bytes.iter().enumerate() {
        match (byte, frame_start) {
            (0, None) => frame_start = Some(at),
            (0, Some(_)) => {
                frame_start = None;
                cut = at + 1;
            }
            (b'\n', None) => cut = at + 1,
            _ => {}
        }
    }
    match frame_start {
        Some(start) if bytes.len() - start > WIRE_STREAM_MAX_FRAME => bytes.len(),
        _ => cut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::server::hello::{BuildFacts, HardwareFacts, ServerHello};

    #[test]
    fn a_matching_hello_asks_and_the_answer_is_swallowed() {
        let mut reader = WireReader::new(true);
        let mut reads = Vec::new();

        reader.push(&json_line(&hello(PACK_FORMAT_VERSION)), 0, |r| {
            reads.push(r)
        });
        assert!(matches!(&reads[0], WireRead::Send(ask) if ask.id == PACK_OPT_IN_REQUEST_ID));
        assert!(matches!(&reads[1], WireRead::Frame(frame) if !frame.packed));
        assert_eq!(reads.len(), 2);

        reads.clear();
        reader.push(&json_line(&answer(WireEncoding::Packed)), 5, |r| {
            reads.push(r)
        });
        assert_eq!(reads.len(), 1, "{reads:?}");
        assert!(matches!(&reads[0], WireRead::Note(note) if note.contains("replies packed")));
        assert_eq!(reader.encoding(), WireEncoding::Packed);

        reads.clear();
        reader.push(&packed(&other(4)), 10, |r| reads.push(r));
        assert!(matches!(
            reads.as_slice(),
            [WireRead::Line(empty), WireRead::Frame(frame)] if empty.is_empty() && frame.packed
        ));
    }

    #[test]
    fn a_board_in_another_pack_format_stays_json_and_says_so_once() {
        let mut reader = WireReader::new(true);
        let mut notes = Vec::new();
        for t in [0, 10_000] {
            reader.push(&json_line(&hello(PACK_FORMAT_VERSION + 1)), t, |r| {
                assert!(!matches!(r, WireRead::Send(_)), "never asked");
                if let WireRead::Note(note) = r {
                    notes.push(note);
                }
            });
        }
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("stay JSON") && notes[0].contains("JSON Pack v"));
    }

    #[test]
    fn a_board_that_does_not_pack_stays_json_and_says_so() {
        let mut reader = WireReader::new(true);
        let notes = notes_of(&mut reader, &json_line(&hello(0)), 0);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("does not pack"), "{notes:?}");
    }

    #[test]
    fn a_refusal_is_noted() {
        let mut reader = WireReader::new(true);
        notes_of(&mut reader, &json_line(&hello(PACK_FORMAT_VERSION)), 0);
        let notes = notes_of(&mut reader, &json_line(&answer(WireEncoding::Json)), 1);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("answered the opt-in with `json`"));
    }

    #[test]
    fn a_fallback_is_noted_once_and_packing_again_is_news() {
        let mut reader = WireReader::new(true);
        notes_of(&mut reader, &json_line(&hello(PACK_FORMAT_VERSION)), 0);
        notes_of(&mut reader, &json_line(&answer(WireEncoding::Packed)), 1);

        let fell = notes_of(&mut reader, &json_line(&other(0)), 5_000);
        assert_eq!(fell.len(), 1);
        assert!(fell[0].contains("fell back"), "{fell:?}");

        let again = notes_of(&mut reader, &packed(&other(0)), 5_100);
        assert!(again[0].contains("replies packed"), "{again:?}");

        // A second fallback is not worth another line.
        assert!(notes_of(&mut reader, &json_line(&other(0)), 20_000).is_empty());
    }

    /// A packed reply that taught the board new names is lost in flight: the
    /// reader drops what follows, notes it once, re-asks through the opt-in
    /// (rate-limited), and is back in step at the board's reset.
    #[test]
    fn a_lost_packed_reply_desyncs_asks_for_a_reset_and_recovers() {
        use lpc_wire::LearnStore;
        let mut reader = WireReader::new(true);
        notes_of(&mut reader, &json_line(&hello(PACK_FORMAT_VERSION)), 0);
        notes_of(&mut reader, &json_line(&answer(WireEncoding::Packed)), 1);
        let mut board = lp_json_pack_table();
        board.reset(1);
        let log = |id, text: &str| {
            WireServerMessage::new(
                id,
                ServerMsgBody::Log {
                    level: LogLevel::Info,
                    message: text.to_string(),
                },
            )
        };
        let mut reads = Vec::new();
        reader.push(&packed_on(&mut board, &log(1, "a")), 10, |r| reads.push(r));
        assert!(
            reads
                .iter()
                .any(|r| matches!(r, WireRead::Frame(f) if f.packed))
        );

        let _lost = packed_on(&mut board, &log(2, "b"));
        reads.clear();
        reader.push(&packed_on(&mut board, &log(3, "c")), 1_000, |r| {
            reads.push(r)
        });
        assert!(
            reads
                .iter()
                .any(|r| matches!(r, WireRead::Note(n) if n.contains("dropped"))),
            "{reads:?}"
        );
        assert!(
            !reads.iter().any(|r| matches!(r, WireRead::Frame(_))),
            "{reads:?}"
        );
        // Inside the opt-in's interval: not asked yet.
        assert!(
            !reads.iter().any(|r| matches!(r, WireRead::Send(_))),
            "{reads:?}"
        );

        reads.clear();
        reader.push(&packed_on(&mut board, &log(4, "d")), 3_500, |r| {
            reads.push(r)
        });
        assert!(
            reads
                .iter()
                .any(|r| matches!(r, WireRead::Send(ask) if ask.id == PACK_OPT_IN_REQUEST_ID)),
            "{reads:?}"
        );
        assert!(
            !reads.iter().any(|r| matches!(r, WireRead::Note(_))),
            "noted once per episode: {reads:?}"
        );
        assert_eq!(reader.desynced_frames(), 2);

        // The board answers and resets its table.
        reader.push(&json_line(&answer(WireEncoding::Packed)), 3_600, |_| {});
        board.reset(2);
        reads.clear();
        reader.push(&packed_on(&mut board, &log(5, "e")), 3_700, |r| {
            reads.push(r)
        });
        assert!(
            reads
                .iter()
                .any(|r| matches!(r, WireRead::Note(n) if n.contains("back in step"))),
            "{reads:?}"
        );
        assert!(
            reads
                .iter()
                .any(|r| matches!(r, WireRead::Frame(f) if f.packed))
        );
    }

    #[test]
    fn a_host_that_wants_json_never_asks_and_never_notes() {
        let mut reader = WireReader::new(false);
        let mut reads = Vec::new();
        reader.push(&json_line(&hello(PACK_FORMAT_VERSION)), 0, |r| {
            reads.push(r)
        });
        assert!(
            matches!(reads.as_slice(), [WireRead::Frame(_)]),
            "{reads:?}"
        );
    }

    #[test]
    fn clear_forgets_the_opt_in_and_the_partial_frame() {
        let mut reader = WireReader::new(true);
        notes_of(&mut reader, &json_line(&hello(PACK_FORMAT_VERSION)), 0);
        notes_of(&mut reader, &json_line(&answer(WireEncoding::Packed)), 1);
        let frame = packed(&other(9));
        reader.push(&frame[..frame.len() / 2], 2, |_| {});

        reader.clear();

        assert_eq!(reader.encoding(), WireEncoding::Json);
        let mut reads = Vec::new();
        reader.push(b"after\n", 3, |r| reads.push(r));
        assert!(
            matches!(reads.as_slice(), [WireRead::Line(line)] if line == "after"),
            "{reads:?}"
        );
    }

    #[test]
    fn a_json_line_with_spliced_text_is_still_handed_on() {
        let mut reader = WireReader::new(true);
        let mut reads = Vec::new();
        reader.push(b"M!{\"id\":0,\"msM![INIT] log\n", 0, |r| reads.push(r));
        assert!(matches!(&reads[0], WireRead::Frame(frame) if frame.message.is_err()));
    }

    #[test]
    fn the_complete_prefix_never_cuts_a_packed_frame() {
        let frame = packed(&other(10));
        let body = &frame[1..];
        assert!(
            body.contains(&b'\n'),
            "id 10 puts a newline byte inside the frame body"
        );
        let mut bytes = b"boot line\n".to_vec();
        bytes.extend_from_slice(&frame);
        bytes.extend_from_slice(b"half a li");

        // Whole: up to the frame's closing zero, the partial line held back.
        assert_eq!(complete_prefix_len(&bytes), bytes.len() - "half a li".len());
        // Cut inside the frame: back to before it, whatever it holds.
        let inside = 10 + frame.len() - 2;
        assert_eq!(complete_prefix_len(&bytes[..inside]), 11);
        assert_eq!(complete_prefix_len(b"no newline"), 0);
    }

    #[test]
    fn the_dev_log_level_is_asked_once_after_the_opt_in_settles() {
        let mut reader = WireReader::new(true).with_device_log_level(Some(LogLevel::Debug));
        let mut sends = Vec::new();
        let mut on = |r: WireRead, sends: &mut Vec<u64>| {
            if let WireRead::Send(request) = r {
                sends.push(request.id);
            }
        };
        reader.push(&json_line(&other(1)), 0, |r| on(r, &mut sends));
        assert!(sends.is_empty(), "nothing before the hello");
        reader.push(&json_line(&hello(PACK_FORMAT_VERSION)), 0, |r| {
            on(r, &mut sends)
        });
        assert_eq!(sends, [PACK_OPT_IN_REQUEST_ID], "the opt-in first, alone");
        reader.push(&json_line(&answer(WireEncoding::Packed)), 5, |r| {
            on(r, &mut sends)
        });
        assert_eq!(sends, [PACK_OPT_IN_REQUEST_ID, DEVICE_LOG_LEVEL_REQUEST_ID]);
        reader.push(&packed(&other(2)), 10, |r| on(r, &mut sends));
        assert_eq!(sends.len(), 2, "once per link");

        let ack = WireServerMessage::new(DEVICE_LOG_LEVEL_REQUEST_ID, ServerMsgBody::SetLogLevel);
        let mut reads = Vec::new();
        reader.push(&packed(&ack), 20, |r| reads.push(r));
        assert!(
            matches!(reads.as_slice(), [WireRead::Line(_), WireRead::Note(note)] if note.contains("applied")),
            "the answer is a note, never a frame: {reads:?}"
        );

        reader.clear();
        sends.clear();
        reader.push(&json_line(&hello(0)), 30, |r| on(r, &mut sends));
        assert_eq!(
            sends,
            [DEVICE_LOG_LEVEL_REQUEST_ID],
            "asked again after a reopen"
        );
    }

    #[test]
    fn no_dev_log_level_never_asks() {
        let mut reader = WireReader::new(false);
        reader.push(&json_line(&hello(0)), 0, |r| {
            assert!(!matches!(r, WireRead::Send(_)));
        });
    }

    fn notes_of(reader: &mut WireReader, bytes: &[u8], now_ms: u64) -> Vec<String> {
        let mut notes = Vec::new();
        reader.push(bytes, now_ms, |r| {
            if let WireRead::Note(note) = r {
                notes.push(note);
            }
        });
        notes
    }

    fn json_line(message: &WireServerMessage) -> Vec<u8> {
        format!("M!{}\n", lpc_wire::json::to_string(message).unwrap()).into_bytes()
    }

    /// A packed reply coded against a fresh table: the first frame after the
    /// board's reset, which a reader takes in any state.
    fn packed(message: &WireServerMessage) -> Vec<u8> {
        packed_on(&mut lp_json_pack_table(), message)
    }

    /// The next packed reply of a link whose board table is `table`.
    fn packed_on(table: &mut lpc_wire::LearnedTable, message: &WireServerMessage) -> Vec<u8> {
        let mut framed = vec![0u8; 4096];
        let n = lpc_wire::ser_learned_frame_to(&mut framed, table, message).unwrap();
        framed.truncate(n);
        framed
    }

    fn lp_json_pack_table() -> lpc_wire::LearnedTable {
        lpc_wire::LearnedTable::default()
    }

    fn hello(pack_format: u8) -> WireServerMessage {
        let hello = ServerHello {
            proto: WIRE_PROTO_VERSION,
            build: BuildFacts {
                features: vec![],
                package: "fw-esp32c6".to_string(),
                commit: "unknown".to_string(),
                dirty: false,
                profile: "release-esp32".to_string(),
            },
            hardware: HardwareFacts::default(),
            device_uid: None,
            pack_format,
            auth: lpc_wire::HelloAuth::TRUSTED,
        };
        WireServerMessage::new(0, ServerMsgBody::Hello(hello))
    }

    fn answer(encoding: WireEncoding) -> WireServerMessage {
        WireServerMessage::new(
            PACK_OPT_IN_REQUEST_ID,
            ServerMsgBody::SetEncoding { encoding },
        )
    }

    fn other(id: u64) -> WireServerMessage {
        WireServerMessage::new(id, ServerMsgBody::StopAllProjects)
    }
}
