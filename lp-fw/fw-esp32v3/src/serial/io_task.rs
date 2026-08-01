//! I/O task for the classic ESP32's UART0 host link.
//!
//! Responsibilities are the S3's:
//! - Drain the outgoing log/message queue and write it (prefixed lines).
//! - Drain accountable server write requests, serialize to JSON, write, and
//!   report the outcome back to the transport.
//! - Read available bytes, split on newlines, push `M!` lines to the incoming
//!   queue.
//!
//! ## Two divergences from `fw-esp32s3/src/serial/io_task.rs`
//!
//! **1. No connection monitor.** The S3 polls the USB-Serial-JTAG SOF bit to
//! tell "cable plugged" from "host application draining", because on that chip
//! a host that stops reading makes every write time out, which stalls the task
//! and starves the watchdog. A UART has neither signal and neither problem:
//! the CH340K clocks bytes onto the wire at line rate whether or not anything
//! is listening, so a write always completes and there is nothing to latch.
//! `UsbConnectionMonitor` and the probe-write self-healing path have no
//! counterpart here; the write timeout below survives only as a backstop
//! against a wedged peripheral.
//!
//! **2. RX is drained *between* TX chunks** ([`UartLink::write_chunked`]).
//! On USB-Serial-JTAG a 16 KiB `ProjectRead` frame leaves in a few
//! milliseconds; at 115200 baud it takes ~1.4 s, and UART0's RX FIFO is 128
//! bytes — about 11 ms of line time. Draining RX only at the top of the loop
//! would overflow the FIFO and silently lose whatever the host sent during a
//! long write. Hence [`WRITE_CHUNK_SIZE`] is sized in *line time*, not in
//! syscall overhead.
//!
//! ## Known duplication
//!
//! The JSON-serialization half below (`StackJsonWriter`,
//! `timed_write_server_msg`, `server_message_detail`) is a third copy of code
//! that is already duplicated between fw-esp32c6 and fw-esp32s3. It is
//! chip-agnostic and belongs in `fw-esp32-common`; lifting it would change
//! two shipping firmwares, which is not this phase's job. Kept diffable
//! against the S3's copy on purpose.

extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use esp_hal::Async;
use esp_hal::uart::{Uart, UartRx, UartTx};
use fw_core::message_router::MessageRouter;
use ser_write_json::SerWrite;

/// Static message channels for MessageRouter
static INCOMING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
static OUTGOING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();

/// Accountable server write requests.
///
/// The server transport submits one message here, then waits on
/// `SERVER_WRITE_RESULT`. This keeps `ServerTransport::send().await` aligned
/// with actual write completion instead of a best-effort task handoff.
///
/// Each request carries a wrapping `u32` generation token that `io_task` echoes
/// back on the result channel, so `transport.send()` can discard a result
/// orphaned by a cancelled send instead of trusting arrival order.
static SERVER_WRITE_REQUEST: Channel<
    CriticalSectionRawMutex,
    (u32, lpc_wire::WireServerMessage),
    1,
> = Channel::new();

static SERVER_WRITE_RESULT: Channel<
    CriticalSectionRawMutex,
    (u32, Result<(), lpc_wire::TransportError>),
    1,
> = Channel::new();

/// Write timeout per chunk. A UART TX FIFO drains at line rate unconditionally,
/// so this is not a host-liveness signal the way the S3's is — it is a
/// backstop against a wedged peripheral, sized to be far above the ~6 ms a
/// full chunk costs at 115200 baud.
const WRITE_TIMEOUT: Duration = Duration::from_millis(250);

/// Chunk size for large writes, in *line time* rather than syscall overhead:
/// 64 bytes is ~5.6 ms at 115200 baud, which is how often RX gets drained
/// during a long write (see the module docs). UART0's 128-byte RX FIFO holds
/// ~11 ms of incoming line, so this leaves a 2x margin against overflow.
const WRITE_CHUNK_SIZE: usize = 64;

/// RX drain buffer. One FIFO's worth, so a single `read_buffered` empties a
/// full FIFO.
const READ_CHUNK_SIZE: usize = 128;

/// The split UART plus the partial-line buffer, held together because writing
/// and reading interleave (see the module docs).
struct UartLink {
    rx: UartRx<'static, Async>,
    tx: UartTx<'static, Async>,
    read_buffer: Vec<u8>,
}

impl UartLink {
    fn new(uart: Uart<'static, Async>) -> Self {
        let (rx, tx) = uart.split();
        Self {
            rx,
            tx,
            read_buffer: Vec::new(),
        }
    }

    /// Move whatever the RX FIFO already holds into the line buffer and push
    /// any complete `M!` lines to the incoming queue.
    ///
    /// Non-blocking by construction: `read_buffered` returns what is there and
    /// never waits, which is the read semantic the C6 and S3 adapters also
    /// present. It is deliberately not the async `read()` — that one is
    /// cancellation-safe but parks on an RX-timeout interrupt that the classic
    /// ESP32 cannot clear while the FIFO is non-empty (esp-hal notes the
    /// erratum in `read_exact_async`), and polling sidesteps the question
    /// entirely at 1 ms granularity.
    fn poll_rx(&mut self, router: &MessageRouter) {
        let mut temp = [0u8; READ_CHUNK_SIZE];
        loop {
            match self.rx.read_buffered(&mut temp) {
                Ok(0) => break,
                Ok(n) => {
                    self.read_buffer.extend_from_slice(&temp[..n]);
                    // A short read means the FIFO is empty; anything else and
                    // there may be more waiting.
                    if n < temp.len() {
                        break;
                    }
                }
                Err(error) => {
                    // Overflow/parity/framing. The FIFO is reset by esp-hal on
                    // overflow; drop the partial line rather than splice two
                    // halves of different messages together.
                    log::warn!("[io_task] UART RX error: {error:?}; dropping partial line");
                    self.read_buffer.clear();
                    break;
                }
            }
        }
        process_read_buffer(&mut self.read_buffer, router);
    }

    /// Write everything in `data`, draining RX between chunks.
    ///
    /// Returns false on a per-chunk timeout, which for a UART means the
    /// peripheral is wedged rather than that a host went away.
    async fn write_chunked(&mut self, data: &[u8], router: &MessageRouter) -> bool {
        use embassy_futures::select::{Either, select};
        let mut offset = 0;
        while offset < data.len() {
            let chunk_end = (offset + WRITE_CHUNK_SIZE).min(data.len());
            match select(
                Timer::after(WRITE_TIMEOUT),
                self.tx.write_all(&data[offset..chunk_end]),
            )
            .await
            {
                Either::First(_) => {
                    log::warn!(
                        "[io_task] UART TX timed out after {offset} of {} B",
                        data.len()
                    );
                    return false;
                }
                Either::Second(Err(error)) => {
                    log::warn!("[io_task] UART TX error: {error:?}");
                    return false;
                }
                Either::Second(Ok(())) => {}
            }
            offset = chunk_end;
            self.poll_rx(router);
        }
        true
    }
}

struct StackJsonWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
}

#[derive(Debug)]
struct StackJsonError;

impl core::fmt::Display for StackJsonError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("stack JSON buffer full")
    }
}

impl<'a> StackJsonWriter<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }

    fn bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl ser_write_json::SerWrite for StackJsonWriter<'_> {
    type Error = StackJsonError;

    fn write(&mut self, buf: &[u8]) -> Result<(), StackJsonError> {
        let end = self.len.checked_add(buf.len()).ok_or(StackJsonError)?;
        if end > self.buf.len() {
            return Err(StackJsonError);
        }
        self.buf[self.len..end].copy_from_slice(buf);
        self.len = end;
        Ok(())
    }
}

/// I/O task for the UART0 host link.
///
/// Runs independently of the server loop and converts between UART bytes and
/// `M!`-prefixed JSON lines.
///
/// # Arguments
///
/// * `uart` - UART0, already configured at 115200 8N1 by `init_board` (which
///   is also where the baud divisor `esp-println` piggybacks on gets set).
#[embassy_executor::task]
pub async fn io_task(uart: Uart<'static, esp_hal::Blocking>) {
    let router = MessageRouter::new(&INCOMING_MSG, &OUTGOING_MSG);
    let mut link = UartLink::new(uart.into_async());

    Timer::after(Duration::from_millis(100)).await;

    loop {
        drain_server_write_request(&mut link, &router).await;
        drain_outgoing_messages(&router, &mut link).await;
        link.poll_rx(&router);

        Timer::after(Duration::from_millis(1)).await;
    }
}

/// Drain the outgoing log/message queue. Always consumes, so the server loop
/// never blocks on a full channel.
async fn drain_outgoing_messages(router: &MessageRouter, link: &mut UartLink) {
    let receiver = router.outgoing().receiver();
    while let Ok(msg) = receiver.try_receive() {
        if !link.write_chunked(b"\n", router).await {
            break;
        }
        if !link.write_chunked(msg.as_bytes(), router).await {
            break;
        }
    }
}

/// Drain accountable server write requests.
async fn drain_server_write_request(link: &mut UartLink, router: &MessageRouter) {
    let receiver = SERVER_WRITE_REQUEST.receiver();
    let Ok((generation, msg)) = receiver.try_receive() else {
        return;
    };

    let result = timed_write_server_msg(link, msg, router).await;
    // Echo the request generation so `transport.send()` can discard any stale
    // result left over from a cancelled send.
    SERVER_WRITE_RESULT
        .sender()
        .send((generation, result))
        .await;
}

async fn timed_write_server_msg(
    link: &mut UartLink,
    msg: lpc_wire::WireServerMessage,
    router: &MessageRouter,
) -> Result<(), lpc_wire::TransportError> {
    let result = timed_write_full_server_msg(link, msg, router).await;
    if result.is_err() {
        // If a write failed part-way through a JSON frame, separate the next
        // frame so host parsers can recover instead of concatenating two `M!`
        // messages.
        let _ = link.write_chunked(b"\n", router).await;
    }
    result
}

async fn timed_write_full_server_msg(
    link: &mut UartLink,
    msg: lpc_wire::WireServerMessage,
    router: &MessageRouter,
) -> Result<(), lpc_wire::TransportError> {
    // Same ~16.7 KiB stack buffer as fw-esp32s3, and the same TODO: it lives
    // as async-fn state and is paid even for tiny acks. On this chip that is a
    // materially larger share of a 192 KB DRAM budget than it is on the S3's
    // ~342 KB, so it is the first place to look if `.bss` needs to shrink.
    const SERVER_MSG_FRAMING_BYTES: usize = 16;
    const SERVER_MSG_JSON_BUFFER_SIZE: usize =
        lpc_wire::PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES + SERVER_MSG_FRAMING_BYTES;
    let mut buf = [0u8; SERVER_MSG_JSON_BUFFER_SIZE];
    let mut writer = StackJsonWriter::new(&mut buf);
    if writer.write(b"\nM!").is_err() {
        log::warn!("[io_task] server message prefix exceeded JSON buffer");
        return Err(lpc_wire::TransportError::Serialization(
            "server message prefix exceeded JSON buffer".into(),
        ));
    }
    // Erased writer: shares one serializer instantiation per wire type with the
    // frame-budget measurement path (lpc_wire::ser_write_json_len), instead of
    // emitting a second copy of every type's serializer for this sink.
    if lpc_wire::ser_write_json_to(&mut writer, &msg).is_err() {
        let detail = server_message_detail(&msg);
        log::warn!(
            "[io_task] server message id={} {} exceeded JSON buffer size={} frame_budget={}; write failed",
            msg.id,
            detail,
            SERVER_MSG_JSON_BUFFER_SIZE,
            lpc_wire::PROJECT_READ_FRAME_MAX_BYTES
        );
        return Err(lpc_wire::TransportError::Serialization(format!(
            "server message id={} {} exceeded JSON buffer",
            msg.id, detail
        )));
    }
    if writer.write(b"\n").is_err() {
        let detail = server_message_detail(&msg);
        log::warn!(
            "[io_task] server message id={} {} suffix exceeded JSON buffer size={}; write failed",
            msg.id,
            detail,
            SERVER_MSG_JSON_BUFFER_SIZE
        );
        return Err(lpc_wire::TransportError::Serialization(format!(
            "server message id={} {} suffix exceeded JSON buffer",
            msg.id, detail
        )));
    }
    let id = msg.id;
    if link.write_chunked(writer.bytes(), router).await {
        Ok(())
    } else {
        Err(lpc_wire::TransportError::Other(format!(
            "server message id={id} UART write timed out or failed"
        )))
    }
}

fn server_message_detail(msg: &lpc_wire::WireServerMessage) -> String {
    match &msg.msg {
        lpc_wire::server::ServerMsgBody::Hello(hello) => {
            format!("Hello proto={}", hello.proto)
        }
        lpc_wire::server::ServerMsgBody::Filesystem(_) => "Filesystem".into(),
        lpc_wire::server::ServerMsgBody::LoadProject { .. } => "LoadProject".into(),
        lpc_wire::server::ServerMsgBody::UnloadProject => "UnloadProject".into(),
        lpc_wire::server::ServerMsgBody::ProjectRead { events } => format!(
            "ProjectRead seq={} fin={} events={}",
            msg.seq,
            msg.fin,
            events.len()
        ),
        lpc_wire::server::ServerMsgBody::ProjectCommand { .. } => "ProjectCommand".into(),
        lpc_wire::server::ServerMsgBody::ListAvailableProjects { projects } => {
            format!("ListAvailableProjects projects={}", projects.len())
        }
        lpc_wire::server::ServerMsgBody::ListLoadedProjects { projects } => {
            format!("ListLoadedProjects projects={}", projects.len())
        }
        lpc_wire::server::ServerMsgBody::StopAllProjects => "StopAllProjects".into(),
        lpc_wire::server::ServerMsgBody::SetLogLevel => "SetLogLevel".into(),
        lpc_wire::server::ServerMsgBody::Log { level, .. } => {
            format!("Log level={level:?}")
        }
        lpc_wire::server::ServerMsgBody::Heartbeat {
            frame_count,
            loaded_projects,
            ..
        } => format!(
            "Heartbeat frame_count={frame_count} loaded_projects={}",
            loaded_projects.len()
        ),
        lpc_wire::server::ServerMsgBody::Error { .. } => "Error".into(),
    }
}

/// Process the read buffer and extract complete lines.
///
/// Looks for newlines, extracts lines starting with `M!`, and pushes them to
/// the incoming queue. Non-`M!` lines are ignored (they are the device's own
/// `esp_println!` output, which shares this UART).
fn process_read_buffer(read_buffer: &mut Vec<u8>, router: &MessageRouter) {
    while let Some(newline_pos) = read_buffer.iter().position(|&b| b == b'\n') {
        let line_bytes: Vec<u8> = read_buffer.drain(..=newline_pos).collect();

        if let Ok(line_str) = core::str::from_utf8(&line_bytes[..line_bytes.len() - 1])
            && line_str.starts_with("M!")
        {
            use alloc::string::ToString;
            if router
                .incoming()
                .sender()
                .try_send(line_str.to_string())
                .is_err()
            {
                log::warn!("[io_task] incoming queue full, dropping M! message");
            }
        }
    }
}

/// Get references to the static message channels.
///
/// Used by main.rs to create the `StreamingMessageRouterTransport`.
pub fn get_message_channels() -> (
    &'static Channel<CriticalSectionRawMutex, String, 32>,
    &'static Channel<CriticalSectionRawMutex, String, 32>,
) {
    (&INCOMING_MSG, &OUTGOING_MSG)
}

/// Get the accountable server write channels for StreamingMessageRouterTransport.
pub fn get_server_write_channels() -> (
    &'static Channel<CriticalSectionRawMutex, (u32, lpc_wire::WireServerMessage), 1>,
    &'static Channel<CriticalSectionRawMutex, (u32, Result<(), lpc_wire::TransportError>), 1>,
) {
    (&SERVER_WRITE_REQUEST, &SERVER_WRITE_RESULT)
}

/// Write log output to the outgoing channel (serial to host).
///
/// Lines go out without the `M!` prefix so the client prints them. When the
/// channel is full, log lines are dropped — logging that would recurse.
pub fn log_write_to_outgoing(msg: &str) {
    use alloc::string::ToString;
    let _ = OUTGOING_MSG.sender().try_send(msg.to_string());
}
